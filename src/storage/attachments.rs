use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    attachment_media::{AttachmentMediaKind, kind as attachment_kind, resolved_mime_type},
    attachment_paths::{
        canonical_blob_relative_path, image_rendition_cache_relative_directory, is_valid_sha256,
    },
    domain::{Attachment, AttachmentAudit, AttachmentRecoveryReport, NoteId},
    image_metadata::probe_image_attachment,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::{AttachmentOperation, Storage, StorageError, StorageResult, new_id, unix_timestamp};

pub(super) const COPY_BUFFER_SIZE: usize = 1024 * 1024;
pub(super) const ORPHAN_QUARANTINE_RETENTION_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AttachmentStagingMode {
    #[default]
    Copy,
    Clone,
}

impl AttachmentStagingMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Clone => "clone",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AttachmentImportProfile {
    pub(crate) byte_size: u64,
    pub(crate) managed_stage: Duration,
    pub(crate) staging_sync: Duration,
    pub(crate) hash_finalize: Duration,
    pub(crate) image_probe: Duration,
    pub(crate) operation_lock_wait: Duration,
    pub(crate) prepare_database: Duration,
    pub(crate) materialize: Duration,
    pub(crate) record_database: Duration,
    pub(crate) prepare_total: Duration,
    pub(crate) finalize_total: Duration,
    pub(crate) total: Duration,
    pub(crate) staging_mode: AttachmentStagingMode,
}

#[derive(Clone, Debug)]
pub(crate) struct ProfiledAttachmentImport {
    pub(crate) attachment: Attachment,
    pub(crate) profile: AttachmentImportProfile,
}

pub(crate) struct AuthorizedDerivedImageAttachment<'a> {
    note_id: &'a NoteId,
    source_attachment_id: &'a str,
    file_name: &'a str,
    mime_type: Option<&'a str>,
    source: &'a Path,
    image_edit_recipe: &'a str,
    authorization: &'a crate::security::PlaintextOperationPermit,
}

impl<'a> AuthorizedDerivedImageAttachment<'a> {
    pub(crate) fn new(
        note_id: &'a NoteId,
        source_attachment_id: &'a str,
        file_name: &'a str,
        mime_type: Option<&'a str>,
        source: &'a Path,
        image_edit_recipe: &'a str,
        authorization: &'a crate::security::PlaintextOperationPermit,
    ) -> Self {
        Self {
            note_id,
            source_attachment_id,
            file_name,
            mime_type,
            source,
            image_edit_recipe,
            authorization,
        }
    }
}

enum AttachmentWriteOrigin {
    Direct,
    Derived {
        source_attachment_id: String,
        image_edit_recipe: String,
    },
}

struct AuthorizedAttachmentWrite<'a> {
    note_id: &'a NoteId,
    file_name: &'a str,
    mime_type: Option<&'a str>,
    source: &'a Path,
    origin: AttachmentWriteOrigin,
    authorization: &'a crate::security::PlaintextOperationPermit,
}

#[derive(Clone, Copy)]
enum StagingBlobVerification {
    VerifiedDuringImport,
    VerifyFromDisk,
}

struct AttachmentBlobReference {
    is_revision: bool,
    is_protected: bool,
    record_id: String,
    owner_id: String,
    file_name: String,
    sha256: String,
    byte_size: i64,
}

impl AttachmentBlobReference {
    fn description(&self) -> String {
        if self.is_protected {
            format!(
                "protected blob {} for note {}",
                self.record_id, self.owner_id
            )
        } else if self.is_revision {
            format!(
                "revision attachment {} in revision {}",
                self.record_id, self.owner_id
            )
        } else {
            format!("attachment {} for note {}", self.record_id, self.owner_id)
        }
    }
}

struct AttachmentBlobExpectation {
    description: String,
    file_name: String,
    canonical_path: String,
    sha256: String,
    byte_size: u64,
}

pub(super) struct StagedAttachmentImport {
    pub(super) attachment_id: Option<String>,
    pub(super) file_name: String,
    pub(super) mime_type: Option<String>,
    pub(super) staging_relative_path: String,
    pub(super) staging_path: PathBuf,
    pub(super) owned_plaintext: Option<crate::security::OwnedPlaintext>,
    pub(super) sha256: String,
    pub(super) byte_size: u64,
}

impl Storage {
    pub(super) fn list_attachments_with_connection(
        &self,
        connection: &Connection,
        note_id: &NoteId,
    ) -> StorageResult<Vec<Attachment>> {
        let mut statement = connection.prepare(
            "SELECT id, note_id, file_name, mime_type, byte_size, sha256,
                    image_pixel_width, image_pixel_height, image_format, image_orientation,
                    image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at
             FROM attachments WHERE note_id = ?1 ORDER BY created_at, lower(file_name), id",
        )?;
        let rows = statement.query_map([note_id], attachment_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub fn list_attachments(&self, note_id: &NoteId) -> StorageResult<Vec<Attachment>> {
        if let Some(attachments) = self.protected_attachments_if_unlocked(note_id)? {
            return Ok(attachments);
        }
        let connection = self.connection()?;
        self.list_attachments_with_connection(&connection, note_id)
    }

    pub(super) fn list_attachments_with_authorization(
        &self,
        note_id: &NoteId,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Vec<Attachment>> {
        if authorization.note_id() != note_id {
            return Err(StorageError::Integrity(
                "attachment plaintext authorization belongs to a different note".to_string(),
            ));
        }
        if let Some(access) = authorization.access() {
            if let Some(attachments) = self.protected_attachments_with_access(note_id, access)? {
                return Ok(attachments);
            }
        } else {
            let connection = self.connection()?;
            if super::protection::protected_note_exists(&connection, note_id)? {
                return Err(StorageError::Validation(
                    "unlock this encrypted note to view its attachments".to_string(),
                ));
            }
        }
        let connection = self.connection()?;
        self.list_attachments_with_connection(&connection, note_id)
    }

    pub(super) fn list_referenced_attachments_with_connection(
        &self,
        connection: &Connection,
        note_id: &NoteId,
    ) -> StorageResult<Vec<Attachment>> {
        let mut statement = connection.prepare(
            "SELECT id, note_id, file_name, mime_type, byte_size, sha256,
                    image_pixel_width, image_pixel_height, image_format, image_orientation,
                    image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at
             FROM attachments
             WHERE note_id = ?1 AND embedded = 1
             ORDER BY created_at, lower(file_name), id",
        )?;
        let rows = statement.query_map([note_id], attachment_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(super) fn reconcile_document_attachments(
        &self,
        transaction: &Transaction<'_>,
        note_id: &NoteId,
        attachment_ids: &[String],
        pending_attachment_request_ids: &[String],
    ) -> StorageResult<(Vec<String>, bool)> {
        let document_ids = attachment_ids.iter().cloned().collect::<HashSet<_>>();
        let rows = {
            let mut statement = transaction.prepare(
                "SELECT id, file_name, embedded, source_attachment_id
                 FROM attachments
                 WHERE note_id = ?1
                 ORDER BY created_at, lower(file_name), id",
            )?;
            let rows = statement.query_map([note_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let known_ids = rows
            .iter()
            .map(|(attachment_id, _, _, _)| attachment_id.clone())
            .collect::<HashSet<_>>();
        if let Some(missing_id) = document_ids
            .iter()
            .find(|attachment_id| !known_ids.contains(*attachment_id))
        {
            return Err(StorageError::Validation(format!(
                "document references missing or foreign attachment {missing_id}"
            )));
        }

        let source_by_id = rows
            .iter()
            .map(|(attachment_id, _, _, source_id)| (attachment_id.clone(), source_id.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let mut desired_ids = document_ids.clone();
        let mut frontier = document_ids.iter().cloned().collect::<Vec<_>>();
        while let Some(attachment_id) = frontier.pop() {
            let Some(source_id) = source_by_id.get(&attachment_id).and_then(Clone::clone) else {
                continue;
            };
            if !known_ids.contains(&source_id) {
                return Err(StorageError::Validation(format!(
                    "image rendition {attachment_id} references missing or foreign source attachment {source_id}"
                )));
            }
            if desired_ids.insert(source_id.clone()) {
                frontier.push(source_id);
            }
        }

        let attachment_names = rows
            .iter()
            .filter(|(attachment_id, _, _, _)| document_ids.contains(attachment_id))
            .map(|(_, file_name, _, _)| file_name.clone())
            .collect::<Vec<_>>();
        let mut membership_changed = false;
        let now = unix_timestamp();
        for (attachment_id, _, embedded, _) in rows {
            let should_be_embedded = desired_ids.contains(&attachment_id);
            if embedded == should_be_embedded {
                continue;
            }
            membership_changed = true;
            if should_be_embedded {
                transaction.execute(
                    "UPDATE attachments
                     SET embedded = 1, unreferenced_at = NULL, updated_at = ?1
                     WHERE id = ?2 AND note_id = ?3 AND embedded = 0",
                    params![now, attachment_id, note_id],
                )?;
            } else {
                transaction.execute(
                    "UPDATE attachments
                     SET embedded = 0, unreferenced_at = ?1, updated_at = ?1
                     WHERE id = ?2 AND note_id = ?3 AND embedded = 1",
                    params![now, attachment_id, note_id],
                )?;
            }
        }

        // Request-scoped ingest rows are the durable bridge between a canonical
        // pending insertion anchor and its stable attachment IDs. Once the
        // document has actually committed a finalized attachment, that bridge
        // has served its purpose and can be removed atomically with this save.
        transaction.execute(
            "DELETE FROM attachment_ingest_jobs
             WHERE note_id = ?1
               AND request_id IS NOT NULL
               AND attachment_id IN (
                   SELECT id FROM attachments WHERE note_id = ?1 AND embedded = 1
               )",
            [note_id],
        )?;

        // A request-scoped row can also become stale when the user removes its
        // canonical pending anchor after finalization. Keep only bridges still
        // represented by the saved document; unfinalized jobs are deliberately
        // left alone because their managed staging file may still be hashing.
        let pending_request_ids = pending_attachment_request_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let finalized_request_rows = {
            let mut statement = transaction.prepare(
                "SELECT jobs.id, jobs.request_id
                 FROM attachment_ingest_jobs AS jobs
                 INNER JOIN attachments AS finalized ON finalized.id = jobs.attachment_id
                 WHERE jobs.note_id = ?1
                   AND jobs.request_id IS NOT NULL
                   AND finalized.embedded = 0",
            )?;
            let rows = statement.query_map([note_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (ingest_id, request_id) in finalized_request_rows {
            if !pending_request_ids.contains(request_id.as_str()) {
                transaction.execute(
                    "DELETE FROM attachment_ingest_jobs WHERE id = ?1",
                    [ingest_id],
                )?;
            }
        }
        Ok((attachment_names, membership_changed))
    }

    pub fn add_attachment(&self, note_id: &NoteId, source: &Path) -> StorageResult<Attachment> {
        self.add_attachment_profiled(note_id, source)
            .map(|result| result.attachment)
    }

    pub(crate) fn add_attachment_profiled(
        &self,
        note_id: &NoteId,
        source: &Path,
    ) -> StorageResult<ProfiledAttachmentImport> {
        let prepared = self.prepare_attachment_import_profiled(note_id, source)?;
        self.finalize_prepared_attachment_import_profiled(prepared)
    }

    pub fn add_attachment_file_as(
        &self,
        note_id: &NoteId,
        file_name: &str,
        mime_type: Option<&str>,
        source: &Path,
    ) -> StorageResult<Attachment> {
        let authorization = self.acquire_plaintext_operation_for_note(note_id)?;
        self.add_attachment_file_as_authorized(
            note_id,
            file_name,
            mime_type,
            source,
            &authorization,
        )
    }

    pub(super) fn add_attachment_file_as_authorized(
        &self,
        note_id: &NoteId,
        file_name: &str,
        mime_type: Option<&str>,
        source: &Path,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Attachment> {
        self.add_attachment_file_internal_authorized(AuthorizedAttachmentWrite {
            note_id,
            file_name,
            mime_type,
            source,
            origin: AttachmentWriteOrigin::Direct,
            authorization,
        })
    }

    pub(super) fn add_derived_image_attachment_file_authorized(
        &self,
        request: AuthorizedDerivedImageAttachment<'_>,
    ) -> StorageResult<Attachment> {
        let AuthorizedDerivedImageAttachment {
            note_id,
            source_attachment_id,
            file_name,
            mime_type,
            source,
            image_edit_recipe,
            authorization,
        } = request;
        self.validate_note_accepts_attachment_with_authorization(note_id, authorization)?;
        let source_attachment_id = self.resolve_image_edit_source_with_authorization(
            note_id,
            source_attachment_id,
            authorization,
        )?;
        let image_edit_recipe = canonical_image_edit_recipe(image_edit_recipe)?;
        self.add_attachment_file_internal_authorized(AuthorizedAttachmentWrite {
            note_id,
            file_name,
            mime_type,
            source,
            origin: AttachmentWriteOrigin::Derived {
                source_attachment_id,
                image_edit_recipe,
            },
            authorization,
        })
    }

    fn add_attachment_file_internal_authorized(
        &self,
        request: AuthorizedAttachmentWrite<'_>,
    ) -> StorageResult<Attachment> {
        let AuthorizedAttachmentWrite {
            note_id,
            file_name,
            mime_type,
            source,
            origin,
            authorization,
        } = request;
        if !source.is_file() {
            return Err(StorageError::Validation(
                "staged attachment is not a file".to_string(),
            ));
        }
        self.validate_note_accepts_attachment_with_authorization(note_id, authorization)?;
        let (file_name, mime_type) = canonical_attachment_identity(file_name, mime_type)?;
        let (staging_relative_path, staging_path) = self.new_attachment_staging_path()?;
        let mut owned_plaintext =
            self.reserve_attachment_owned_plaintext(note_id, &staging_relative_path)?;
        if owned_plaintext.path() != staging_path {
            return Err(StorageError::Integrity(
                "attachment staging ownership path mismatch".to_string(),
            ));
        }
        let staged_result = owned_plaintext.copy_and_hash_from(source);
        let (sha256, byte_size) = match staged_result {
            Ok(result) => result,
            Err(error) => {
                return Err(
                    self.cleanup_owned_plaintext_after_error(&mut owned_plaintext, error.into())
                );
            }
        };
        let (source_attachment_id, image_edit_recipe) = match origin {
            AttachmentWriteOrigin::Direct => (None, None),
            AttachmentWriteOrigin::Derived {
                source_attachment_id,
                image_edit_recipe,
            } => (Some(source_attachment_id), Some(image_edit_recipe)),
        };
        self.finish_staged_attachment_import(
            note_id,
            authorization,
            StagedAttachmentImport {
                attachment_id: None,
                file_name,
                mime_type,
                staging_relative_path,
                staging_path,
                owned_plaintext: Some(owned_plaintext),
                sha256,
                byte_size,
            },
            source_attachment_id,
            image_edit_recipe,
        )
    }

    fn resolve_image_edit_source_with_authorization(
        &self,
        note_id: &NoteId,
        source_attachment_id: &str,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<String> {
        if authorization.note_id() != note_id {
            return Err(StorageError::Integrity(
                "image-edit plaintext authorization belongs to a different note".to_string(),
            ));
        }
        if let Some(access) = authorization.access() {
            let attachments = self
                .protected_attachments_with_access(note_id, access)?
                .ok_or_else(|| {
                    StorageError::Integrity(
                        "Vault authorization exists for a note without a protected graph"
                            .to_string(),
                    )
                })?;
            let mut current = source_attachment_id.to_string();
            let mut visited = HashSet::new();
            for _ in 0..32 {
                if !visited.insert(current.clone()) {
                    return Err(StorageError::Integrity(
                        "image edit source chain contains a cycle".to_string(),
                    ));
                }
                let source = attachments
                    .iter()
                    .find(|attachment| attachment.id == current)
                    .ok_or_else(|| {
                        StorageError::Validation(
                            "image edit source attachment no longer exists".to_string(),
                        )
                    })?;
                if attachment_kind(&source.file_name, source.mime_type.as_deref())
                    != AttachmentMediaKind::Image
                {
                    return Err(StorageError::Validation(
                        "image edit source is not an image".to_string(),
                    ));
                }
                match source.source_attachment_id.as_ref() {
                    Some(parent) => current = parent.clone(),
                    None => return Ok(current),
                }
            }
            return Err(StorageError::Integrity(
                "image edit source chain is unexpectedly deep".to_string(),
            ));
        }

        let connection = self.connection()?;
        let mut current = source_attachment_id.to_string();
        let mut visited = HashSet::new();
        for _ in 0..32 {
            if !visited.insert(current.clone()) {
                return Err(StorageError::Integrity(
                    "image edit source chain contains a cycle".to_string(),
                ));
            }
            let row = connection
                .query_row(
                    "SELECT note_id, file_name, mime_type, source_attachment_id
                     FROM attachments WHERE id = ?1",
                    [&current],
                    |row| {
                        Ok((
                            row.get::<_, NoteId>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()?;
            let Some((source_note_id, file_name, mime_type, parent_source)) = row else {
                return Err(StorageError::Validation(
                    "image edit source attachment no longer exists".to_string(),
                ));
            };
            if &source_note_id != note_id {
                return Err(StorageError::Validation(
                    "image edit source belongs to another note".to_string(),
                ));
            }
            if attachment_kind(&file_name, mime_type.as_deref()) != AttachmentMediaKind::Image {
                return Err(StorageError::Validation(
                    "image edit source is not an image".to_string(),
                ));
            }
            match parent_source {
                Some(parent) => current = parent,
                None => return Ok(current),
            }
        }
        Err(StorageError::Integrity(
            "image edit source chain is unexpectedly deep".to_string(),
        ))
    }

    fn new_attachment_staging_path(&self) -> StorageResult<(String, PathBuf)> {
        let incoming_directory = self.attachments_directory.join(".incoming");
        fs::create_dir_all(&incoming_directory)?;
        let staging_name = format!("{}.tmp", new_id("blob"));
        Ok((
            format!(".incoming/{staging_name}"),
            incoming_directory.join(staging_name),
        ))
    }

    fn finish_staged_attachment_import(
        &self,
        note_id: &NoteId,
        authorization: &crate::security::PlaintextOperationPermit,
        staged: StagedAttachmentImport,
        source_attachment_id: Option<String>,
        image_edit_recipe: Option<String>,
    ) -> StorageResult<Attachment> {
        let mut profile = AttachmentImportProfile::default();
        self.finish_staged_attachment_import_profiled(
            note_id,
            authorization,
            staged,
            source_attachment_id,
            image_edit_recipe,
            &mut profile,
        )
    }

    pub(super) fn finish_staged_attachment_import_profiled(
        &self,
        note_id: &NoteId,
        authorization: &crate::security::PlaintextOperationPermit,
        mut staged: StagedAttachmentImport,
        source_attachment_id: Option<String>,
        image_edit_recipe: Option<String>,
        profile: &mut AttachmentImportProfile,
    ) -> StorageResult<Attachment> {
        validate_sha256(&staged.sha256)?;

        let phase = Instant::now();
        let image_metadata = probe_image_attachment(
            &staged.staging_path,
            &staged.file_name,
            staged.mime_type.as_deref(),
        );
        profile.image_probe = phase.elapsed();

        let now = unix_timestamp();
        let attachment = Attachment {
            id: staged
                .attachment_id
                .clone()
                .unwrap_or_else(|| new_id("att")),
            note_id: note_id.clone(),
            file_name: staged.file_name,
            mime_type: staged.mime_type,
            byte_size: i64::try_from(staged.byte_size).map_err(|_| {
                StorageError::Validation("attachment is too large to index".to_string())
            })?,
            sha256: staged.sha256,
            image_pixel_width: image_metadata.pixel_width,
            image_pixel_height: image_metadata.pixel_height,
            image_format: image_metadata.format,
            image_orientation: image_metadata.orientation,
            image_has_alpha: image_metadata.has_alpha,
            image_animated: image_metadata.animated,
            source_attachment_id,
            image_edit_recipe,
            embedded: false,
            unreferenced_at: Some(now),
            created_at: now,
            updated_at: now,
        };

        // Protected notes commit attachment metadata into the encrypted
        // AttachmentIndex and encrypt only this blob. Acquire the Vault reader
        // before the storage lock inside that helper to preserve the canonical
        // Lock-vs-reader ordering.
        if let Some(protected_attachment) = self
            .persist_protected_attachment_from_staging_if_unlocked(
                &attachment,
                &staged.staging_path,
                authorization,
            )?
        {
            if let Some(owned_plaintext) = staged.owned_plaintext.as_mut() {
                self.discard_owned_plaintext(owned_plaintext)?;
            }
            return Ok(protected_attachment);
        }

        let phase = Instant::now();
        let _operation_guard = self.lock_operations()?;
        profile.operation_lock_wait = phase.elapsed();

        let phase = Instant::now();
        let operation = match self.prepare_attachment_add(
            &attachment,
            &staged.staging_relative_path,
            staged.owned_plaintext.as_mut(),
        ) {
            Ok(operation) => operation,
            Err(error) => {
                // Durable two-phase ingests remain owned by their ingest row.
                // Direct imports remain owned by `managed_plaintext_staging`;
                // the caller performs fail-closed cleanup if this preparation
                // fails before ownership transfer.
                return Err(error);
            }
        };
        profile.prepare_database = phase.elapsed();

        let (finished, materialize, record_database) = self.finish_attachment_add_profiled(
            &operation,
            &attachment,
            StagingBlobVerification::VerifiedDuringImport,
        )?;
        profile.materialize = materialize;
        profile.record_database = record_database;
        if !finished {
            return Err(StorageError::Integrity(format!(
                "attachment {} has no recoverable staged or final blob",
                attachment.id
            )));
        }
        Ok(attachment)
    }

    pub fn remove_attachment(&self, attachment_id: &str) -> StorageResult<()> {
        if self
            .remove_protected_attachment_if_unlocked(attachment_id)?
            .is_some()
        {
            return Ok(());
        }

        // Resolve only the derivative scope before entering the coordinator.
        // The attachment is re-read and fully validated under the returned
        // derivative-cleanup + storage boundary, so a concurrent Protect or
        // competing removal cannot make this preflight authoritative.
        let connection = self.read_only_connection()?;
        let note_id = connection
            .query_row(
                "SELECT note_id FROM attachments WHERE id = ?1",
                [attachment_id],
                |row| row.get::<_, crate::domain::NoteId>(0),
            )
            .optional()?;
        drop(connection);
        let Some(note_id) = note_id else {
            return Ok(());
        };
        let _cleanup_guard = self.begin_plaintext_derivative_cleanup(&note_id)?;
        let Some((operation, attachment)) = self.prepare_attachment_remove(attachment_id)? else {
            return Ok(());
        };
        if attachment.note_id != note_id {
            return Err(StorageError::Integrity(
                "attachment ownership changed during derivative cleanup admission".to_string(),
            ));
        }
        self.finish_attachment_remove(&operation, &attachment)
    }
    pub(super) fn prepare_attachment_add(
        &self,
        attachment: &Attachment,
        staging_relative_path: &str,
        mut owned_plaintext: Option<&mut crate::security::OwnedPlaintext>,
    ) -> StorageResult<AttachmentOperation> {
        validate_staging_relative_path(staging_relative_path)?;
        validate_sha256(&attachment.sha256)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let note_trashed = transaction
            .query_row(
                "SELECT trashed FROM notes WHERE id = ?1",
                [&attachment.note_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let Some(note_trashed) = note_trashed else {
            return Err(StorageError::Validation(
                "note no longer exists".to_string(),
            ));
        };
        if super::protection::protected_note_exists(&transaction, &attachment.note_id)? {
            return Err(StorageError::Validation(
                "note encryption changed while the attachment was being added; retry after unlocking"
                    .to_string(),
            ));
        }
        if note_trashed != 0 {
            return Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before adding attachments"
                    .to_string(),
            ));
        }
        if let Some(source_id) = attachment.source_attachment_id.as_deref() {
            if source_id == attachment.id {
                return Err(StorageError::Validation(
                    "an edited image cannot use itself as its source".to_string(),
                ));
            }
            let source_note_id = transaction
                .query_row(
                    "SELECT note_id FROM attachments WHERE id = ?1",
                    [source_id],
                    |row| row.get::<_, NoteId>(0),
                )
                .optional()?;
            if source_note_id.as_ref() != Some(&attachment.note_id) {
                return Err(StorageError::Validation(
                    "image edit source attachment is missing or belongs to another note"
                        .to_string(),
                ));
            }
            if attachment.image_edit_recipe.is_none() {
                return Err(StorageError::Validation(
                    "edited image is missing its edit recipe".to_string(),
                ));
            }
        } else if attachment.image_edit_recipe.is_some() {
            return Err(StorageError::Validation(
                "image edit recipe requires a source attachment".to_string(),
            ));
        }
        let operation = self.insert_attachment_operation(
            &transaction,
            "add",
            attachment,
            Some(staging_relative_path),
        )?;
        if let Some(owned_plaintext) = owned_plaintext.as_deref_mut() {
            self.transfer_owned_plaintext_stage(&transaction, owned_plaintext)?;
        }
        transaction.commit()?;
        if let Some(owned_plaintext) = owned_plaintext {
            self.complete_owned_plaintext_transfer(owned_plaintext)?;
        }
        Ok(operation)
    }
    fn finish_attachment_add(
        &self,
        operation: &AttachmentOperation,
        attachment: &Attachment,
        staging_verification: StagingBlobVerification,
    ) -> StorageResult<bool> {
        self.finish_attachment_add_profiled(operation, attachment, staging_verification)
            .map(|(finished, _, _)| finished)
    }

    fn finish_attachment_add_profiled(
        &self,
        operation: &AttachmentOperation,
        attachment: &Attachment,
        staging_verification: StagingBlobVerification,
    ) -> StorageResult<(bool, Duration, Duration)> {
        let phase = Instant::now();
        let materialized =
            self.materialize_attachment_blob(operation, attachment, staging_verification)?;
        let materialize = phase.elapsed();
        if !materialized {
            return Ok((false, materialize, Duration::ZERO));
        }

        let phase = Instant::now();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.upsert_attachment(&transaction, attachment)?;
        self.enqueue_sync_upsert(&transaction, "attachment", &attachment.id, attachment)?;
        transaction.execute(
            "DELETE FROM attachment_operations WHERE id = ?1",
            [&operation.id],
        )?;
        transaction.commit()?;
        Ok((true, materialize, phase.elapsed()))
    }
    pub(super) fn prepare_attachment_remove(
        &self,
        attachment_id: &str,
    ) -> StorageResult<Option<(AttachmentOperation, Attachment)>> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let attachment = transaction
            .query_row(
                "SELECT id, note_id, file_name, mime_type, byte_size, sha256,
                        image_pixel_width, image_pixel_height, image_format, image_orientation,
                        image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at
                 FROM attachments WHERE id = ?1",
                [attachment_id],
                attachment_from_row,
            )
            .optional()?;
        let Some(attachment) = attachment else {
            transaction.commit()?;
            return Ok(None);
        };
        let note_trashed: bool = transaction.query_row(
            "SELECT trashed FROM notes WHERE id = ?1",
            [&attachment.note_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )?;
        if super::protection::protected_note_exists(&transaction, &attachment.note_id)? {
            return Err(StorageError::Validation(
                "note encryption changed while the attachment was being removed; retry after unlocking"
                    .to_string(),
            ));
        }
        if note_trashed {
            return Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before removing attachments"
                    .to_string(),
            ));
        }
        if attachment.embedded {
            return Err(StorageError::Validation(
                "embedded attachments must be removed from the document; the stored file is retained for undo and version history"
                    .to_string(),
            ));
        }
        let retained_by_derived_image: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM attachments WHERE source_attachment_id = ?1 LIMIT 1)",
            [attachment_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )?;
        if retained_by_derived_image {
            return Err(StorageError::Validation(
                "source image is retained by an edited rendition".to_string(),
            ));
        }
        let operation =
            self.insert_attachment_operation(&transaction, "remove", &attachment, None)?;
        self.enqueue_sync_delete(&transaction, "attachment", attachment_id)?;
        transaction.execute("DELETE FROM attachments WHERE id = ?1", [attachment_id])?;
        super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
            &transaction,
            &attachment.sha256,
        )?;
        transaction.commit()?;
        Ok(Some((operation, attachment)))
    }
    pub(super) fn finish_attachment_remove(
        &self,
        operation: &AttachmentOperation,
        attachment: &Attachment,
    ) -> StorageResult<()> {
        self.quarantine_blob_if_unreferenced(&attachment.sha256)?;
        self.delete_attachment_operation(&operation.id)
    }
    pub(super) fn insert_attachment_operation(
        &self,
        transaction: &Transaction<'_>,
        operation_kind: &str,
        attachment: &Attachment,
        staging_relative_path: Option<&str>,
    ) -> StorageResult<AttachmentOperation> {
        if !matches!(operation_kind, "add" | "remove") {
            return Err(StorageError::Validation(format!(
                "unsupported attachment operation {operation_kind}"
            )));
        }
        validate_sha256(&attachment.sha256)?;
        if let Some(staging) = staging_relative_path {
            validate_staging_relative_path(staging)?;
        }
        let now = unix_timestamp();
        let operation = AttachmentOperation {
            id: new_id("attop"),
            operation: operation_kind.to_string(),
            attachment_id: attachment.id.clone(),
            note_id: attachment.note_id.clone(),
            attachment_json: serde_json::to_string(attachment)?,
            staging_relative_path: staging_relative_path.map(str::to_string),
            created_at: now,
            updated_at: now,
        };
        transaction.execute(
            "INSERT INTO attachment_operations(
                id, operation, attachment_id, note_id, attachment_json,
                staging_relative_path, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(attachment_id) DO UPDATE SET
                id = excluded.id,
                operation = excluded.operation,
                note_id = excluded.note_id,
                attachment_json = excluded.attachment_json,
                staging_relative_path = excluded.staging_relative_path,
                updated_at = excluded.updated_at",
            params![
                operation.id,
                operation.operation,
                operation.attachment_id,
                operation.note_id,
                operation.attachment_json,
                operation.staging_relative_path,
                operation.created_at,
                operation.updated_at
            ],
        )?;
        Ok(operation)
    }
    pub(super) fn delete_attachment_operation(&self, operation_id: &str) -> StorageResult<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM attachment_operations WHERE id = ?1",
            [operation_id],
        )?;
        Ok(())
    }
    pub(super) fn list_attachment_operations(&self) -> StorageResult<Vec<AttachmentOperation>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, operation, attachment_id, note_id, attachment_json,
                    staging_relative_path, created_at, updated_at
             FROM attachment_operations ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], attachment_operation_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    fn materialize_attachment_blob(
        &self,
        operation: &AttachmentOperation,
        attachment: &Attachment,
        staging_verification: StagingBlobVerification,
    ) -> StorageResult<bool> {
        let final_path = self.attachment_file_path(attachment)?;
        let expected_size = attachment.byte_size as u64;
        if crate::security::managed_path_is_present(&final_path)? {
            if final_path.metadata()?.len() == expected_size {
                let verified = self.attachment_blob_is_verified(
                    &attachment.sha256,
                    expected_size,
                    &final_path,
                );
                if verified {
                    self.discard_staged_attachment(operation)?;
                    return Ok(true);
                }

                let (checksum, size) = sha256_file(&final_path)?;
                if checksum == attachment.sha256 && size == expected_size {
                    self.mark_attachment_blob_verified(
                        &attachment.sha256,
                        expected_size,
                        &final_path,
                    );
                    self.discard_staged_attachment(operation)?;
                    return Ok(true);
                }
            }
            self.forget_verified_attachment_blob(&attachment.sha256);
        }

        let Some(staging_relative_path) = operation.staging_relative_path.as_deref() else {
            return Ok(false);
        };
        let staging_path = self.staging_attachment_path(staging_relative_path)?;
        if !crate::security::managed_path_is_present(&staging_path)? {
            return Ok(false);
        }
        if matches!(
            staging_verification,
            StagingBlobVerification::VerifiedDuringImport
        ) {
            // The import path has already hashed the exact managed staging
            // bytes and durably synced that file. The managed .incoming path is
            // not externally mutable through Rustnote, so re-hashing it here
            // would only add a second full-file read.
            // Recovery paths never take this branch: they verify staged bytes
            // from disk before replaying an interrupted operation.
            if staging_path.metadata()?.len() != expected_size {
                return Ok(false);
            }
        } else {
            let (checksum, size) = sha256_file(&staging_path)?;
            if checksum != attachment.sha256 || size != expected_size {
                return Ok(false);
            }
        }
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if crate::security::managed_path_is_present(&final_path)? {
            // Content-addressed attachment replacement is recoverable from the
            // still-durable attachment operation, so do not create an unowned
            // `.previous` plaintext sibling. Remove the invalid destination
            // strictly, then move the already-owned staging path into place.
            let removed = crate::native::privacy::remove_file(&final_path)?;
            if !removed && crate::security::managed_path_is_present(&final_path)? {
                return Err(StorageError::Integrity(format!(
                    "attachment destination is not a removable file: {}",
                    final_path.display()
                )));
            }
        }
        let mut owned_staging =
            crate::security::OwnedPlaintext::durable_attachment_operation_existing(
                staging_path.clone(),
                operation.id.clone(),
                operation.note_id.clone(),
            );
        owned_staging.rename_durable_attachment_operation(final_path.clone())?;
        sync_parent_directory(&final_path)?;
        self.mark_attachment_blob_verified(&attachment.sha256, expected_size, &final_path);
        Ok(true)
    }

    pub(super) fn discard_attachment_operation_staging_for_note(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<usize> {
        let operations = self
            .list_attachment_operations()?
            .into_iter()
            .filter(|operation| &operation.note_id == note_id)
            .collect::<Vec<_>>();
        let mut removed = 0;
        for operation in operations {
            if operation.operation != "add" {
                continue;
            }
            self.discard_staged_attachment(&operation)?;
            let attachment: Attachment =
                serde_json::from_str(&operation.attachment_json).map_err(|error| {
                    StorageError::Integrity(format!(
                        "attachment operation {} has invalid ownership metadata: {error}",
                        operation.id
                    ))
                })?;
            if attachment.note_id != *note_id {
                return Err(StorageError::Integrity(format!(
                    "attachment operation {} belongs to the wrong note",
                    operation.id
                )));
            }
            self.discard_unreferenced_attachment_operation_blob(&operation, &attachment)?;
            removed += 1;
        }
        Ok(removed)
    }

    fn discard_staged_attachment(&self, operation: &AttachmentOperation) -> StorageResult<()> {
        let Some(staging) = operation.staging_relative_path.as_deref() else {
            return Ok(());
        };
        let staging_path = self.staging_attachment_path(staging)?;
        if !crate::security::managed_path_is_present(&staging_path)? {
            return Ok(());
        }
        let mut owned = crate::security::OwnedPlaintext::durable_attachment_operation_existing(
            staging_path,
            operation.id.clone(),
            operation.note_id.clone(),
        );
        owned.remove_file()?;
        Ok(())
    }

    fn discard_unreferenced_attachment_operation_blob(
        &self,
        operation: &AttachmentOperation,
        attachment: &Attachment,
    ) -> StorageResult<()> {
        validate_sha256(&attachment.sha256)?;
        let connection = self.connection()?;
        let references: i64 = connection.query_row(
            "SELECT
                (SELECT COUNT(*) FROM attachments WHERE sha256 = ?1)
                + (SELECT COUNT(*) FROM revision_attachment_refs WHERE sha256 = ?1)
                + (SELECT COUNT(*) FROM vault_blob_refs WHERE sha256 = ?1)",
            [&attachment.sha256],
            |row| row.get(0),
        )?;
        drop(connection);
        if references != 0 {
            return Ok(());
        }

        // An add-operation row remains the cleanup authority after `.incoming`
        // has been renamed to its canonical hash path but before the attachment
        // DB transaction commits. Delete that unreferenced final-path state
        // before a note delete is allowed to cascade the operation row away.
        let final_path = self.attachment_file_path(attachment)?;
        if crate::security::managed_path_is_present(&final_path)? {
            let mut owned = crate::security::OwnedPlaintext::durable_attachment_operation_existing(
                final_path.clone(),
                operation.id.clone(),
                operation.note_id.clone(),
            );
            owned.remove_file()?;
            sync_parent_directory(&final_path)?;
            self.forget_verified_attachment_blob(&attachment.sha256);
        }
        Ok(())
    }

    fn attachment_blob_is_verified(&self, sha256: &str, byte_size: u64, path: &Path) -> bool {
        let Some(stamp) = attachment_blob_verification_stamp(path) else {
            return false;
        };
        self.verified_attachment_blobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(&(sha256.to_string(), byte_size, stamp))
    }

    fn mark_attachment_blob_verified(&self, sha256: &str, byte_size: u64, path: &Path) {
        let Some(stamp) = attachment_blob_verification_stamp(path) else {
            return;
        };
        let mut verified = self
            .verified_attachment_blobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        verified.retain(|(verified_sha256, _, _)| verified_sha256 != sha256);
        verified.insert((sha256.to_string(), byte_size, stamp));
    }

    fn forget_verified_attachment_blob(&self, sha256: &str) {
        self.verified_attachment_blobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|(verified_sha256, _, _)| verified_sha256 != sha256);
    }
    pub(super) fn staging_attachment_path(&self, relative_path: &str) -> StorageResult<PathBuf> {
        validate_staging_relative_path(relative_path)?;
        Ok(self.attachments_directory.join(relative_path))
    }
    pub(super) fn reconcile_attachment_storage(&self) -> StorageResult<AttachmentRecoveryReport> {
        // Recovery can delete attachment rows, canonical blobs, quarantine
        // artifacts, and persistent derivatives. Freeze all derivative
        // producers before taking storage mutation authority so a worker that
        // started earlier cannot republish a cache after cleanup.
        let _cleanup_guard = self.begin_library_plaintext_derivative_cleanup()?;
        self.reconcile_attachment_storage_unlocked()
    }

    pub(super) fn reconcile_attachment_storage_unlocked(
        &self,
    ) -> StorageResult<AttachmentRecoveryReport> {
        let operations = self.list_attachment_operations()?;
        let mut report = AttachmentRecoveryReport::default();

        for operation in &operations {
            let attachment: Attachment = match serde_json::from_str(&operation.attachment_json) {
                Ok(attachment) => attachment,
                Err(_) => {
                    report.unresolved_operations += 1;
                    continue;
                }
            };
            match operation.operation.as_str() {
                "add" => {
                    let note_exists = self.get_note(&operation.note_id)?.is_some();
                    if !note_exists {
                        self.discard_staged_attachment(operation)?;
                        // The Add row is itself authoritative ownership for a
                        // partially materialized canonical successor. Retire it
                        // only in the same transaction that establishes the
                        // generic canonical owner when no other durable database
                        // reference exists. Quarantine may run only afterward.
                        let mut connection = self.connection()?;
                        let transaction =
                            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                        transaction.execute(
                            "DELETE FROM attachment_operations WHERE id = ?1",
                            [&operation.id],
                        )?;
                        super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
                            &transaction,
                            &attachment.sha256,
                        )?;
                        transaction.commit()?;
                        self.quarantine_blob_if_unreferenced(&attachment.sha256)?;
                        continue;
                    }
                    if self.finish_attachment_add(
                        operation,
                        &attachment,
                        StagingBlobVerification::VerifyFromDisk,
                    )? {
                        report.replayed_adds += 1;
                    } else {
                        report.unresolved_operations += 1;
                    }
                }
                "remove" => {
                    let mut connection = self.connection()?;
                    let transaction =
                        connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    self.enqueue_sync_delete(&transaction, "attachment", &attachment.id)?;
                    transaction
                        .execute("DELETE FROM attachments WHERE id = ?1", [&attachment.id])?;
                    super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
                        &transaction,
                        &attachment.sha256,
                    )?;
                    transaction.commit()?;
                    match self.quarantine_blob_if_unreferenced(&attachment.sha256) {
                        Ok(()) => {
                            self.delete_attachment_operation(&operation.id)?;
                            report.replayed_removes += 1;
                        }
                        Err(_) => report.unresolved_operations += 1,
                    }
                }
                _ => report.unresolved_operations += 1,
            }
        }

        report.removed_incoming_files =
            self.cleanup_untracked_incoming_files(&self.list_attachment_operations()?)?;
        report.removed_unreferenced_attachment_records =
            self.prune_unreferenced_attachment_records_unlocked()?;
        report.quarantined_orphans = self.quarantine_orphan_blobs_unlocked()?;
        report.purged_quarantine_files = self.purge_old_quarantine_files()?;
        report.missing_attachment_records = self.audit_attachments()?.missing.len();
        Ok(report)
    }
    pub fn attachment_file_path(&self, attachment: &Attachment) -> StorageResult<PathBuf> {
        let (relative_path, _, _) = validate_attachment_blob_identity(
            &format!("attachment {}", attachment.id),
            &attachment.sha256,
            attachment.byte_size,
        )?;
        Ok(self.attachments_directory.join(relative_path))
    }
    pub fn audit_attachments(&self) -> StorageResult<AttachmentAudit> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT 0, 0, id, note_id, file_name, sha256, byte_size
             FROM attachments
             UNION ALL
             SELECT 1, 0, attachment_id, revision_id, attachment_id, sha256, byte_size
             FROM revision_attachment_refs
             UNION ALL
             SELECT 0, 1, sha256, note_id, 'Encrypted attachment data', sha256, byte_size
             FROM vault_blob_refs
             ORDER BY sha256, byte_size, 1, 2, 3, 4",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AttachmentBlobReference {
                is_revision: row.get::<_, i64>(0)? != 0,
                is_protected: row.get::<_, i64>(1)? != 0,
                record_id: row.get(2)?,
                owner_id: row.get(3)?,
                file_name: row.get(4)?,
                sha256: row.get(5)?,
                byte_size: row.get(6)?,
            })
        })?;
        let mut audit = AttachmentAudit::default();
        let mut current_blob = None::<AttachmentBlobExpectation>;

        for reference in rows {
            let reference = reference?;
            let description = reference.description();
            let (canonical_path, normalized_sha256, byte_size) = validate_attachment_blob_identity(
                &description,
                &reference.sha256,
                reference.byte_size,
            )?;
            if let Some(existing) = current_blob.as_ref()
                && existing.sha256 == normalized_sha256
            {
                if existing.byte_size != byte_size {
                    return Err(StorageError::Integrity(format!(
                        "conflicting attachment metadata for SHA-256 {}: {} declares {} byte(s), but {} declares {} byte(s)",
                        normalized_sha256,
                        existing.description,
                        existing.byte_size,
                        description,
                        byte_size
                    )));
                }
                continue;
            }
            if let Some(expected) = current_blob.take() {
                self.audit_attachment_blob(expected, &mut audit)?;
            }
            current_blob = Some(AttachmentBlobExpectation {
                description,
                file_name: reference.file_name,
                canonical_path,
                sha256: normalized_sha256,
                byte_size,
            });
        }
        if let Some(expected) = current_blob {
            self.audit_attachment_blob(expected, &mut audit)?;
        }
        Ok(audit)
    }
    fn audit_attachment_blob(
        &self,
        expected: AttachmentBlobExpectation,
        audit: &mut AttachmentAudit,
    ) -> StorageResult<()> {
        audit.checked += 1;
        let path = self.attachments_directory.join(&expected.canonical_path);
        if !crate::security::managed_path_is_present(&path)? {
            self.forget_verified_attachment_blob(&expected.sha256);
            audit.missing.push(expected.file_name);
            return Ok(());
        }
        let (checksum, byte_size) = sha256_file(&path)?;
        if checksum != expected.sha256 || byte_size != expected.byte_size {
            self.forget_verified_attachment_blob(&expected.sha256);
            audit.corrupt.push(expected.file_name);
        } else {
            self.mark_attachment_blob_verified(&expected.sha256, expected.byte_size, &path);
        }
        Ok(())
    }
    pub(super) fn insert_attachment(
        &self,
        transaction: &Transaction<'_>,
        attachment: &Attachment,
    ) -> StorageResult<()> {
        transaction.execute(
            "INSERT INTO attachments(
                id, note_id, file_name, mime_type, byte_size, sha256,
                image_pixel_width, image_pixel_height, image_format, image_orientation,
                image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                attachment.id,
                attachment.note_id,
                attachment.file_name,
                attachment.mime_type,
                attachment.byte_size,
                attachment.sha256,
                attachment.image_pixel_width,
                attachment.image_pixel_height,
                attachment.image_format,
                attachment.image_orientation,
                attachment.image_has_alpha.map(|value| if value { 1_i64 } else { 0_i64 }),
                attachment.image_animated.map(|value| if value { 1_i64 } else { 0_i64 }),
                attachment.source_attachment_id,
                attachment.image_edit_recipe,
                attachment.embedded as i64,
                attachment.unreferenced_at,
                attachment.created_at,
                attachment.updated_at
            ],
        )?;
        Ok(())
    }
    pub(super) fn upsert_attachment(
        &self,
        transaction: &Transaction<'_>,
        attachment: &Attachment,
    ) -> StorageResult<()> {
        transaction.execute(
            "INSERT INTO attachments(
                id, note_id, file_name, mime_type, byte_size, sha256,
                image_pixel_width, image_pixel_height, image_format, image_orientation,
                image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(id) DO UPDATE SET
                note_id = excluded.note_id,
                file_name = excluded.file_name,
                mime_type = excluded.mime_type,
                byte_size = excluded.byte_size,
                sha256 = excluded.sha256,
                image_pixel_width = excluded.image_pixel_width,
                image_pixel_height = excluded.image_pixel_height,
                image_format = excluded.image_format,
                image_orientation = excluded.image_orientation,
                image_has_alpha = excluded.image_has_alpha,
                image_animated = excluded.image_animated,
                source_attachment_id = excluded.source_attachment_id,
                image_edit_recipe = excluded.image_edit_recipe,
                embedded = excluded.embedded,
                unreferenced_at = excluded.unreferenced_at,
                updated_at = excluded.updated_at",
            params![
                attachment.id,
                attachment.note_id,
                attachment.file_name,
                attachment.mime_type,
                attachment.byte_size,
                attachment.sha256,
                attachment.image_pixel_width,
                attachment.image_pixel_height,
                attachment.image_format,
                attachment.image_orientation,
                attachment.image_has_alpha.map(|value| if value { 1_i64 } else { 0_i64 }),
                attachment.image_animated.map(|value| if value { 1_i64 } else { 0_i64 }),
                attachment.source_attachment_id,
                attachment.image_edit_recipe,
                attachment.embedded as i64,
                attachment.unreferenced_at,
                attachment.created_at,
                attachment.updated_at
            ],
        )?;
        Ok(())
    }
    pub(super) fn list_all_attachments(&self) -> StorageResult<Vec<Attachment>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, note_id, file_name, mime_type, byte_size, sha256,
                    image_pixel_width, image_pixel_height, image_format, image_orientation,
                    image_has_alpha, image_animated, source_attachment_id, image_edit_recipe, embedded, unreferenced_at, created_at, updated_at
             FROM attachments ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], attachment_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub(super) fn cleanup_untracked_incoming_files(
        &self,
        operations: &[AttachmentOperation],
    ) -> StorageResult<usize> {
        let incoming = self.attachments_directory.join(".incoming");
        fs::create_dir_all(&incoming)?;
        let mut tracked = operations
            .iter()
            .filter_map(|operation| operation.staging_relative_path.clone())
            .collect::<HashSet<_>>();
        tracked.extend(self.pending_ingest_staging_paths()?);
        tracked.extend(self.pending_managed_attachment_staging_paths()?);
        let mut removed = 0;
        for entry in fs::read_dir(&incoming)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(StorageError::Integrity(format!(
                    "attachment incoming staging contains an unexpected non-file entry: {}",
                    entry.path().display()
                )));
            }
            let relative = entry
                .path()
                .strip_prefix(&self.attachments_directory)
                .map_err(|_| StorageError::Integrity("invalid incoming path".to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            if !tracked.contains(&relative) && crate::native::privacy::remove_file(&entry.path())? {
                removed += 1;
            }
        }
        sync_parent_directory(&incoming.join("placeholder"))?;
        Ok(removed)
    }
    fn canonical_blob_is_collectable(&self, sha256: &str) -> StorageResult<bool> {
        validate_sha256(sha256)?;
        let connection = self.connection()?;
        let references: i64 = connection.query_row(
            "SELECT
                (SELECT COUNT(*) FROM attachments WHERE sha256 = ?1)
                + (SELECT COUNT(*) FROM revision_attachment_refs WHERE sha256 = ?1)
                + (SELECT COUNT(*) FROM vault_blob_refs WHERE sha256 = ?1)",
            [sha256],
            |row| row.get(0),
        )?;
        drop(connection);
        if references != 0 {
            return Ok(false);
        }
        for operation in self.list_attachment_operations()? {
            if operation.operation != "add" {
                continue;
            }
            let attachment: Attachment =
                serde_json::from_str(&operation.attachment_json).map_err(|error| {
                    StorageError::Integrity(format!(
                        "pending attachment Add ownership record is corrupt: {error}"
                    ))
                })?;
            if attachment.sha256 == sha256 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn quarantine_blob_if_unreferenced(&self, sha256: &str) -> StorageResult<()> {
        if !self.canonical_blob_is_collectable(sha256)? {
            return Ok(());
        }
        // Derived plaintext must disappear before the last canonical attachment
        // owner is moved to quarantine. If either strict cleanup fails, the
        // canonical blob and its durable ownership remain in place so normal
        // attachment recovery can retry the same content identity.
        self.discard_image_rendition_cache_strict(sha256)?;
        crate::media_compat::discard_cache_strict(
            &crate::library_paths::media_compat_directory(&self.data_directory),
            sha256,
        )?;

        let relative_path = blob_relative_path(sha256)?;
        let path = self.attachments_directory.join(relative_path);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                self.move_blob_to_quarantine(&path)?;
            }
            Ok(_) => {
                return Err(StorageError::Integrity(format!(
                    "collectable attachment blob is not a regular file: {}",
                    path.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    /// Immediately retire an unreferenced canonical blob that already has a
    /// durable generic cleanup owner. Protected hard deletion uses this after
    /// atomically handing off the final `vault_blob_refs` ownership; ciphertext
    /// does not need plaintext quarantine retention, but the durable owner makes
    /// a crash between database commit and file deletion recoverable.
    pub(super) fn discard_canonical_blob_if_unreferenced_strict(
        &self,
        sha256: &str,
    ) -> StorageResult<()> {
        if !self.canonical_blob_is_collectable(sha256)? {
            return Ok(());
        }
        // Protection/hard-delete is fail-closed for every derived plaintext
        // equivalent. Keep the canonical durable owner alive until these
        // derivatives are proven absent.
        self.discard_image_rendition_cache_strict(sha256)?;
        crate::media_compat::discard_cache_strict(
            &crate::library_paths::media_compat_directory(&self.data_directory),
            sha256,
        )?;

        let relative_path = blob_relative_path(sha256)?;
        let path = self.attachments_directory.join(&relative_path);
        if self
            .portability_blob_plaintext_stage_id(&relative_path)?
            .is_some()
        {
            let mut owned = self.require_canonical_blob_owned_plaintext(&relative_path)?;
            self.discard_owned_plaintext(&mut owned)?;
        } else {
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => {
                    return Err(StorageError::Integrity(
                        "collectable canonical blob exists without its durable cleanup owner"
                            .to_string(),
                    ));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub(super) fn discard_image_rendition_cache_strict(&self, sha256: &str) -> StorageResult<()> {
        validate_sha256(sha256)?;
        let relative_directory =
            image_rendition_cache_relative_directory(sha256).ok_or_else(|| {
                StorageError::Validation("invalid image rendition cache identity".to_string())
            })?;
        let cache_root = crate::library_paths::image_rendition_directory(&self.data_directory);
        let owner = crate::native::rendition::persistent_rendition_owner(&cache_root)?;
        owner
            .remove_subtree_strict(&self.data_directory.join(relative_directory))
            .map_err(StorageError::Io)
    }
    pub(super) fn move_blob_to_quarantine(&self, source: &Path) -> StorageResult<PathBuf> {
        let source_relative = source
            .strip_prefix(&self.attachments_directory)
            .map_err(|_| {
                StorageError::Integrity("attachment blob escaped library ownership".to_string())
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let file_name = source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                StorageError::Integrity("attachment blob has no canonical identity".to_string())
            })?;
        validate_sha256(file_name)?;
        if blob_relative_path(file_name)? != source_relative {
            return Err(StorageError::Integrity(
                "only canonical attachment blobs may enter quarantine".to_string(),
            ));
        }

        let quarantine = self.attachments_directory.join(".orphaned");
        fs::create_dir_all(&quarantine)?;
        let destination_relative = format!(
            ".orphaned/{}-{}-{}",
            unix_timestamp(),
            new_id("orphan"),
            file_name
        );
        // The source owner is established before the successor reservation and
        // the predecessor records that successor before the filesystem rename.
        // Either crash state is therefore recoverable by the durable ledger.
        let mut source_owner = self.require_canonical_blob_owned_plaintext(&source_relative)?;
        let _successor_owner = self.reserve_orphan_blob_owned_plaintext(&destination_relative)?;
        self.rename_owned_canonical_blob_to_orphan(&mut source_owner, &destination_relative)?;
        let destination = self.attachments_directory.join(&destination_relative);
        if is_valid_sha256(file_name) {
            self.forget_verified_attachment_blob(file_name);
        }
        sync_parent_directory(source)?;
        sync_parent_directory(&destination)?;
        self.complete_owned_library_blob_rename(&mut source_owner)?;
        if let Some(parent) = source.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(destination)
    }
    pub(super) fn prune_unreferenced_attachment_records_unlocked(&self) -> StorageResult<usize> {
        let cutoff = unix_timestamp().saturating_sub(ORPHAN_QUARANTINE_RETENTION_SECONDS as i64);
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let candidates = {
            let mut statement = transaction.prepare(
                "SELECT a.id, a.sha256
                 FROM attachments a
                 WHERE a.embedded = 0
                   AND a.unreferenced_at <= ?1
                   AND NOT EXISTS(
                       SELECT 1 FROM vault_note_roots root WHERE root.note_id = a.note_id
                   )
                   AND NOT EXISTS(
                       SELECT 1 FROM revision_attachment_refs r WHERE r.attachment_id = a.id
                   )
                   AND NOT EXISTS(
                       SELECT 1 FROM attachments child WHERE child.source_attachment_id = a.id
                   )
                   AND NOT EXISTS(
                       SELECT 1 FROM attachment_ingest_jobs jobs
                       WHERE jobs.attachment_id = a.id
                   )
                 ORDER BY a.unreferenced_at, a.id",
            )?;
            let rows = statement.query_map([cutoff], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (attachment_id, _) in &candidates {
            self.enqueue_sync_delete(&transaction, "attachment", attachment_id)?;
            transaction.execute("DELETE FROM attachments WHERE id = ?1", [attachment_id])?;
        }
        // Bridge the final database reference to durable generic ownership in
        // this same transaction. The authority also recognizes pending Add
        // operations as existing owners, so callers cannot create competing
        // cleanup obligations for the same canonical blob.
        let candidate_hashes = candidates
            .iter()
            .map(|(_, sha256)| sha256.clone())
            .collect::<HashSet<_>>();
        for sha256 in candidate_hashes {
            super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
                &transaction,
                &sha256,
            )?;
        }
        transaction.commit()?;
        for (_, sha256) in &candidates {
            self.quarantine_blob_if_unreferenced(sha256)?;
        }
        Ok(candidates.len())
    }

    pub(super) fn quarantine_orphan_blobs_unlocked(&self) -> StorageResult<usize> {
        let mut referenced = self
            .list_all_attachments()?
            .into_iter()
            .map(|attachment| blob_relative_path(&attachment.sha256))
            .collect::<StorageResult<HashSet<_>>>()?;
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT DISTINCT sha256 FROM revision_attachment_refs")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for sha256 in rows {
            referenced.insert(blob_relative_path(&sha256?)?);
        }
        drop(statement);
        let mut protected_statement =
            connection.prepare("SELECT DISTINCT sha256 FROM vault_blob_refs")?;
        let protected_rows = protected_statement.query_map([], |row| row.get::<_, String>(0))?;
        for sha256 in protected_rows {
            referenced.insert(blob_relative_path(&sha256?)?);
        }
        drop(protected_statement);
        drop(connection);
        for operation in self.list_attachment_operations()? {
            if operation.operation != "add" {
                continue;
            }
            let attachment: Attachment =
                serde_json::from_str(&operation.attachment_json).map_err(|error| {
                    StorageError::Integrity(format!(
                        "pending attachment Add ownership record is corrupt: {error}"
                    ))
                })?;
            referenced.insert(blob_relative_path(&attachment.sha256)?);
        }
        let blobs_root = self.attachments_directory.join("blobs");
        if !crate::security::managed_path_is_present(&blobs_root)? {
            return Ok(0);
        }
        let mut quarantined = 0;
        for prefix in fs::read_dir(&blobs_root)? {
            let prefix = prefix?;
            if !prefix.file_type()?.is_dir() {
                continue;
            }
            for blob in fs::read_dir(prefix.path())? {
                let blob = blob?;
                if !blob.file_type()?.is_file() {
                    continue;
                }
                let relative = blob
                    .path()
                    .strip_prefix(&self.attachments_directory)
                    .map_err(|_| StorageError::Integrity("invalid attachment path".to_string()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !referenced.contains(&relative) {
                    let sha256 = blob.file_name().to_string_lossy().to_string();
                    // Keep the canonical plaintext and its durable cleanup owner
                    // as the retry anchor until every derivative is gone.
                    self.discard_image_rendition_cache_strict(&sha256)?;
                    crate::media_compat::discard_cache_strict(
                        &crate::library_paths::media_compat_directory(&self.data_directory),
                        &sha256,
                    )?;
                    self.move_blob_to_quarantine(&blob.path())?;
                    quarantined += 1;
                }
            }
            let _ = fs::remove_dir(prefix.path());
        }
        Ok(quarantined)
    }
    /// Remove quarantined historical plaintext copies for specific content
    /// identities. Quarantine names end with the original canonical SHA-256, so
    /// Vault protection can retire every Rustnote-managed equivalent of a
    /// protected blob without deleting unrelated orphan-recovery history.
    pub(super) fn discard_orphaned_quarantine_for_hashes_strict(
        &self,
        sha256_values: &HashSet<String>,
    ) -> StorageResult<usize> {
        for sha256 in sha256_values {
            validate_sha256(sha256)?;
        }
        if sha256_values.is_empty() {
            return Ok(0);
        }

        // Quarantine is the final per-blob durable retry anchor. Retire every
        // derived plaintext equivalent first; if cleanup fails, the quarantine
        // ownership survives and a later recovery/protection pass can retry.
        for sha256 in sha256_values {
            self.discard_image_rendition_cache_strict(sha256)?;
            crate::media_compat::discard_cache_strict(
                &crate::library_paths::media_compat_directory(&self.data_directory),
                sha256,
            )?;
        }

        let quarantine = self.attachments_directory.join(".orphaned");
        let entries = match fs::read_dir(&quarantine) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some((_, candidate_hash)) = name.rsplit_once('-') else {
                continue;
            };
            if !sha256_values.contains(candidate_hash) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() || metadata.is_file() {
                if metadata.file_type().is_symlink() {
                    return Err(StorageError::Integrity(format!(
                        "matching attachment quarantine artifact is a symlink: {}",
                        entry.path().display()
                    )));
                }
                let relative = entry
                    .path()
                    .strip_prefix(&self.attachments_directory)
                    .map_err(|_| {
                        StorageError::Integrity(
                            "quarantined attachment escaped library ownership".to_string(),
                        )
                    })?
                    .to_string_lossy()
                    .replace('\\', "/");
                let mut owned = self.require_orphan_blob_owned_plaintext(&relative)?;
                self.discard_owned_plaintext(&mut owned)?;
                removed += 1;
            } else {
                return Err(StorageError::Integrity(format!(
                    "matching attachment quarantine artifact is not a file: {}",
                    entry.path().display()
                )));
            }
        }
        if removed > 0 {
            sync_parent_directory(&quarantine.join("placeholder"))?;
        }
        Ok(removed)
    }

    pub(super) fn purge_old_quarantine_files(&self) -> StorageResult<usize> {
        let quarantine = self.attachments_directory.join(".orphaned");
        if !crate::security::managed_path_is_present(&quarantine)? {
            return Ok(0);
        }
        let now = SystemTime::now();
        let mut purged = 0;
        for entry in fs::read_dir(&quarantine)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let modified = entry.metadata()?.modified().unwrap_or(now);
            let age = now.duration_since(modified).unwrap_or_default().as_secs();
            if age >= ORPHAN_QUARANTINE_RETENTION_SECONDS {
                let name = entry.file_name();
                let name = name.to_str().ok_or_else(|| {
                    StorageError::Integrity(format!(
                        "quarantined attachment name is not valid UTF-8: {}",
                        entry.path().display()
                    ))
                })?;
                let (_, sha256) = name.rsplit_once('-').ok_or_else(|| {
                    StorageError::Integrity(format!(
                        "quarantined attachment name has no content identity: {}",
                        entry.path().display()
                    ))
                })?;
                validate_sha256(sha256)?;

                // Do not retire the final per-blob owner before all persistent
                // derivatives are absent. Cleanup remains ownership-complete even
                // when a producer/delete race leaves a derivative behind.
                self.discard_image_rendition_cache_strict(sha256)?;
                crate::media_compat::discard_cache_strict(
                    &crate::library_paths::media_compat_directory(&self.data_directory),
                    sha256,
                )?;

                let relative = entry
                    .path()
                    .strip_prefix(&self.attachments_directory)
                    .map_err(|_| {
                        StorageError::Integrity(
                            "quarantined attachment escaped library ownership".to_string(),
                        )
                    })?
                    .to_string_lossy()
                    .replace('\\', "/");
                let mut owned = self.require_orphan_blob_owned_plaintext(&relative)?;
                self.discard_owned_plaintext(&mut owned)?;
                purged += 1;
            }
        }
        if purged > 0 {
            sync_parent_directory(&quarantine.join("placeholder"))?;
        }
        Ok(purged)
    }
}

fn attachment_blob_verification_stamp(path: &Path) -> Option<u128> {
    let modified = path.metadata().ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
}

pub(super) fn canonical_attachment_identity(
    file_name: &str,
    mime_type: Option<&str>,
) -> StorageResult<(String, Option<String>)> {
    let file_name = Path::new(file_name)
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("image")
        .to_string();
    if file_name.len() > 1024 {
        return Err(StorageError::Validation(
            "attachment filename is too long".to_string(),
        ));
    }
    let mime_type = resolved_mime_type(&file_name, mime_type);
    if mime_type.as_ref().is_some_and(|value| value.len() > 255) {
        return Err(StorageError::Validation(
            "attachment media type is too long".to_string(),
        ));
    }
    Ok((file_name, mime_type))
}

fn canonical_image_edit_recipe(recipe: &str) -> StorageResult<String> {
    if recipe.len() > 524_288 {
        return Err(StorageError::Validation(
            "image edit recipe is too large".to_string(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(recipe)
        .map_err(|_| StorageError::Validation("image edit recipe is not valid JSON".to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(StorageError::Validation(
            "image edit recipe must be a JSON object".to_string(),
        ));
    };
    if object.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err(StorageError::Validation(
            "unsupported image edit recipe version".to_string(),
        ));
    }
    serde_json::to_string(&value).map_err(StorageError::from)
}

pub(super) fn attachment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: row.get(0)?,
        note_id: row.get(1)?,
        file_name: row.get(2)?,
        mime_type: row.get(3)?,
        byte_size: row.get(4)?,
        sha256: row.get(5)?,
        image_pixel_width: row.get(6)?,
        image_pixel_height: row.get(7)?,
        image_format: row.get(8)?,
        image_orientation: row.get(9)?,
        image_has_alpha: row.get::<_, Option<i64>>(10)?.map(|value| value != 0),
        image_animated: row.get::<_, Option<i64>>(11)?.map(|value| value != 0),
        source_attachment_id: row.get(12)?,
        image_edit_recipe: row.get(13)?,
        embedded: row.get::<_, i64>(14)? != 0,
        unreferenced_at: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

pub(super) fn attachment_operation_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AttachmentOperation> {
    Ok(AttachmentOperation {
        id: row.get(0)?,
        operation: row.get(1)?,
        attachment_id: row.get(2)?,
        note_id: row.get(3)?,
        attachment_json: row.get(4)?,
        staging_relative_path: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub(super) fn validate_attachment_blob_identity(
    description: &str,
    sha256: &str,
    byte_size: i64,
) -> StorageResult<(String, String, u64)> {
    if !is_valid_sha256(sha256) {
        return Err(StorageError::Integrity(format!(
            "{description} has invalid SHA-256 value {sha256}"
        )));
    }
    let byte_size = u64::try_from(byte_size)
        .map_err(|_| StorageError::Integrity(format!("{description} has a negative byte size")))?;
    let canonical_path = canonical_blob_relative_path(sha256)
        .expect("validated SHA-256 must have a canonical attachment path");
    Ok((canonical_path, sha256.to_string(), byte_size))
}

pub(super) fn validate_sha256(value: &str) -> StorageResult<()> {
    if !is_valid_sha256(value) {
        return Err(StorageError::InvalidBackup(format!(
            "invalid SHA-256 value {value}"
        )));
    }
    Ok(())
}

pub(super) fn blob_relative_path(sha256: &str) -> StorageResult<String> {
    validate_sha256(sha256)?;
    Ok(canonical_blob_relative_path(sha256)
        .expect("validated SHA-256 must have a canonical attachment path"))
}

pub(super) fn validate_staging_relative_path(relative_path: &str) -> StorageResult<()> {
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
        || !relative_path.starts_with(".incoming/")
    {
        return Err(StorageError::Integrity(format!(
            "unsafe staging path {relative_path}"
        )));
    }
    Ok(())
}

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest_to_hex(&digest)
}

pub(super) fn sha256_file(path: &Path) -> StorageResult<(String, u64)> {
    let mut reader = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    let digest = hasher.finalize();
    Ok((digest_to_hex(&digest), size))
}

pub(super) fn digest_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub(super) fn atomic_replace(destination: &Path, bytes: &[u8]) -> StorageResult<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut transaction = crate::native::output::ExternalFileOutputTransaction::begin(destination)?;
    let mut file = transaction.create_file()?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    transaction.commit()?;
    Ok(())
}

pub(super) fn copy_file_atomically(source: &Path, destination: &Path) -> StorageResult<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::native::output::copy_file_atomically(source, destination)?;
    Ok(())
}

pub(super) fn temporary_sibling(destination: &Path, suffix: &str) -> StorageResult<PathBuf> {
    let parent = destination.parent().ok_or_else(|| {
        StorageError::Validation("destination has no parent directory".to_string())
    })?;
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("rustnote");
    Ok(parent.join(format!(".{name}.{}.{}", new_id("tmp"), suffix)))
}

pub(super) fn sync_parent_directory(path: &Path) -> StorageResult<()> {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            File::open(parent)?.sync_all()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::storage::new_id;

    use super::*;

    fn test_storage(label: &str) -> (Storage, PathBuf) {
        let suffix = new_id(label);
        let root = std::env::temp_dir().join(format!("rustnote-{suffix}"));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("test storage should initialize");
        (storage, root)
    }

    fn cleanup(storage: Storage, root: PathBuf) {
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn quarantine_transfer_keeps_durable_owner_until_plaintext_deletion() {
        let (storage, root) = test_storage("quarantine-owner-lifetime");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("owned-orphan.txt");
        fs::write(&source, b"orphan ownership continuity").expect("write source");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        storage
            .remove_attachment(&attachment.id)
            .expect("remove and quarantine attachment");

        let connection = storage.connection().expect("owner connection");
        let (stage_id, relative_path): (String, String) = connection
            .query_row(
                "SELECT id, relative_path FROM managed_plaintext_staging
                 WHERE kind = 'portability_blob' AND relative_path LIKE '.orphaned/%'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("durable quarantine owner");
        drop(connection);
        let quarantine_path = storage.attachments_directory.join(&relative_path);
        assert!(quarantine_path.exists());
        let mut hashes = HashSet::new();
        hashes.insert(attachment.sha256.clone());
        assert_eq!(
            storage
                .discard_orphaned_quarantine_for_hashes_strict(&hashes)
                .expect("discard owned quarantine"),
            1
        );
        assert!(!quarantine_path.exists());
        let owner_exists: i64 = storage
            .connection()
            .expect("owner verification")
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM managed_plaintext_staging WHERE id = ?1)",
                [&stage_id],
                |row| row.get(0),
            )
            .expect("owner state");
        assert_eq!(
            owner_exists, 0,
            "ownership may retire only after plaintext deletion"
        );
        cleanup(storage, root);
    }

    #[test]
    fn pending_add_remains_the_only_owner_when_last_attachment_reference_is_removed() {
        let (storage, root) = test_storage("pending-add-owner");
        let source_note = storage.create_note(None).expect("create source note");
        let pending_note = storage.create_note(None).expect("create pending note");
        let source = root.join("shared-pending-add.txt");
        fs::write(&source, b"shared canonical bytes").expect("write source");
        let attachment = storage
            .add_attachment(&source_note.id, &source)
            .expect("add source attachment");
        let canonical_path = storage
            .attachment_file_path(&attachment)
            .expect("canonical path");

        let mut pending = attachment.clone();
        pending.id = new_id("attachment");
        pending.note_id = pending_note.id.clone();
        let mut connection = storage.connection().expect("pending add connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("pending add transaction");
        storage
            .insert_attachment_operation(&transaction, "add", &pending, None)
            .expect("insert pending Add owner");
        transaction.commit().expect("commit pending Add owner");

        storage
            .remove_attachment(&attachment.id)
            .expect("remove final persistent attachment reference");

        assert!(
            canonical_path.exists(),
            "pending Add ownership must prevent canonical blob quarantine/deletion"
        );
        let canonical_relative =
            blob_relative_path(&attachment.sha256).expect("canonical relative");
        let generic_owner_exists: i64 = storage
            .connection()
            .expect("owner verification")
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM managed_plaintext_staging
                    WHERE kind = 'portability_blob' AND relative_path = ?1
                )",
                [&canonical_relative],
                |row| row.get(0),
            )
            .expect("generic owner state");
        assert_eq!(
            generic_owner_exists, 0,
            "pending Add must remain the sole cleanup owner for its canonical successor"
        );

        cleanup(storage, root);
    }

    #[test]
    fn corrupt_pending_add_fails_closed_before_canonical_ownership_can_move() {
        let (storage, root) = test_storage("corrupt-pending-add-owner");
        let source_note = storage.create_note(None).expect("create source note");
        let pending_note = storage.create_note(None).expect("create pending note");
        let source = root.join("corrupt-pending-add.txt");
        fs::write(&source, b"canonical bytes with uncertain successor owner")
            .expect("write source");
        let attachment = storage
            .add_attachment(&source_note.id, &source)
            .expect("add source attachment");
        let canonical_path = storage
            .attachment_file_path(&attachment)
            .expect("canonical path");

        let mut pending = attachment.clone();
        pending.id = new_id("attachment");
        pending.note_id = pending_note.id.clone();
        let mut connection = storage.connection().expect("pending add connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("pending add transaction");
        let operation = storage
            .insert_attachment_operation(&transaction, "add", &pending, None)
            .expect("insert pending Add owner");
        transaction
            .execute(
                "UPDATE attachment_operations SET attachment_json = '{' WHERE id = ?1",
                [&operation.id],
            )
            .expect("corrupt pending Add owner");
        transaction
            .commit()
            .expect("commit corrupt ownership record");

        assert!(
            storage.remove_attachment(&attachment.id).is_err(),
            "uncertain pending Add ownership must fail closed before the last reference is removed"
        );
        assert!(
            canonical_path.exists(),
            "canonical plaintext must remain in place"
        );
        assert!(
            storage
                .list_attachments(&source_note.id)
                .expect("source attachments")
                .iter()
                .any(|candidate| candidate.id == attachment.id),
            "the database reference must roll back when ownership cannot be proven"
        );

        cleanup(storage, root);
    }

    #[test]
    fn steady_state_quarantine_rejects_unowned_canonical_plaintext() {
        let (storage, root) = test_storage("unowned-canonical-fail-closed");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("unowned-canonical.txt");
        fs::write(&source, b"canonical plaintext must not be adopted late").expect("write source");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        let canonical = storage
            .attachment_file_path(&attachment)
            .expect("canonical path");

        storage
            .connection()
            .expect("corrupt owner connection")
            .execute("DELETE FROM attachments WHERE id = ?1", [&attachment.id])
            .expect("simulate legacy unowned canonical residue");

        assert!(
            storage
                .quarantine_blob_if_unreferenced(&attachment.sha256)
                .is_err(),
            "steady-state quarantine must not manufacture ownership after plaintext exists"
        );
        assert!(
            canonical.exists(),
            "fail-closed quarantine must preserve the unowned path"
        );
        cleanup(storage, root);
    }

    #[test]
    fn startup_verification_rejects_unowned_canonical_plaintext() {
        let (storage, root) = test_storage("canonical-startup-fail-closed");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("unowned-canonical-startup.txt");
        fs::write(&source, b"unowned canonical startup residue").expect("write source");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        let canonical = storage
            .attachment_file_path(&attachment)
            .expect("canonical path");
        storage
            .connection()
            .expect("invariant fixture connection")
            .execute("DELETE FROM attachments WHERE id = ?1", [&attachment.id])
            .expect("simulate unowned canonical residue");

        assert!(
            storage
                .verify_managed_attachment_plaintext_ownership_unlocked()
                .is_err(),
            "startup must reject canonical plaintext that has neither a database owner nor a prior cleanup owner"
        );
        assert!(
            canonical.exists(),
            "verification must preserve the unowned evidence"
        );
        assert!(
            storage
                .require_canonical_blob_owned_plaintext(
                    &blob_relative_path(&attachment.sha256).expect("canonical relative path"),
                )
                .is_err(),
            "startup verification must not adopt canonical plaintext after it exists"
        );
        cleanup(storage, root);
    }

    #[cfg(unix)]
    #[test]
    fn ambiguous_attachment_operation_stage_presence_fails_closed_until_retry() {
        use std::os::unix::fs::symlink;

        let (storage, root) = test_storage("attachment-operation-ambiguous-presence");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("operation-source.txt");
        fs::write(&source, b"canonical attachment").expect("write source");
        let mut pending = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        pending.id = new_id("attachment");

        let staging_relative = format!(".incoming/{}.tmp", new_id("ambiguous-stage"));
        let staging_path = storage
            .staging_attachment_path(&staging_relative)
            .expect("staging path");
        fs::create_dir_all(staging_path.parent().expect("incoming parent"))
            .expect("create incoming");
        let mut connection = storage.connection().expect("operation connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("operation transaction");
        let operation = storage
            .insert_attachment_operation(&transaction, "add", &pending, Some(&staging_relative))
            .expect("insert attachment operation");
        transaction.commit().expect("commit operation owner");
        fs::write(&staging_path, b"owned staged plaintext").expect("write stage");

        let incoming = staging_path
            .parent()
            .expect("incoming parent")
            .to_path_buf();
        let preserved_incoming = storage.attachments_directory.join(".incoming-preserved-op");
        fs::rename(&incoming, &preserved_incoming).expect("move incoming directory");
        symlink(".incoming", &incoming).expect("symlink loop");

        assert!(
            storage.discard_staged_attachment(&operation).is_err(),
            "ambiguous staging metadata must fail closed"
        );
        assert!(
            storage
                .list_attachment_operations()
                .expect("list operations")
                .iter()
                .any(|candidate| candidate.id == operation.id),
            "durable attachment-operation owner must survive failure"
        );

        fs::remove_file(&incoming).expect("remove symlink loop");
        fs::rename(&preserved_incoming, &incoming).expect("restore incoming directory");
        storage
            .discard_staged_attachment(&operation)
            .expect("retry stage cleanup");
        assert!(!staging_path.exists(), "retry must remove staged plaintext");
        storage
            .delete_attachment_operation(&operation.id)
            .expect("retire operation owner after cleanup");

        cleanup(storage, root);
    }

    #[test]
    fn abandoned_pending_add_transfers_ownership_before_operation_retirement() {
        let (storage, root) = test_storage("abandoned-add-owner-handoff");
        let source_note = storage.create_note(None).expect("create source note");
        let source = root.join("abandoned-add.txt");
        fs::write(&source, b"pending Add canonical successor").expect("write source");
        let attachment = storage
            .add_attachment(&source_note.id, &source)
            .expect("add source attachment");
        let canonical = storage
            .attachment_file_path(&attachment)
            .expect("canonical path");

        let mut pending = attachment.clone();
        pending.id = new_id("attachment");
        pending.note_id = crate::domain::NoteId::parse("missing-pending-add-note")
            .expect("valid missing note id");
        let mut connection = storage.connection().expect("pending add connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("pending add transaction");
        storage
            .insert_attachment_operation(&transaction, "add", &pending, None)
            .expect("insert pending Add owner");
        transaction.commit().expect("commit pending Add owner");

        storage
            .remove_attachment(&attachment.id)
            .expect("remove final database reference while Add owns canonical successor");
        assert!(canonical.exists());
        storage
            .reconcile_attachment_storage_unlocked()
            .expect("recover abandoned Add");
        assert!(
            !canonical.exists(),
            "abandoned Add successor should move to quarantine"
        );
        let (operations, orphan_owners): (i64, i64) = storage
            .connection()
            .expect("owner verification")
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM attachment_operations),
                    (SELECT COUNT(*) FROM managed_plaintext_staging
                     WHERE kind = 'portability_blob' AND relative_path LIKE '.orphaned/%')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ownership state");
        assert_eq!(operations, 0);
        assert_eq!(
            orphan_owners, 1,
            "quarantined successor must retain durable cleanup ownership"
        );
        cleanup(storage, root);
    }

    #[test]
    fn revision_pruning_transfers_revision_only_blob_ownership_before_commit() {
        let (storage, root) = test_storage("revision-prune-owner-handoff");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("historical-only.txt");
        fs::write(&source, b"historical revision attachment").expect("write source");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        let canonical = storage
            .attachment_file_path(&attachment)
            .expect("canonical path");

        let mut connection = storage.connection().expect("revision connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("revision transaction");
        for revision_no in 1_i64..=11 {
            let revision_id = format!("rev-owner-{revision_no}");
            transaction
                .execute(
                    "INSERT INTO note_revisions(
                        id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at
                     ) VALUES (?1, ?2, ?3, ?4, '{}', 2, ?5)",
                    params![
                        &revision_id,
                        &note.id,
                        revision_no,
                        format!("checksum-{revision_no}"),
                        revision_no,
                    ],
                )
                .expect("insert revision");
            if revision_no == 1 {
                transaction
                    .execute(
                        "INSERT INTO revision_attachment_refs(
                            revision_id, attachment_id, sha256, byte_size
                         ) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            &revision_id,
                            &attachment.id,
                            &attachment.sha256,
                            attachment.byte_size,
                        ],
                    )
                    .expect("insert historical blob ref");
            }
        }
        transaction.commit().expect("commit revision fixtures");

        storage
            .remove_attachment(&attachment.id)
            .expect("remove current attachment row");
        assert!(
            canonical.exists(),
            "revision reference should retain canonical blob"
        );
        storage
            .set_revision_retention(10, 0)
            .expect("prune oldest revision");
        assert!(
            !canonical.exists(),
            "pruned historical-only blob should leave canonical path"
        );
        let owned_orphan: i64 = storage
            .connection()
            .expect("owner verification")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging
                 WHERE kind = 'portability_blob' AND relative_path LIKE '.orphaned/%'",
                [],
                |row| row.get(0),
            )
            .expect("orphan owner count");
        assert_eq!(owned_orphan, 1);
        cleanup(storage, root);
    }

    #[test]
    fn attachment_audit_reports_a_missing_canonical_blob_without_losing_its_record() {
        let (storage, root) = test_storage("attachment-missing");
        let note = storage.create_note(None).expect("create test note");
        let source = root.join("missing.txt");
        fs::write(&source, b"attachment that will disappear").expect("write source attachment");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        let canonical_path = storage
            .attachment_file_path(&attachment)
            .expect("resolve canonical path");
        fs::remove_file(&canonical_path).expect("remove canonical attachment blob");

        let audit = storage.audit_attachments().expect("audit attachments");
        assert!(
            audit
                .missing
                .iter()
                .any(|name| name == &attachment.file_name)
        );
        assert!(
            storage
                .list_attachments(&note.id)
                .expect("list attachment records")
                .iter()
                .any(|candidate| candidate.id == attachment.id)
        );

        cleanup(storage, root);
    }
}

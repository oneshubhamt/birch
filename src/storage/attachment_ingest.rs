use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Instant,
};

use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::{
    editor_document::analyze_editor_state,
    domain::{Attachment, NoteId},
    storage::unix_timestamp,
};

use super::{Storage, StorageError, StorageResult, new_id};
use super::attachments::{
    AttachmentImportProfile, AttachmentStagingMode, ProfiledAttachmentImport, StagedAttachmentImport,
    canonical_attachment_identity, sha256_file, sync_parent_directory,
};

const MAX_PENDING_INGESTS: i64 = 32;
static FINALIZE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug)]
pub(crate) struct PreparedAttachmentImport {
    pub(crate) ingest_id: String,
    pub(crate) attachment_id: String,
    pub(crate) note_id: NoteId,
    pub(crate) file_name: String,
    pub(crate) mime_type: Option<String>,
    pub(crate) profile: AttachmentImportProfile,
    /// Protected imports finalize while the Vault authorization and managed
    /// plaintext owner are still alive, so sensitive filename/MIME metadata is
    /// never written to the ordinary attachment-ingest journal.
    finalized_attachment: Option<Attachment>,
}

#[derive(Clone, Debug)]
pub(crate) struct AttachmentIngestIntent {
    pub(crate) request_id: String,
    pub(crate) attachment_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingAttachmentIngestResource {
    pub(crate) note_id: NoteId,
    pub(crate) path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) mime_type: Option<String>,
}

#[derive(Clone, Debug)]
struct AttachmentIngestJob {
    id: String,
    attachment_id: String,
    note_id: NoteId,
    file_name: String,
    mime_type: Option<String>,
    byte_size: i64,
    staging_relative_path: String,
    request_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl Storage {
    pub(crate) fn prepare_attachment_import_profiled(
        &self,
        note_id: &NoteId,
        source: &Path,
    ) -> StorageResult<PreparedAttachmentImport> {
        self.prepare_attachment_import_for_request_profiled(note_id, None, source)
    }

    pub(crate) fn prepare_attachment_import_for_request_profiled(
        &self,
        note_id: &NoteId,
        request_id: Option<&str>,
        source: &Path,
    ) -> StorageResult<PreparedAttachmentImport> {
        let started = Instant::now();
        // Attachment staging itself is Rustnote-managed plaintext. Hold the
        // note-scoped plaintext boundary from authorization through durable
        // pending-ingest registration so Encrypt cannot pass the zero-pending
        // check in the middle of a large copy.
        let plaintext_authorization = self.acquire_plaintext_operation_for_note(note_id)?;
        if !source.is_file() {
            return Err(StorageError::Validation(format!(
                "attachment is not a file: {}",
                source.display()
            )));
        }

        self.validate_note_accepts_attachment_with_authorization(note_id, &plaintext_authorization)?;
        let pending_count: i64 = self.connection()?.query_row(
            "SELECT COUNT(*)
             FROM attachment_ingest_jobs AS jobs
             LEFT JOIN attachments AS finalized ON finalized.id = jobs.attachment_id
             WHERE finalized.id IS NULL",
            [],
            |row| row.get(0),
        )?;
        if pending_count >= MAX_PENDING_INGESTS {
            return Err(StorageError::Validation(
                "too many attachment imports are waiting to finalize; let the current imports finish first"
                    .to_string(),
            ));
        }

        let file_name = source
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("attachment");
        let (file_name, mime_type) = canonical_attachment_identity(file_name, None)?;
        let request_id = request_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if request_id.as_ref().is_some_and(|value| value.len() > 128) {
            return Err(StorageError::Validation(
                "attachment import request identifier is too long".to_string(),
            ));
        }
        let ingest_id = new_id("ingest");
        let attachment_id = new_id("att");
        let (staging_relative_path, staging_path) =
            self.new_ingest_staging_path(&ingest_id, &file_name)?;
        // Reserve durable ownership before the first filesystem write. A crash
        // or failed cleanup can therefore never turn `.incoming` plaintext into
        // an unowned file.
        let mut owned_plaintext =
            self.reserve_attachment_owned_plaintext(note_id, &staging_relative_path)?;
        if owned_plaintext.path() != staging_path {
            return Err(StorageError::Integrity(
                "attachment staging ownership path mismatch".to_string(),
            ));
        }

        let stage_started = Instant::now();
        let staged_with_clone = match owned_plaintext.copy_from(source) {
            Ok(staged_with_clone) => staged_with_clone,
            Err(error) => {
                return Err(self.cleanup_owned_plaintext_after_error(
                    &mut owned_plaintext,
                    error.into(),
                ));
            }
        };
        let managed_stage = stage_started.elapsed();

        let byte_size = staging_path.metadata()?.len();
        if byte_size > i64::MAX as u64 {
            return Err(self.cleanup_owned_plaintext_after_error(
                &mut owned_plaintext,
                StorageError::Validation("attachment is too large to index".to_string()),
            ));
        }

        let sync_started = Instant::now();
        let sync_result = (|| -> StorageResult<()> {
            File::open(&staging_path)?.sync_all()?;
            sync_parent_directory(&staging_path)
        })();
        if let Err(error) = sync_result {
            return Err(self.cleanup_owned_plaintext_after_error(
                &mut owned_plaintext,
                error,
            ));
        }
        let staging_sync = sync_started.elapsed();

        let mut profile = AttachmentImportProfile {
            byte_size,
            managed_stage,
            staging_sync,
            staging_mode: if staged_with_clone {
                AttachmentStagingMode::Clone
            } else {
                AttachmentStagingMode::Copy
            },
            ..AttachmentImportProfile::default()
        };

        let protected = {
            let connection = self.connection()?;
            super::protection::protected_note_exists(&connection, note_id)?
        };
        if protected {
            let hash_started = Instant::now();
            let (sha256, verified_size) = match sha256_file(&staging_path) {
                Ok(result) => result,
                Err(error) => {
                    return Err(self.cleanup_owned_plaintext_after_error(
                        &mut owned_plaintext,
                        error,
                    ));
                }
            };
            profile.hash_finalize = hash_started.elapsed();
            if verified_size != byte_size {
                return Err(self.cleanup_owned_plaintext_after_error(
                    &mut owned_plaintext,
                    StorageError::Integrity(
                        "protected attachment changed while being staged".to_string(),
                    ),
                ));
            }
            let staged = StagedAttachmentImport {
                attachment_id: Some(attachment_id.clone()),
                file_name: file_name.clone(),
                mime_type: mime_type.clone(),
                staging_relative_path: staging_relative_path.clone(),
                staging_path: staging_path.clone(),
                owned_plaintext: Some(owned_plaintext),
                sha256,
                byte_size,
            };
            let finalized = self.finish_staged_attachment_import_profiled(
                note_id,
                &plaintext_authorization,
                staged,
                None,
                None,
                &mut profile,
            )?;
            profile.prepare_total = started.elapsed();
            profile.total = profile.prepare_total;
            return Ok(PreparedAttachmentImport {
                ingest_id,
                attachment_id,
                note_id: note_id.clone(),
                file_name,
                mime_type,
                profile,
                finalized_attachment: Some(finalized),
            });
        }

        let now = unix_timestamp();
        let job = AttachmentIngestJob {
            id: ingest_id.clone(),
            attachment_id: attachment_id.clone(),
            note_id: note_id.clone(),
            file_name: file_name.clone(),
            mime_type: mime_type.clone(),
            byte_size: i64::try_from(byte_size).map_err(|_| StorageError::Validation(
                "attachment is too large to index".to_string()
            ))?,
            staging_relative_path,
            request_id,
            created_at: now,
            updated_at: now,
        };
        if let Err(error) = self.insert_attachment_ingest_job(&job, &mut owned_plaintext) {
            return Err(self.cleanup_owned_plaintext_after_error(
                &mut owned_plaintext,
                error,
            ));
        }

        let prepare_total = started.elapsed();
        profile.prepare_total = prepare_total;
        profile.total = prepare_total;
        Ok(PreparedAttachmentImport {
            ingest_id,
            attachment_id,
            note_id: note_id.clone(),
            file_name,
            mime_type,
            profile,
            finalized_attachment: None,
        })
    }

    pub(crate) fn finalize_prepared_attachment_import_profiled(
        &self,
        prepared: PreparedAttachmentImport,
    ) -> StorageResult<ProfiledAttachmentImport> {
        if let Some(attachment) = prepared.finalized_attachment {
            return Ok(ProfiledAttachmentImport {
                attachment,
                profile: prepared.profile,
            });
        }
        let _finalize_guard = FINALIZE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.finalize_attachment_ingest_with_profile(&prepared.ingest_id, prepared.profile)
    }

    pub(crate) fn reconcile_pending_attachment_ingests(&self) -> StorageResult<(usize, usize)> {
        let _finalize_guard = FINALIZE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let jobs = self.list_attachment_ingest_jobs()?;
        let mut replayed = 0;
        let mut unresolved = 0;
        for job in jobs {
            match self.finalize_attachment_ingest_job(job, AttachmentImportProfile::default()) {
                Ok(_) => replayed += 1,
                Err(_) => unresolved += 1,
            }
        }
        Ok((replayed, unresolved))
    }

    pub(crate) fn reconcile_pending_attachment_ingests_for_note(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<(usize, usize)> {
        let _finalize_guard = FINALIZE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let jobs = self
            .list_attachment_ingest_jobs()?
            .into_iter()
            .filter(|job| &job.note_id == note_id)
            .collect::<Vec<_>>();
        let mut replayed = 0;
        let mut unresolved = 0;
        for job in jobs {
            match self.finalize_attachment_ingest_job(job, AttachmentImportProfile::default()) {
                Ok(_) => replayed += 1,
                Err(_) => unresolved += 1,
            }
        }
        Ok((replayed, unresolved))
    }

    pub(crate) fn pending_attachment_ingest_resource(
        &self,
        ingest_id: &str,
    ) -> StorageResult<Option<PendingAttachmentIngestResource>> {
        let Some(job) = self.get_attachment_ingest_job(ingest_id)? else {
            return Ok(None);
        };
        let staging_path = self.staging_attachment_path(&job.staging_relative_path)?;
        let path = if staging_path.is_file() {
            staging_path
        } else if let Some(finalized) = self
            .list_attachments(&job.note_id)?
            .into_iter()
            .find(|attachment| attachment.id == job.attachment_id)
        {
            self.attachment_file_path(&finalized)?
        } else {
            return Ok(None);
        };
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(PendingAttachmentIngestResource {
            note_id: job.note_id,
            path,
            file_name: job.file_name,
            mime_type: job.mime_type,
        }))
    }

    pub(crate) fn pending_attachment_ingest_resource_with_authorization(
        &self,
        ingest_id: &str,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Option<PendingAttachmentIngestResource>> {
        let Some(job) = self.get_attachment_ingest_job(ingest_id)? else {
            return Ok(None);
        };
        if authorization.note_id() != &job.note_id {
            return Err(StorageError::Integrity(
                "pending-ingest plaintext authorization belongs to a different note".to_string(),
            ));
        }
        let staging_path = self.staging_attachment_path(&job.staging_relative_path)?;
        let path = if staging_path.is_file() {
            staging_path
        } else if let Some(access) = authorization.access() {
            let Some(resource) = self.protected_attachment_with_access(&job.attachment_id, access)? else {
                return Ok(None);
            };
            resource.path
        } else if let Some(finalized) = self
            .list_attachments_with_authorization(&job.note_id, authorization)?
            .into_iter()
            .find(|attachment| attachment.id == job.attachment_id)
        {
            self.attachment_file_path(&finalized)?
        } else {
            return Ok(None);
        };
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(PendingAttachmentIngestResource {
            note_id: job.note_id,
            path,
            file_name: job.file_name,
            mime_type: job.mime_type,
        }))
    }

    pub(super) fn pending_ingest_staging_paths(&self) -> StorageResult<Vec<String>> {
        Ok(self
            .list_attachment_ingest_jobs()?
            .into_iter()
            .map(|job| job.staging_relative_path)
            .collect())
    }

    fn finalize_attachment_ingest_with_profile(
        &self,
        ingest_id: &str,
        profile: AttachmentImportProfile,
    ) -> StorageResult<ProfiledAttachmentImport> {
        let Some(job) = self.get_attachment_ingest_job(ingest_id)? else {
            return Err(StorageError::Validation(
                "attachment import is no longer pending".to_string(),
            ));
        };
        self.finalize_attachment_ingest_job(job, profile)
    }

    fn finalize_attachment_ingest_job(
        &self,
        job: AttachmentIngestJob,
        mut profile: AttachmentImportProfile,
    ) -> StorageResult<ProfiledAttachmentImport> {
        // Finalization reads/hashes Rustnote-managed plaintext and can move it
        // into the canonical attachment namespace. Keep the note derivative
        // permit for the complete lifetime so Protect/delete must drain this
        // producer before scrubbing or cascading its durable owners.
        let plaintext_authorization = self.acquire_plaintext_operation_for_note(&job.note_id)?;
        let started = Instant::now();
        if let Some(existing) = self
            .list_attachments_with_authorization(&job.note_id, &plaintext_authorization)?
            .into_iter()
            .find(|attachment| attachment.id == job.attachment_id)
        {
            let request_still_pending = match job.request_id.as_deref() {
                Some(request_id) if !existing.embedded => {
                    self.has_pending_attachment_request(&job.note_id, request_id)?
                }
                _ => false,
            };
            if job.request_id.is_none() || existing.embedded || !request_still_pending {
                self.discard_attachment_ingest_job(&job)?;
            }
            profile.byte_size = u64::try_from(existing.byte_size).map_err(|_| {
                StorageError::Integrity("finalized attachment has a negative byte size".to_string())
            })?;
            profile.finalize_total = started.elapsed();
            profile.total = profile.prepare_total.saturating_add(profile.finalize_total);
            return Ok(ProfiledAttachmentImport {
                attachment: existing,
                profile,
            });
        }

        if let Err(error) = self.validate_note_accepts_attachment_with_authorization(
            &job.note_id,
            &plaintext_authorization,
        ) {
            self.discard_attachment_ingest_job(&job)?;
            return Err(error);
        }
        if let Some(request_id) = job.request_id.as_deref()
            && !self.has_pending_attachment_request(&job.note_id, request_id)?
        {
            self.discard_attachment_ingest_job(&job)?;
            return Err(StorageError::Validation(
                "attachment import was cancelled before finalization".to_string(),
            ));
        }

        let staging_path = self.staging_attachment_path(&job.staging_relative_path)?;
        if !staging_path.is_file() {
            self.discard_attachment_ingest_job(&job)?;
            return Err(StorageError::Integrity(format!(
                "pending attachment {} has no managed staging file",
                job.file_name
            )));
        }
        let expected_size = u64::try_from(job.byte_size).map_err(|_| StorageError::Integrity(
            "pending attachment has a negative byte size".to_string()
        ))?;
        if staging_path.metadata()?.len() != expected_size {
            self.discard_attachment_ingest_job(&job)?;
            return Err(StorageError::Integrity(format!(
                "pending attachment {} changed size before finalization",
                job.file_name
            )));
        }

        let hash_started = Instant::now();
        let (sha256, byte_size) = sha256_file(&staging_path)?;
        profile.hash_finalize = hash_started.elapsed();
        profile.byte_size = byte_size;
        if let Some(request_id) = job.request_id.as_deref()
            && !self.has_pending_attachment_request(&job.note_id, request_id)?
        {
            self.discard_attachment_ingest_job(&job)?;
            return Err(StorageError::Validation(
                "attachment import was cancelled while integrity verification was running"
                    .to_string(),
            ));
        }
        if byte_size != expected_size {
            self.discard_attachment_ingest_job(&job)?;
            return Err(StorageError::Integrity(format!(
                "pending attachment {} changed while being verified",
                job.file_name
            )));
        }

        let staged = StagedAttachmentImport {
            attachment_id: Some(job.attachment_id.clone()),
            file_name: job.file_name.clone(),
            mime_type: job.mime_type.clone(),
            staging_relative_path: job.staging_relative_path.clone(),
            staging_path,
            owned_plaintext: None,
            sha256,
            byte_size,
        };
        let attachment = match self.finish_staged_attachment_import_profiled(
            &job.note_id,
            &plaintext_authorization,
            staged,
            None,
            None,
            &mut profile,
        ) {
            Ok(attachment) => attachment,
            Err(error @ StorageError::Validation(_)) => {
                // The note can be trashed, deleted, or lose its durable pending
                // anchor while a multi-gigabyte hash is running. None of those
                // states can become valid by retrying this ingest.
                self.discard_attachment_ingest_job(&job)?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        if job.request_id.is_none() {
            self.delete_attachment_ingest_job(&job.id)?;
        }
        profile.finalize_total = started.elapsed();
        profile.total = profile.prepare_total.saturating_add(profile.finalize_total);
        Ok(ProfiledAttachmentImport {
            attachment,
            profile,
        })
    }

    pub(crate) fn has_pending_attachment_request(
        &self,
        note_id: &NoteId,
        request_id: &str,
    ) -> StorageResult<bool> {
        let connection = self.connection()?;
        let editor_state = connection
            .query_row(
                "SELECT editor_state FROM notes WHERE id = ?1",
                [note_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(editor_state) = editor_state else {
            return Ok(false);
        };
        let analysis = analyze_editor_state(&editor_state).map_err(|error| {
            StorageError::Integrity(format!(
                "saved note contains invalid editor state while recovering attachment ingest: {error}"
            ))
        })?;
        Ok(analysis
            .pending_attachment_request_ids
            .iter()
            .any(|candidate| candidate == request_id))
    }

    pub(super) fn discard_attachment_ingests_for_note(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<usize> {
        let jobs = self
            .list_attachment_ingest_jobs()?
            .into_iter()
            .filter(|job| &job.note_id == note_id)
            .collect::<Vec<_>>();
        let mut removed = 0;
        for job in jobs {
            self.discard_attachment_ingest_job(&job)?;
            removed += 1;
        }
        Ok(removed)
    }

    fn discard_attachment_ingest_job(&self, job: &AttachmentIngestJob) -> StorageResult<()> {
        let path = self.staging_attachment_path(&job.staging_relative_path)?;
        if crate::security::managed_path_is_present(&path)? {
            let mut owned = crate::security::OwnedPlaintext::durable_attachment_ingest_existing(
                path,
                job.id.clone(),
                job.note_id.clone(),
            );
            // I2: the durable ingest row remains authoritative until unlink has
            // succeeded (or the path is already absent). A cleanup failure leaves
            // the row intact for retry/recovery.
            owned.remove_file()?;
        }
        self.delete_attachment_ingest_job(&job.id)
    }


    pub(super) fn validate_note_accepts_attachment_with_authorization(
        &self,
        note_id: &NoteId,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<()> {
        if authorization.note_id() != note_id {
            return Err(StorageError::Integrity(
                "attachment plaintext authorization belongs to a different note".to_string(),
            ));
        }
        let connection = self.connection()?;
        let state = connection
            .query_row(
                "SELECT n.trashed, EXISTS(SELECT 1 FROM vault_note_roots root WHERE root.note_id = n.id)
                 FROM notes n WHERE n.id = ?1",
                [note_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?)),
            )
            .optional()?;
        match state {
            None => Err(StorageError::Validation("note no longer exists".to_string())),
            Some((trashed, _)) if trashed != 0 => Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before adding attachments"
                    .to_string(),
            )),
            Some((_, true)) if authorization.access().is_none() => Err(StorageError::Validation(
                "unlock this encrypted note before adding attachments".to_string(),
            )),
            Some((_, true)) => {
                let access = authorization.access().expect("checked above");
                if !self
                    .privacy_coordinator
                    .read_permit_is_publishable(access.permit())
                    .map_err(|message| StorageError::Integrity(message.to_string()))?
                {
                    return Err(StorageError::Validation(
                        "encrypted note authorization is no longer current".to_string(),
                    ));
                }
                Ok(())
            }
            Some((_, false)) => Ok(()),
        }
    }

    fn new_ingest_staging_path(
        &self,
        ingest_id: &str,
        file_name: &str,
    ) -> StorageResult<(String, PathBuf)> {
        let incoming = self.attachments_directory.join(".incoming");
        fs::create_dir_all(&incoming)?;
        let extension = Path::new(file_name)
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 24
                    && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
            .map(|value| value.to_ascii_lowercase());
        let staging_name = extension
            .map(|extension| format!("{ingest_id}.{extension}"))
            .unwrap_or_else(|| ingest_id.to_string());
        Ok((
            format!(".incoming/{staging_name}"),
            incoming.join(staging_name),
        ))
    }

    fn insert_attachment_ingest_job(
        &self,
        job: &AttachmentIngestJob,
        owned_plaintext: &mut crate::security::OwnedPlaintext,
    ) -> StorageResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending_count: i64 = transaction.query_row(
            "SELECT COUNT(*)
             FROM attachment_ingest_jobs AS jobs
             LEFT JOIN attachments AS finalized ON finalized.id = jobs.attachment_id
             WHERE finalized.id IS NULL",
            [],
            |row| row.get(0),
        )?;
        if pending_count >= MAX_PENDING_INGESTS {
            return Err(StorageError::Validation(
                "too many attachment imports are waiting to finalize; let the current imports finish first"
                    .to_string(),
            ));
        }
        transaction.execute(
            "INSERT INTO attachment_ingest_jobs(
                id, attachment_id, note_id, file_name, mime_type, byte_size,
                staging_relative_path, request_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                job.id,
                job.attachment_id,
                job.note_id,
                job.file_name,
                job.mime_type,
                job.byte_size,
                job.staging_relative_path,
                job.request_id,
                job.created_at,
                job.updated_at,
            ],
        )?;
        self.transfer_owned_plaintext_stage(&transaction, owned_plaintext)?;
        transaction.commit()?;
        self.complete_owned_plaintext_transfer(owned_plaintext)?;
        Ok(())
    }

    fn get_attachment_ingest_job(
        &self,
        ingest_id: &str,
    ) -> StorageResult<Option<AttachmentIngestJob>> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT id, attachment_id, note_id, file_name, mime_type, byte_size,
                        staging_relative_path, request_id, created_at, updated_at
                 FROM attachment_ingest_jobs WHERE id = ?1",
                [ingest_id],
                attachment_ingest_job_from_row,
            )
            .optional()?)
    }

    fn list_attachment_ingest_jobs(&self) -> StorageResult<Vec<AttachmentIngestJob>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, attachment_id, note_id, file_name, mime_type, byte_size,
                    staging_relative_path, request_id, created_at, updated_at
             FROM attachment_ingest_jobs ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], attachment_ingest_job_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn attachment_ingest_intents(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Vec<AttachmentIngestIntent>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT request_id, attachment_id
             FROM attachment_ingest_jobs
             WHERE note_id = ?1 AND request_id IS NOT NULL
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([note_id], |row| {
            Ok(AttachmentIngestIntent {
                request_id: row.get(0)?,
                attachment_id: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn delete_attachment_ingest_job(&self, ingest_id: &str) -> StorageResult<()> {
        self.connection()?.execute(
            "DELETE FROM attachment_ingest_jobs WHERE id = ?1",
            [ingest_id],
        )?;
        Ok(())
    }
}

fn attachment_ingest_job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AttachmentIngestJob> {
    Ok(AttachmentIngestJob {
        id: row.get(0)?,
        attachment_id: row.get(1)?,
        note_id: row.get(2)?,
        file_name: row.get(3)?,
        mime_type: row.get(4)?,
        byte_size: row.get(5)?,
        staging_relative_path: row.get(6)?,
        request_id: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
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
    fn prepared_attachment_import_is_durable_until_finalization() {
        let (storage, root) = test_storage("attachment-finalize");
        let note = storage.create_note(None).expect("create test note");
        let source = root.join("source.txt");
        fs::write(&source, b"durable attachment bytes").expect("write source attachment");

        let prepared = storage
            .prepare_attachment_import_profiled(&note.id, &source)
            .expect("prepare attachment import");
        let pending = storage
            .pending_attachment_ingest_resource(&prepared.ingest_id)
            .expect("read pending attachment resource")
            .expect("pending attachment resource should exist");
        let generic_owners: i64 = storage
            .connection()
            .expect("owner connection")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("count generic owners");
        assert_eq!(
            generic_owners, 0,
            "generic staging ownership must transfer atomically to the ingest row"
        );
        assert!(pending.path.is_file());
        assert_eq!(fs::read(&pending.path).expect("read staged bytes"), b"durable attachment bytes");
        assert!(
            storage
                .list_attachments(&note.id)
                .expect("list attachments before finalization")
                .is_empty()
        );

        let finalized = storage
            .finalize_prepared_attachment_import_profiled(prepared)
            .expect("finalize attachment import");
        let final_path = storage
            .attachment_file_path(&finalized.attachment)
            .expect("resolve canonical attachment path");
        assert_eq!(fs::read(final_path).expect("read finalized bytes"), b"durable attachment bytes");
        assert_eq!(
            storage
                .list_attachments(&note.id)
                .expect("list finalized attachments")
                .len(),
            1
        );

        cleanup(storage, root);
    }

    #[test]
    fn startup_removes_unowned_incoming_but_preserves_durable_ingest_stage() {
        let (storage, root) = test_storage("attachment-startup-owner");
        let note = storage.create_note(None).expect("create test note");
        let source = root.join("pending.txt");
        fs::write(&source, b"durably owned staging").expect("write source attachment");
        let prepared = storage
            .prepare_attachment_import_profiled(&note.id, &source)
            .expect("prepare attachment import");
        let owned_path = storage
            .pending_attachment_ingest_resource(&prepared.ingest_id)
            .expect("pending lookup")
            .expect("pending resource")
            .path;
        let unowned = storage.attachments_directory.join(".incoming/unowned.tmp");
        fs::write(&unowned, b"unowned plaintext").expect("unowned staging");

        drop(storage);
        let reopened = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("startup recovery");
        assert!(!unowned.exists(), "startup must remove unowned .incoming plaintext");
        assert!(owned_path.is_file(), "durably owned ingest staging must survive startup cleanup");
        assert!(
            reopened
                .pending_attachment_ingest_resource(&prepared.ingest_id)
                .expect("pending lookup after reopen")
                .is_some()
        );

        cleanup(reopened, root);
    }

    #[test]
    fn failed_pending_ingest_cleanup_keeps_durable_job_owner() {
        let (storage, root) = test_storage("attachment-ingest-cleanup-owner");
        let note = storage.create_note(None).expect("create test note");
        let source = root.join("pending.txt");
        fs::write(&source, b"durably owned pending ingest").expect("write source attachment");
        let prepared = storage
            .prepare_attachment_import_profiled(&note.id, &source)
            .expect("prepare attachment import");
        let job = storage
            .get_attachment_ingest_job(&prepared.ingest_id)
            .expect("read ingest job")
            .expect("durable ingest job");
        let staging_path = storage
            .staging_attachment_path(&job.staging_relative_path)
            .expect("staging path");

        fs::remove_file(&staging_path).expect("remove staged file for failure simulation");
        fs::create_dir(&staging_path).expect("replace staged file with non-removable directory");
        assert!(
            storage.discard_attachment_ingest_job(&job).is_err(),
            "cleanup must fail rather than forgetting a non-file plaintext path"
        );
        assert!(
            storage
                .get_attachment_ingest_job(&prepared.ingest_id)
                .expect("read ingest owner after failed cleanup")
                .is_some(),
            "failed cleanup must keep the durable attachment-ingest owner"
        );

        cleanup(storage, root);
    }

    #[cfg(unix)]
    #[test]
    fn ambiguous_pending_ingest_presence_keeps_durable_owner_until_retry() {
        use std::os::unix::fs::symlink;

        let (storage, root) = test_storage("attachment-ingest-ambiguous-presence");
        let note = storage.create_note(None).expect("create test note");
        let source = root.join("ambiguous.txt");
        fs::write(&source, b"durably owned ambiguous ingest").expect("write source attachment");
        let prepared = storage
            .prepare_attachment_import_profiled(&note.id, &source)
            .expect("prepare attachment import");
        let job = storage
            .get_attachment_ingest_job(&prepared.ingest_id)
            .expect("read ingest job")
            .expect("durable ingest job");
        let staging_path = storage
            .staging_attachment_path(&job.staging_relative_path)
            .expect("staging path");
        let incoming = staging_path.parent().expect("incoming parent").to_path_buf();
        let preserved_incoming = storage.attachments_directory.join(".incoming-preserved");
        fs::rename(&incoming, &preserved_incoming).expect("move incoming directory");
        symlink(".incoming", &incoming).expect("symlink loop");

        assert!(
            storage.discard_attachment_ingest_job(&job).is_err(),
            "ambiguous metadata must fail cleanup closed"
        );
        assert!(
            storage
                .get_attachment_ingest_job(&prepared.ingest_id)
                .expect("read owner after ambiguous cleanup")
                .is_some(),
            "durable ingest owner must survive ambiguous absence"
        );
        assert!(
            preserved_incoming.join(staging_path.file_name().expect("staging name")).is_file(),
            "underlying plaintext must remain covered by the durable owner"
        );

        fs::remove_file(&incoming).expect("remove symlink loop");
        fs::rename(&preserved_incoming, &incoming).expect("restore incoming directory");
        storage
            .discard_attachment_ingest_job(&job)
            .expect("retry cleanup after repair");
        assert!(
            storage
                .get_attachment_ingest_job(&prepared.ingest_id)
                .expect("read owner after retry")
                .is_none(),
            "owner may retire only after cleanup succeeds"
        );
        assert!(!staging_path.exists(), "retry must remove the managed plaintext");

        cleanup(storage, root);
    }

    #[test]
    fn pending_attachment_ingest_recovery_replays_managed_stage_once() {
        let (storage, root) = test_storage("attachment-recovery");
        let note = storage.create_note(None).expect("create test note");
        let source = root.join("recover.txt");
        fs::write(&source, b"recover me").expect("write source attachment");

        let prepared = storage
            .prepare_attachment_import_profiled(&note.id, &source)
            .expect("prepare attachment import");
        assert!(
            storage
                .pending_attachment_ingest_resource(&prepared.ingest_id)
                .expect("read pending resource")
                .is_some()
        );

        let (replayed, unresolved) = storage
            .reconcile_pending_attachment_ingests_for_note(&note.id)
            .expect("reconcile pending attachment ingest");
        assert_eq!((replayed, unresolved), (1, 0));
        assert_eq!(
            storage
                .list_attachments(&note.id)
                .expect("list recovered attachments")
                .len(),
            1
        );
        let (replayed_again, unresolved_again) = storage
            .reconcile_pending_attachment_ingests_for_note(&note.id)
            .expect("reconcile a second time");
        assert_eq!((replayed_again, unresolved_again), (0, 0));

        cleanup(storage, root);
    }
}

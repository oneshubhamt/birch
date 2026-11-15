//! E2 selective note encryption persistence boundary.
//!
//! SQLite remains the database engine. A protected note keeps only structural
//! routing/state in ordinary tables; sensitive state is split across a small
//! authenticated encrypted object graph keyed by mutation locality. Revisions
//! are immutable encrypted objects and attachment bytes remain independently
//! authenticated, chunked protected blobs. The raw note key exists only in
//! process memory while explicitly unlocked and is password-wrapped at rest.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    attachment_paths::canonical_blob_relative_path,
    domain::{
        Attachment, Citation, Note, NoteDocument, NoteId, NoteProtectionState, NoteRevision,
        NoteRevisionSummary, NoteSourceSummary, RevisionRetention, RevisionSnapshot, Source,
        SourceId, SourceKind,
    },
    editor_document::{EMPTY_EDITOR_STATE, analyze_editor_state},
    editor_projection::derive_editor_projections,
    security::{
        Argon2idParams, DEFAULT_PROTECTED_BLOB_CHUNK_BYTES, OwnedPlaintext, VaultContext,
        VaultDomain, VaultKey, VaultNoteAccess, VaultReadPermit, VaultService,
        sensitive_data::{
            AttachmentIndex, MetadataState, NOTE_PRIVATE_STATE_FORMAT_VERSION, NotePrivateState,
            ProtectedPublicPersistenceSpec, ProtectedScrubStep, RevisionIndex, RevisionObject,
            SensitiveNoteData, SensitivePdfAnnotation, SensitiveSourceLink, VaultBlobReference,
        },
    },
    vault_profile::{VaultProfileSpan, VaultProfileStage},
};

use super::attachments::sha256_file;
use super::vault_objects::{VaultCurrentGraph, VaultManifest, VaultNoteRootRecord};
use super::{
    NoteSaveScope, RevisionMode, Storage, StorageError, StorageResult, new_id, unix_timestamp,
};

const LOCKED_ATTACHMENT_NAME: &str = "Locked attachment";

fn ensure_wal_truncated(connection: &Connection) -> StorageResult<()> {
    let (busy, log_frames, checkpointed_frames): (i64, i64, i64) =
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 || (log_frames >= 0 && checkpointed_frames < log_frames) {
        return Err(StorageError::Integrity(format!(
            "SQLite could not fully truncate the WAL during vault privacy cleanup ({checkpointed_frames}/{log_frames} frames, busy={busy})"
        )));
    }
    Ok(())
}

#[derive(Debug)]
struct RetiredPlaintextBlob {
    original_path: PathBuf,
    plaintext: OwnedPlaintext,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultOperationDetails {
    // Durable recovery metadata must remain opaque with respect to protected
    // plaintext. Ciphertext identities are safe to persist; plaintext hashes
    // are deliberately derived from staged bytes only when recovery runs.
    ciphertext_hashes: Vec<String>,
}

#[derive(Clone, Debug)]
struct VaultOperationRecord {
    note_id: NoteId,
    operation: String,
    phase: String,
    operation_id: String,
    details: VaultOperationDetails,
}

struct ProtectedBlobInput<'a> {
    note_id: &'a NoteId,
    staging: &'a Path,
    source_path: &'a Path,
    plaintext_sha256: String,
    plaintext_byte_size: i64,
}

const VAULT_OPERATION_PROTECT: &str = "protect";
const VAULT_OPERATION_UNPROTECT: &str = "unprotect";
const VAULT_PHASE_PREPARED: &str = "prepared";
const VAULT_PHASE_DATABASE_COMMITTED: &str = "database_committed";

impl Storage {
    fn begin_vault_operation(
        &self,
        note_id: &NoteId,
        operation: &str,
        operation_id: &str,
        details: &VaultOperationDetails,
    ) -> StorageResult<()> {
        let details_json = serde_json::to_string(details)?;
        let now = unix_timestamp();
        let connection = self.connection()?;
        // The prepared journal must reach durable storage before any plaintext
        // attachment is renamed/staged. NORMAL WAL durability is sufficient for
        // routine autosave, but not for this cross-filesystem recovery anchor.
        connection.execute_batch("PRAGMA synchronous = FULL;")?;
        connection.execute(
            "INSERT INTO vault_operations(
                 note_id, operation, phase, operation_id, details_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                note_id,
                operation,
                VAULT_PHASE_PREPARED,
                operation_id,
                details_json,
                now
            ],
        )?;
        Ok(())
    }

    fn complete_vault_operation(&self, note_id: &NoteId) -> StorageResult<()> {
        let connection = self.connection()?;
        connection.execute("DELETE FROM vault_operations WHERE note_id = ?1", [note_id])?;
        Ok(())
    }

    fn mark_vault_operation_database_committed(
        transaction: &Transaction<'_>,
        note_id: &NoteId,
        operation: &str,
        operation_id: &str,
        now: i64,
    ) -> StorageResult<()> {
        let changed = transaction.execute(
            "UPDATE vault_operations
             SET phase = ?1, updated_at = ?2
             WHERE note_id = ?3 AND operation = ?4 AND operation_id = ?5 AND phase = ?6",
            params![
                VAULT_PHASE_DATABASE_COMMITTED,
                now,
                note_id,
                operation,
                operation_id,
                VAULT_PHASE_PREPARED,
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::Integrity(
                "vault transition journal changed unexpectedly".to_string(),
            ));
        }
        Ok(())
    }

    fn load_vault_operations(&self) -> StorageResult<Vec<VaultOperationRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT note_id, operation, phase, operation_id, details_json
             FROM vault_operations
             ORDER BY created_at, note_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut operations = Vec::new();
        for row in rows {
            let (note_id, operation, phase, operation_id, details_json) = row?;
            let note_id = NoteId::parse(note_id).map_err(|_| {
                StorageError::Integrity(
                    "vault transition journal has an invalid note id".to_string(),
                )
            })?;
            if operation_id.is_empty()
                || operation_id.len() > 160
                || !operation_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(StorageError::Integrity(
                    "vault transition journal has an invalid operation id".to_string(),
                ));
            }
            let details =
                serde_json::from_str::<VaultOperationDetails>(&details_json).map_err(|_| {
                    StorageError::Integrity(
                        "vault transition journal has invalid recovery details".to_string(),
                    )
                })?;
            for hash in &details.ciphertext_hashes {
                super::attachments::validate_sha256(hash)?;
            }
            operations.push(VaultOperationRecord {
                note_id,
                operation,
                phase,
                operation_id,
                details,
            });
        }
        Ok(operations)
    }

    fn vault_operation_details(index: &AttachmentIndex) -> VaultOperationDetails {
        VaultOperationDetails {
            ciphertext_hashes: index
                .blobs
                .iter()
                .map(|blob| blob.ciphertext_sha256.clone())
                .collect(),
        }
    }

    fn retiring_operation_directory(&self, operation_id: &str) -> PathBuf {
        self.attachments_directory
            .join(".vault-retiring")
            .join(operation_id)
    }

    fn incoming_operation_directory(&self, operation_id: &str) -> PathBuf {
        self.attachments_directory
            .join(".vault-incoming")
            .join(operation_id)
    }

    /// Recover vault transitions interrupted between filesystem preparation and
    /// SQLite commit/finalization. This runs before normal integrity checks.
    pub(super) fn recover_vault_operations_unlocked(&self) -> StorageResult<()> {
        let operations = self.load_vault_operations()?;
        let active_incoming = operations
            .iter()
            .filter(|operation| operation.operation == VAULT_OPERATION_UNPROTECT)
            .map(|operation| operation.operation_id.as_str())
            .collect::<HashSet<_>>();
        self.cleanup_stale_vault_incoming_unlocked(&active_incoming)?;
        drop(active_incoming);

        for operation in operations {
            match (operation.operation.as_str(), operation.phase.as_str()) {
                (VAULT_OPERATION_PROTECT, VAULT_PHASE_PREPARED) => {
                    self.restore_retiring_operation(&operation.note_id, &operation.operation_id)?;
                    self.delete_ciphertext_blobs_if_unreferenced(
                        operation
                            .details
                            .ciphertext_hashes
                            .iter()
                            .map(String::as_str),
                    )?;
                    self.complete_vault_operation(&operation.note_id)?;
                }
                (VAULT_OPERATION_PROTECT, VAULT_PHASE_DATABASE_COMMITTED) => {
                    let retiring = self.retiring_operation_directory(&operation.operation_id);
                    self.discard_vault_operation_plaintext_directory(
                        &operation.note_id,
                        &operation.operation_id,
                        &retiring,
                    )?;
                    // A crash may have occurred before targeted cache/handoff cleanup
                    // had decrypted protected attachment state available. Conservatively
                    // clear all Rustnote-managed derived namespaces instead of
                    // persisting plaintext attachment hashes in the recovery journal.
                    self.scrub_all_managed_plaintext_namespaces_unlocked()?;
                    self.complete_vault_operation(&operation.note_id)?;
                }
                (VAULT_OPERATION_UNPROTECT, VAULT_PHASE_PREPARED) => {
                    let incoming = self.incoming_operation_directory(&operation.operation_id);
                    self.discard_vault_operation_plaintext_directory(
                        &operation.note_id,
                        &operation.operation_id,
                        &incoming,
                    )?;
                    self.complete_vault_operation(&operation.note_id)?;
                }
                (VAULT_OPERATION_UNPROTECT, VAULT_PHASE_DATABASE_COMMITTED) => {
                    self.finalize_staged_plaintext_operation(
                        &operation.note_id,
                        &operation.operation_id,
                    )?;
                    self.delete_ciphertext_blobs_if_unreferenced(
                        operation
                            .details
                            .ciphertext_hashes
                            .iter()
                            .map(String::as_str),
                    )?;
                    self.complete_vault_operation(&operation.note_id)?;
                }
                _ => {
                    return Err(StorageError::Integrity(format!(
                        "unsupported vault recovery state {} / {}",
                        operation.operation, operation.phase
                    )));
                }
            }
        }
        Ok(())
    }

    fn cleanup_stale_vault_incoming_unlocked(
        &self,
        active_operation_ids: &HashSet<&str>,
    ) -> StorageResult<()> {
        let incoming = self.attachments_directory.join(".vault-incoming");
        let entries = match fs::read_dir(&incoming) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type()?.is_dir() && active_operation_ids.contains(name.as_ref()) {
                continue;
            }
            if entry.file_type()?.is_dir() {
                crate::native::privacy::remove_tree(&path)?;
            } else {
                let _ = crate::native::privacy::remove_file(&path)?;
            }
        }
        let _ = fs::remove_dir(&incoming);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn protect_note_with_password(
        &self,
        note_id: &NoteId,
        password: &[u8],
    ) -> StorageResult<Note> {
        self.protect_note_with_password_and_public_label(note_id, password, "Locked note")
    }

    pub(crate) fn protect_note_with_password_and_public_label(
        &self,
        note_id: &NoteId,
        password: &[u8],
        public_label: &str,
    ) -> StorageResult<Note> {
        let public_label = normalize_locked_note_label(public_label)?;
        validate_password(password)?;
        let privacy_profile = VaultProfileSpan::stage(VaultProfileStage::PrivacyDrains);
        let _privacy_guard = self.begin_note_privacy_transition(
            note_id,
            crate::security::NotePrivacyTransition::Protect,
        )?;
        drop(privacy_profile);
        let preparation_profile = VaultProfileSpan::stage(VaultProfileStage::ProtectPreparation);
        self.discard_managed_plaintext_staging_for_note(note_id)?;
        let connection = self.connection()?;
        if protected_note_exists(&connection, note_id)? {
            return Err(StorageError::Validation(
                "this note is already encrypted".to_string(),
            ));
        }
        ensure_note_protection_ready(&connection, note_id)?;
        let mut note = self
            .note_from_connection(&connection, note_id)?
            .ok_or_else(|| StorageError::Validation("note no longer exists".to_string()))?;
        if note.trashed {
            return Err(StorageError::Validation(
                "recover this note from Trash before encrypting it".to_string(),
            ));
        }
        note.protection = NoteProtectionState::Unprotected;
        let attachments = self.list_attachments_with_connection(&connection, note_id)?;
        let revisions = load_revision_snapshots(self, &connection, note_id)?;
        let sources = load_source_links(&connection, note_id)?;
        let pdf_annotations = load_pdf_annotations(&connection, note_id)?;
        let outgoing_note_links = load_outgoing_note_links(&connection, note_id)?;
        drop(preparation_profile);

        let original_updated_at = note.updated_at;
        let vault = VaultService::new();
        let crypto_profile = VaultProfileSpan::stage(VaultProfileStage::Crypto);
        let note_key = vault.generate_key()?;
        let key_envelope = vault.wrap_key_with_password(
            &note_key,
            password,
            VaultContext::new(VaultDomain::KeyEnvelope, note_id.as_str().as_bytes()),
            Argon2idParams::V1_INTERACTIVE,
        )?;
        drop(crypto_profile);

        let attachments_encrypt_profile =
            VaultProfileSpan::stage(VaultProfileStage::AttachmentsEncrypt);
        let protected_blobs =
            self.protect_note_blobs(&vault, &note_key, note_id, &attachments, &revisions)?;
        drop(attachments_encrypt_profile);
        // `classify` is the exhaustive privacy authority. Revisions are inserted
        // below once their immutable predecessor object IDs are known.
        let sensitive = SensitiveNoteData::classify(
            &note,
            public_label.clone(),
            attachments,
            protected_blobs,
            outgoing_note_links,
            sources,
            pdf_annotations,
            Vec::new(),
        );
        sensitive
            .note_private_state
            .validate_for(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;
        sensitive
            .metadata_state
            .validate_for(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;
        sensitive
            .attachment_index
            .validate_for(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;

        drop(connection);
        let operation_id = new_id("vault-op");
        let operation_details = Self::vault_operation_details(&sensitive.attachment_index);
        if let Err(error) = self.begin_vault_operation(
            note_id,
            VAULT_OPERATION_PROTECT,
            &operation_id,
            &operation_details,
        ) {
            return Err(
                self.cleanup_uncommitted_protected_blobs(&sensitive.attachment_index.blobs, error)
            );
        }
        let attachments_stage_profile =
            VaultProfileSpan::stage(VaultProfileStage::AttachmentsStage);
        let retired_plaintext_result = self.stage_exclusive_plaintext_blobs(
            note_id,
            &sensitive.attachment_index,
            &operation_id,
        );
        drop(attachments_stage_profile);
        let mut retired_plaintext = match retired_plaintext_result {
            Ok(retired) => retired,
            Err(error) => {
                let rollback_result = self
                    .restore_retiring_operation(note_id, &operation_id)
                    .and_then(|()| {
                        self.delete_ciphertext_blobs_if_unreferenced(
                            operation_details
                                .ciphertext_hashes
                                .iter()
                                .map(String::as_str),
                        )
                    });
                return match rollback_result {
                    Ok(()) => {
                        let _ = self.complete_vault_operation(note_id);
                        Err(error)
                    }
                    Err(rollback_error) => Err(StorageError::Integrity(format!(
                        "note encryption failed ({error}) and durable rollback is incomplete ({rollback_error})"
                    ))),
                };
            }
        };

        let object_writes_profile = VaultProfileSpan::stage(VaultProfileStage::ObjectWrites);
        let database_result = (|| -> StorageResult<()> {
            let now = unix_timestamp();
            let mut connection = self.connection()?;
            connection.execute_batch("PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;")?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current_updated_at: Option<i64> = transaction
                .query_row(
                    "SELECT updated_at FROM notes WHERE id = ?1",
                    [note_id],
                    |row| row.get(0),
                )
                .optional()?;
            if current_updated_at != Some(original_updated_at) {
                return Err(StorageError::Validation(
                    "the note changed while encryption was being prepared; try again".to_string(),
                ));
            }
            ensure_note_protection_ready(&transaction, note_id)?;

            let private_id = self.insert_vault_object(
                &transaction,
                &note_key,
                note_id,
                &sensitive.note_private_state,
            )?;
            let metadata_id = self.insert_vault_object(
                &transaction,
                &note_key,
                note_id,
                &sensitive.metadata_state,
            )?;
            let attachment_index_id = self.insert_vault_object(
                &transaction,
                &note_key,
                note_id,
                &sensitive.attachment_index,
            )?;

            let mut revision_head = None;
            let mut revision_floor = None;
            let mut revision_count = 0_u64;
            let mut revision_index_rows = Vec::with_capacity(revisions.len());
            for (revision, snapshot) in &revisions {
                let revision_object = RevisionObject::new(
                    note_id.clone(),
                    revision.id.clone(),
                    revision.revision_no,
                    revision_head.clone(),
                    revision.created_at,
                    snapshot.clone(),
                )
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
                let object_id =
                    self.insert_vault_object(&transaction, &note_key, note_id, &revision_object)?;
                revision_index_rows.push((
                    object_id.clone(),
                    revision_object.revision_no,
                    revision_object.snapshot_byte_size,
                    revision_object.created_at,
                ));
                if revision_floor.is_none() {
                    revision_floor = Some(object_id.clone());
                }
                revision_head = Some(object_id);
                revision_count = revision_count.saturating_add(1);
            }
            revision_index_rows.reverse();
            let revision_index =
                RevisionIndex::from_newest_first(note_id.clone(), revision_index_rows)
                    .map_err(|message| StorageError::Integrity(message.to_string()))?;
            let revision_index_id =
                self.insert_vault_object(&transaction, &note_key, note_id, &revision_index)?;

            let manifest = VaultManifest::new(
                note_id.clone(),
                1,
                private_id,
                metadata_id,
                attachment_index_id,
                revision_index_id,
                revision_head,
                revision_floor,
                revision_count,
            )?;
            let manifest_id =
                self.insert_vault_object(&transaction, &note_key, note_id, &manifest)?;
            self.install_vault_note_root(
                &transaction,
                &note_key,
                note_id,
                &key_envelope,
                &manifest_id,
            )?;
            self.replace_vault_blob_refs(&transaction, note_id, &sensitive.attachment_index)?;
            sanitize_protected_note_rows(
                &transaction,
                note_id,
                &sensitive.public_stub.locked_label,
                &sensitive.attachment_index,
                now,
            )?;
            Self::mark_vault_operation_database_committed(
                &transaction,
                note_id,
                VAULT_OPERATION_PROTECT,
                &operation_id,
                now,
            )?;
            transaction.commit()?;
            Ok(())
        })();
        drop(object_writes_profile);
        if let Err(error) = database_result {
            restore_retired_plaintext_blobs(&mut retired_plaintext)?;
            self.delete_ciphertext_blobs_if_unreferenced(
                sensitive
                    .attachment_index
                    .blobs
                    .iter()
                    .map(|blob| blob.ciphertext_sha256.as_str()),
            )?;
            let _ = self.complete_vault_operation(note_id);
            return Err(error);
        }

        let attachments_cleanup_profile =
            VaultProfileSpan::stage(VaultProfileStage::AttachmentsCleanup);
        let attachment_cleanup_result = discard_retired_plaintext_blobs(&mut retired_plaintext);
        drop(attachments_cleanup_profile);
        if let Err(error) = attachment_cleanup_result.and_then(|()| {
            let managed_residue_profile =
                VaultProfileSpan::stage(VaultProfileStage::ManagedResidueScrub);
            let result = self.scrub_managed_plaintext_residue_after_protection(
                note_id,
                &sensitive.attachment_index,
            );
            drop(managed_residue_profile);
            result
        }) {
            return Err(StorageError::Integrity(format!(
                "the note is encrypted, but Rustnote could not finish vault finalization: {error}"
            )));
        }
        let transition_finalize_profile =
            VaultProfileSpan::stage(VaultProfileStage::TransitionFinalize);
        self.complete_vault_operation(note_id)?;

        self.privacy_coordinator
            .insert_note_key(note_id.clone(), note_key)
            .map_err(vault_runtime_error)?;

        // We still own the Protect transition admission here. Re-entering the
        // public `get_note()` path would try to acquire a fresh protected-note
        // reader and wait for this very transition to end. Preserve the public
        // timestamp written by the protection transaction through the direct
        // stub reader, then return the already-canonical decrypted note state.
        let connection = self.connection()?;
        let public_stub = self
            .note_from_connection(&connection, note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("encrypted note disappeared after commit".to_string())
            })?;
        note.updated_at = public_stub.updated_at;
        note.protection = NoteProtectionState::Unlocked;
        drop(transition_finalize_profile);
        Ok(note)
    }

    /// Close Rustnote-managed plaintext residues after the canonical protection
    /// transaction succeeds. This is intentionally broader than the note rows:
    /// pre-encryption automatic backups, persistent preview caches, attachment
    /// handoffs, SQLite WAL frames and free pages can all contain earlier bytes.
    fn scrub_managed_plaintext_residue_after_protection(
        &self,
        note_id: &NoteId,
        index: &AttachmentIndex,
    ) -> StorageResult<()> {
        let plaintext_hashes = index
            .blobs
            .iter()
            .map(|blob| blob.plaintext_sha256.clone())
            .collect::<HashSet<_>>();
        let attachment_ids = index
            .attachments
            .iter()
            .map(|attachment| attachment.id.clone())
            .collect::<HashSet<_>>();
        self.scrub_managed_plaintext_residue_after_hashes(
            note_id.as_str(),
            plaintext_hashes,
            attachment_ids,
        )
    }

    fn scrub_managed_plaintext_residue_after_hashes(
        &self,
        note_id: &str,
        plaintext_hashes: HashSet<String>,
        attachment_ids: HashSet<String>,
    ) -> StorageResult<()> {
        let mut failures = Vec::new();
        let cache_profile = VaultProfileSpan::stage(VaultProfileStage::CacheCleanup);
        let media_cache = crate::library_paths::media_compat_directory(&self.data_directory);
        for plaintext_hash in &plaintext_hashes {
            if let Err(error) = self.discard_image_rendition_cache_strict(plaintext_hash) {
                failures.push(format!(
                    "image rendition cleanup failed for {plaintext_hash}: {error}"
                ));
            }
            if let Err(error) =
                crate::media_compat::discard_cache_strict(&media_cache, plaintext_hash)
            {
                failures.push(format!(
                    "media compatibility cleanup failed for {plaintext_hash}: {error}"
                ));
            }
        }
        if let Err(error) = self.discard_orphaned_quarantine_for_hashes_strict(&plaintext_hashes) {
            failures.push(format!("attachment quarantine cleanup failed: {error}"));
        }
        drop(cache_profile);

        let staging_profile = VaultProfileSpan::stage(VaultProfileStage::StagingCleanup);
        if let Err(error) = crate::native::staging::discard_staged_derivatives_for_note(note_id) {
            failures.push(format!("plaintext note staging cleanup failed: {error}"));
        }
        if let Err(error) =
            crate::native::staging::discard_staged_derivatives_for_attachments(&attachment_ids)
        {
            failures.push(format!(
                "plaintext derivative staging cleanup failed: {error}"
            ));
        }
        drop(staging_profile);

        let handoff_profile = VaultProfileSpan::stage(VaultProfileStage::HandoffCleanup);
        let mut handoff_source_keys = plaintext_hashes.clone();
        handoff_source_keys.insert(crate::native::handoff::note_handoff_source_key(note_id));
        if let Err(error) = crate::native::handoff::discard_handoffs_for_source_keys(
            &crate::library_paths::attachment_handoff_directory(&self.data_directory),
            &handoff_source_keys,
        ) {
            failures.push(format!("attachment handoff cleanup failed: {error}"));
        }
        drop(handoff_profile);

        let backup_profile = VaultProfileSpan::stage(VaultProfileStage::BackupRepositoryReset);
        if let Err(error) = self.reset_backup_repository_for_vault_unlocked() {
            failures.push(format!("automatic backup reset failed: {error}"));
        }
        drop(backup_profile);

        let sqlite_profile = VaultProfileSpan::stage(VaultProfileStage::SqliteResidueScrub);
        if let Err(error) = self.scrub_sqlite_plaintext_residue_unlocked() {
            failures.push(format!("SQLite residue cleanup failed: {error}"));
        }
        drop(sqlite_profile);

        if failures.is_empty() {
            Ok(())
        } else {
            Err(StorageError::Integrity(failures.join("; ")))
        }
    }

    /// Protected note identifiers are exposed only to startup privacy recovery so
    /// Rustnote can remove protected emergency-journal records without bypassing
    /// the storage boundary. No plaintext is returned here.
    pub(crate) fn protected_note_ids_for_privacy_recovery(&self) -> StorageResult<Vec<NoteId>> {
        let connection = self.connection()?;
        let mut statement =
            connection.prepare("SELECT note_id FROM vault_note_roots ORDER BY note_id")?;
        let rows = statement.query_map([], |row| row.get::<_, NoteId>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn scrub_all_managed_plaintext_namespaces_unlocked(&self) -> StorageResult<()> {
        // Protected attachment IDs remain structural routing metadata even while
        // their names/content are sealed, so crash recovery can target current-process
        // edit/PDF staging without globally deleting another library's staging.
        let protected_attachment_ids = {
            let connection = self.connection()?;
            let mut statement = connection.prepare(
                "SELECT a.id FROM attachments a JOIN vault_note_roots root ON root.note_id = a.note_id",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        crate::native::staging::discard_staged_derivatives_for_attachments(
            &protected_attachment_ids,
        )
        .map_err(|error| {
            StorageError::Integrity(format!(
                "plaintext derivative staging cleanup failed: {error}"
            ))
        })?;

        // A database-committed Protect crash is recovered before any NoteKey is
        // available, so the recovery journal intentionally cannot reveal plaintext
        // attachment hashes. Conservatively clear Rustnote-owned quarantine here;
        // normal live protection uses hash-targeted cleanup and preserves unrelated
        // orphan history.
        match crate::native::privacy::remove_tree(&self.attachments_directory.join(".orphaned")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(StorageError::Io(error)),
        }

        let rendition_root = crate::library_paths::image_rendition_directory(&self.data_directory);
        crate::native::rendition::persistent_rendition_owner(&rendition_root)?
            .remove_tree_strict()?;
        crate::media_compat::discard_all_cache_strict(
            &crate::library_paths::media_compat_directory(&self.data_directory),
        )?;
        crate::native::handoff::discard_all_handoffs(
            &crate::library_paths::attachment_handoff_directory(&self.data_directory),
        )
        .map_err(|error| {
            StorageError::Integrity(format!("attachment handoff cleanup failed: {error}"))
        })?;
        self.reset_backup_repository_for_vault_unlocked()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let _ = super::settings::scrub_persisted_note_list_filter_tags(&transaction)?;
        transaction.commit()?;
        drop(connection);
        self.scrub_sqlite_plaintext_residue_unlocked()?;
        Ok(())
    }

    fn scrub_sqlite_plaintext_residue_unlocked(&self) -> StorageResult<()> {
        let connection = self.connection()?;
        connection.execute_batch("PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;")?;
        // FTS5 keeps its own segment/shadow-table history. Rebuild from current
        // live content so obsolete protected terms are rewritten under secure_delete
        // before the WAL is truncated. Native forensic testing proved that
        // this sequence removes both historical-only and current plaintext markers
        // from the live database and WAL without a VACUUM pass.
        let fts_profile = VaultProfileSpan::stage(VaultProfileStage::Fts);
        connection.execute_batch("INSERT INTO notes_fts(notes_fts) VALUES('rebuild');")?;
        drop(fts_profile);
        let wal_profile = VaultProfileSpan::stage(VaultProfileStage::Wal);
        let result = ensure_wal_truncated(&connection);
        drop(wal_profile);
        result
    }

    /// Durable Vault transitions block autosave mutation admission, but an
    /// already-unlocked protected note remains eligible for encrypted autosave.
    /// The in-memory autosave barrier separately blocks Lock/Protect while their
    /// transition is active and after Lock has revoked the note session.
    pub(crate) fn note_has_pending_vault_operation(&self, note_id: &NoteId) -> StorageResult<bool> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM vault_operations WHERE note_id = ?1)",
                [note_id],
                |row| row.get(0),
            )
            .map_err(StorageError::from)
    }

    /// Authoritative autosave mutation admission after the transition-scoped
    /// worker barrier has been released. Ordinary notes are writable; protected
    /// notes are writable only while their in-memory Vault key is present.
    pub(crate) fn note_autosave_mutation_is_blocked(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<bool> {
        if self.note_has_pending_vault_operation(note_id)? {
            return Ok(true);
        }
        let connection = self.connection()?;
        let protected = protected_note_exists(&connection, note_id)?;
        drop(connection);
        if !protected {
            return Ok(false);
        }
        self.privacy_coordinator
            .note_session_is_active(note_id)
            .map(|unlocked| !unlocked)
            .map_err(vault_runtime_error)
    }

    /// Authoritative plaintext-retention/privacy state. A protected note, or a
    /// note with an unfinished Vault transition, must never write its plaintext
    /// into the emergency journal. This predicate is intentionally stronger than
    /// autosave mutation admission: unlocked protected notes autosave through the
    /// encrypted object model while plaintext recovery artifacts stay forbidden.
    pub(crate) fn note_requires_vault_autosave_barrier(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<bool> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT
                    EXISTS(SELECT 1 FROM vault_note_roots WHERE note_id = ?1)
                    OR EXISTS(SELECT 1 FROM vault_operations WHERE note_id = ?1)",
                [note_id],
                |row| row.get(0),
            )
            .map_err(StorageError::from)
    }

    pub(crate) fn unlock_protected_note(
        &self,
        note_id: &NoteId,
        password: &[u8],
    ) -> StorageResult<Note> {
        validate_password(password)?;
        let privacy_profile = VaultProfileSpan::stage(VaultProfileStage::PrivacyDrains);
        let _privacy_guard = self.begin_note_privacy_transition(
            note_id,
            crate::security::NotePrivacyTransition::Unlock,
        )?;
        drop(privacy_profile);
        let connection = self.connection()?;
        let root = self
            .vault_note_root_record(&connection, note_id)?
            .ok_or_else(|| StorageError::Validation("this note is not encrypted".to_string()))?;
        let vault = VaultService::new();
        let crypto_profile = VaultProfileSpan::stage(VaultProfileStage::Crypto);
        let note_key = vault.unwrap_key_with_password(
            password,
            VaultContext::new(VaultDomain::KeyEnvelope, note_id.as_str().as_bytes()),
            &root.key_envelope,
        )?;
        // Authenticate the canonical root and all current mutable objects before
        // retaining the note key in session memory. Reuse that authenticated graph
        // to materialize the returned note so Unlock decrypts current state once.
        let graph = self
            .read_vault_current_graph(&connection, &note_key, note_id)?
            .ok_or_else(|| StorageError::Integrity("protected note root is missing".to_string()))?;
        let base = self
            .note_from_connection(&connection, note_id)?
            .ok_or_else(|| StorageError::Integrity("unlocked note disappeared".to_string()))?;
        let note = materialize_vault_note(base, &graph)?;
        drop(crypto_profile);
        self.privacy_coordinator
            .insert_note_key(note_id.clone(), note_key)
            .map_err(vault_runtime_error)?;
        Ok(note)
    }

    pub(crate) fn lock_protected_note(&self, note_id: &NoteId) -> StorageResult<Note> {
        let privacy_profile = VaultProfileSpan::stage(VaultProfileStage::PrivacyDrains);
        let _privacy_guard = self
            .begin_note_privacy_transition(note_id, crate::security::NotePrivacyTransition::Lock)?;
        drop(privacy_profile);
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Err(StorageError::Validation(
                "this note is not encrypted".to_string(),
            ));
        }
        let attachment_ids = {
            let mut statement =
                connection.prepare("SELECT id FROM attachments WHERE note_id = ?1 ORDER BY id")?;
            let rows = statement.query_map([note_id], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        drop(connection);
        let attachments_cleanup_profile =
            VaultProfileSpan::stage(VaultProfileStage::AttachmentsCleanup);
        crate::native::staging::discard_staged_derivatives_for_note(note_id.as_str()).map_err(
            |error| {
                StorageError::Integrity(format!(
                    "protected note staging cleanup failed during Lock: {error}"
                ))
            },
        )?;
        crate::native::staging::discard_staged_derivatives_for_attachments(&attachment_ids)
            .map_err(|error| {
                StorageError::Integrity(format!(
                    "protected attachment staging cleanup failed during Lock: {error}"
                ))
            })?;
        drop(attachments_cleanup_profile);
        // Do not call `get_note()` while the Lock admission is held: the
        // protected hydration path acquires note access and would wait for this
        // same transition. Read the public stub directly instead.
        let connection = self.connection()?;
        let mut note = self
            .note_from_connection(&connection, note_id)?
            .ok_or_else(|| StorageError::Integrity("locked note disappeared".to_string()))?;
        note.protection = NoteProtectionState::Locked;
        Ok(note)
    }

    pub(crate) fn remove_note_protection_with_password(
        &self,
        note_id: &NoteId,
        password: &[u8],
    ) -> StorageResult<Note> {
        validate_password(password)?;
        let privacy_profile = VaultProfileSpan::stage(VaultProfileStage::PrivacyDrains);
        let _privacy_guard = self.begin_note_privacy_transition(
            note_id,
            crate::security::NotePrivacyTransition::Unprotect,
        )?;
        drop(privacy_profile);
        let connection = self.connection()?;
        let root = self
            .vault_note_root_record(&connection, note_id)?
            .ok_or_else(|| StorageError::Validation("this note is not encrypted".to_string()))?;
        let vault = VaultService::new();
        let crypto_profile = VaultProfileSpan::stage(VaultProfileStage::Crypto);
        let note_key = vault.unwrap_key_with_password(
            password,
            VaultContext::new(VaultDomain::KeyEnvelope, note_id.as_str().as_bytes()),
            &root.key_envelope,
        )?;
        let graph = self
            .read_vault_current_graph(&connection, &note_key, note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let revisions =
            self.read_vault_revision_chain(&connection, &note_key, note_id, &graph.manifest)?;
        let base = self
            .note_from_connection(&connection, note_id)?
            .ok_or_else(|| StorageError::Validation("note no longer exists".to_string()))?;
        drop(connection);
        drop(crypto_profile);

        let operation_id = new_id("vault-op");
        let operation_details = Self::vault_operation_details(&graph.attachment_index);
        self.begin_vault_operation(
            note_id,
            VAULT_OPERATION_UNPROTECT,
            &operation_id,
            &operation_details,
        )?;
        let attachments_decrypt_profile =
            VaultProfileSpan::stage(VaultProfileStage::AttachmentsDecrypt);
        let plaintext_stage_result = self.stage_plaintext_blobs(
            &vault,
            &note_key,
            note_id,
            &graph.attachment_index,
            &operation_id,
        );
        drop(attachments_decrypt_profile);
        if let Err(error) = plaintext_stage_result {
            let staging = self.incoming_operation_directory(&operation_id);
            return match self.discard_vault_operation_plaintext_directory(
                note_id,
                &operation_id,
                &staging,
            ) {
                Ok(()) => {
                    let _ = self.complete_vault_operation(note_id);
                    Err(error)
                }
                Err(cleanup_error) => Err(StorageError::Integrity(format!(
                    "note decryption staging failed ({error}) and plaintext cleanup is incomplete ({cleanup_error})"
                ))),
            };
        }

        let object_writes_profile = VaultProfileSpan::stage(VaultProfileStage::ObjectWrites);
        let database_result = (|| -> StorageResult<()> {
            let mut connection = self.connection()?;
            connection.execute_batch("PRAGMA synchronous = FULL;")?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if !protected_note_exists(&transaction, note_id)? {
                return Err(StorageError::Validation(
                    "note encryption changed while it was being unlocked".to_string(),
                ));
            }
            restore_protected_note_rows(self, &transaction, &base, &graph, &revisions)?;
            transaction.execute("DELETE FROM vault_note_roots WHERE note_id = ?1", [note_id])?;
            transaction.execute("DELETE FROM vault_objects WHERE note_id = ?1", [note_id])?;
            let now = unix_timestamp();
            Self::mark_vault_operation_database_committed(
                &transaction,
                note_id,
                VAULT_OPERATION_UNPROTECT,
                &operation_id,
                now,
            )?;
            transaction.commit()?;
            Ok(())
        })();
        drop(object_writes_profile);
        if let Err(error) = database_result {
            let staging = self.incoming_operation_directory(&operation_id);
            return match self.discard_vault_operation_plaintext_directory(
                note_id,
                &operation_id,
                &staging,
            ) {
                Ok(()) => {
                    let _ = self.complete_vault_operation(note_id);
                    Err(error)
                }
                Err(cleanup_error) => Err(StorageError::Integrity(format!(
                    "note protection removal failed ({error}) and plaintext rollback is incomplete ({cleanup_error})"
                ))),
            };
        }

        let attachments_finalize_profile =
            VaultProfileSpan::stage(VaultProfileStage::AttachmentsFinalize);
        if let Err(error) = self.finalize_staged_plaintext_operation(note_id, &operation_id) {
            return Err(StorageError::Integrity(format!(
                "note encryption was removed, but plaintext attachment finalization is incomplete: {error}"
            )));
        }
        self.delete_ciphertext_blobs_if_unreferenced(
            operation_details
                .ciphertext_hashes
                .iter()
                .map(String::as_str),
        )?;
        drop(attachments_finalize_profile);
        self.complete_vault_operation(note_id)?;
        self.get_note(note_id)?.ok_or_else(|| {
            StorageError::Integrity("decrypted note disappeared after commit".to_string())
        })
    }

    pub(super) fn hydrate_protected_note(&self, mut base: Note) -> StorageResult<Note> {
        if !base.protection.is_protected() {
            return Ok(base);
        }
        let note_id = base.id.clone();
        let Some(hydrated) = self.with_unlocked_graph(&note_id, |graph| {
            materialize_vault_note(base.clone(), &graph)
        })?
        else {
            base.protection = NoteProtectionState::Locked;
            return Ok(base);
        };
        Ok(hydrated)
    }

    pub(super) fn hydrate_protected_summary(
        &self,
        mut summary: crate::domain::NoteSummary,
    ) -> StorageResult<crate::domain::NoteSummary> {
        if !summary.protection.is_protected() {
            return Ok(summary);
        }
        let note_id = summary.id.clone();
        let Some((title, preview_text, tags, attachment_names)) =
            self.with_unlocked_graph(&note_id, |graph| {
                let projections = derive_editor_projections(&graph.note_private_state.editor_state)
                    .map_err(StorageError::from)?;
                let title = graph.note_private_state.title;
                let preview_text = projections.plain_text.chars().take(600).collect();
                let tags = graph.metadata_state.tags;
                let attachment_names = graph
                    .attachment_index
                    .attachments
                    .into_iter()
                    .filter(|attachment| attachment.embedded)
                    .map(|attachment| attachment.file_name)
                    .collect();
                Ok((title, preview_text, tags, attachment_names))
            })?
        else {
            summary.protection = NoteProtectionState::Locked;
            // `notes.title` is the user-approved public locked-note label for a
            // protected row. Keep that non-secret label in list summaries while
            // scrubbing every private projection. Clearing it here made the note
            // list fall back to the generic "Locked note" even though the locked
            // note pane correctly retained the configured public label.
            summary.preview_text.clear();
            summary.tags.clear();
            summary.attachment_names.clear();
            return Ok(summary);
        };
        summary.protection = NoteProtectionState::Unlocked;
        summary.title = title;
        summary.preview_text = preview_text;
        summary.tags = tags;
        summary.attachment_names = attachment_names;
        Ok(summary)
    }

    pub(super) fn protected_revision_summaries_if_unlocked(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Option<Vec<NoteRevisionSummary>>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        drop(connection);
        let Some(access) = self
            .privacy_coordinator
            .acquire_note_access(note_id)
            .map_err(vault_runtime_error)?
        else {
            return Ok(Some(Vec::new()));
        };
        let connection = self.connection()?;
        let head = self
            .read_vault_graph_head(&connection, access.key(), note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let revision_index =
            self.read_vault_revision_index(&connection, access.key(), note_id, &head.manifest)?;
        let summaries = revision_index
            .entries
            .into_iter()
            .map(|entry| NoteRevisionSummary {
                // The UI handle is the opaque encrypted object ID already
                // authenticated by the compact revision index. Listing history
                // never decrypts unrelated revision snapshots.
                id: entry.object_id.as_str().to_string(),
                note_id: note_id.clone(),
                revision_no: entry.revision_no,
                byte_size: entry.snapshot_byte_size,
                created_at: entry.created_at,
            })
            .collect::<Vec<_>>();
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(summaries))
    }

    pub(super) fn protected_revision_snapshot_if_unlocked(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> StorageResult<Option<RevisionSnapshot>> {
        self.protected_revision_snapshot_resource_if_unlocked(note_id, revision_id)
            .map(|resource| resource.map(|resource| resource.snapshot().clone()))
    }

    pub(super) fn protected_revision_snapshot_resource_if_unlocked(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> StorageResult<Option<super::RevisionSnapshotResource>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        drop(connection);
        let access = self
            .privacy_coordinator
            .acquire_note_access(note_id)
            .map_err(vault_runtime_error)?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note to view its version history".to_string(),
                )
            })?;
        let connection = self.connection()?;
        let head = self
            .read_vault_graph_head(&connection, access.key(), note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let revision = self
            .read_vault_revision_by_handle(
                &connection,
                access.key(),
                note_id,
                &head.manifest,
                revision_id,
            )?
            .ok_or_else(|| StorageError::Validation("revision no longer exists".to_string()))?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "encrypted note was locked while loading its version history".to_string(),
            ));
        }
        Ok(Some(super::RevisionSnapshotResource::protected(
            revision.snapshot,
            access.into_permit(),
        )))
    }

    pub(super) fn restore_protected_revision_if_unlocked(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> StorageResult<
        Option<(
            crate::domain::NoteRevisionRestoreResult,
            super::RevisionRestoreAuthorization,
        )>,
    > {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        drop(connection);
        let restore_guard = self.begin_protected_note_restore(note_id)?.ok_or_else(|| {
            StorageError::Validation(
                "unlock this encrypted note before restoring its version history".to_string(),
            )
        })?;
        let access = restore_guard.access();
        let connection = self.connection()?;
        let base = self
            .note_from_connection(&connection, note_id)?
            .ok_or_else(|| StorageError::Validation("note no longer exists".to_string()))?;
        if base.trashed {
            return Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before restoring a version"
                    .to_string(),
            ));
        }
        let graph = self
            .read_vault_current_graph(&connection, access.key(), note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let selected = self
            .read_vault_revision_by_handle(
                &connection,
                access.key(),
                note_id,
                &graph.manifest,
                revision_id,
            )?
            .ok_or_else(|| StorageError::Validation("revision no longer exists".to_string()))?;
        let snapshot = selected.snapshot;
        let mut restored_note = super::history::materialize_revision_note(&snapshot)?;
        restored_note.id = note_id.clone();
        restored_note.notebook_id = base.notebook_id.clone();
        restored_note.pinned = base.pinned;
        restored_note.archived = base.archived;
        restored_note.trashed = base.trashed;
        restored_note.purge_after = base.purge_after;
        restored_note.created_at = base.created_at;
        let now = unix_timestamp().max(base.updated_at);
        restored_note.updated_at = now;
        restored_note.protection = NoteProtectionState::Unlocked;

        let mut restored_attachments = snapshot
            .attachments
            .iter()
            .map(|attachment| {
                super::history::revision_attachment_to_attachment(note_id, attachment)
            })
            .collect::<StorageResult<Vec<_>>>()?;
        restored_attachments.sort_by_key(|attachment| attachment.source_attachment_id.is_some());
        for attachment in &restored_attachments {
            let blob = graph
                .attachment_index
                .blobs
                .iter()
                .find(|blob| blob.plaintext_sha256 == attachment.sha256)
                .ok_or_else(|| {
                    StorageError::Integrity(format!(
                        "protected revision attachment {} has no encrypted blob mapping",
                        attachment.id
                    ))
                })?;
            if blob.plaintext_byte_size != attachment.byte_size {
                return Err(StorageError::Integrity(format!(
                    "protected revision attachment {} has an inconsistent encrypted blob mapping",
                    attachment.id
                )));
            }
        }
        restored_note.attachment_names = restored_attachments
            .iter()
            .filter(|attachment| attachment.embedded)
            .map(|attachment| attachment.file_name.clone())
            .collect();
        let analysis =
            analyze_editor_state(restored_note.editor_state()).map_err(StorageError::from)?;

        let current_note = materialize_vault_note(base.clone(), &graph)?;
        let current_snapshot =
            RevisionSnapshot::new(&current_note, graph.attachment_index.attachments.clone());
        let appended_revision = self.prepare_next_vault_revision(
            &connection,
            access.key(),
            note_id,
            &graph.manifest,
            current_snapshot,
            now,
        )?;
        drop(connection);

        let private_state = NotePrivateState {
            format_version: NOTE_PRIVATE_STATE_FORMAT_VERSION,
            note_id: note_id.clone(),
            title: restored_note.title.clone(),
            title_style: restored_note.title_style.clone().normalized(),
            editor_state: restored_note.editor_state().to_string(),
        };
        let mut metadata_state = graph.metadata_state.clone();
        metadata_state.tags = restored_note.tags.clone();
        metadata_state.outgoing_note_links = analysis.note_links;
        metadata_state
            .validate_for(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;
        let mut attachment_index = graph.attachment_index.clone();
        attachment_index.attachments = restored_attachments.clone();
        attachment_index
            .validate_for(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.commit_vault_graph_update(
            &transaction,
            access,
            &graph.root,
            &graph.manifest,
            Some(&private_state),
            Some(&metadata_state),
            Some(&attachment_index),
            appended_revision.as_ref(),
        )?;
        replace_protected_attachment_stubs(&transaction, note_id, &attachment_index, now)?;
        transaction.execute(
            "UPDATE notes SET updated_at = ?1 WHERE id = ?2",
            params![now, note_id],
        )?;
        transaction.commit()?;

        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "the encrypted note was locked while its revision was being restored".to_string(),
            ));
        }
        let retained_access = restore_guard.into_retained_access();
        Ok(Some((
            crate::domain::NoteRevisionRestoreResult {
                note: restored_note,
                warnings: Vec::new(),
            },
            super::RevisionRestoreAuthorization::protected(retained_access),
        )))
    }

    pub(super) fn persist_protected_runtime_normalization_if_unlocked(
        &self,
        note: &Note,
    ) -> StorageResult<bool> {
        let note_id = &note.id;
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(false);
        }
        drop(connection);
        let mutation = self
            .begin_protected_note_mutation(note_id)?
            .ok_or_else(|| StorageError::Validation("encrypted note is locked".to_string()))?;
        self.persist_protected_runtime_normalization_with_access(note, mutation.access())?;
        Ok(true)
    }

    pub(super) fn persist_protected_runtime_normalization_after_restore(
        &self,
        note: &Note,
        authorization: &super::RevisionRestoreAuthorization,
    ) -> StorageResult<()> {
        let access = authorization.access();
        if access.permit().note_id() != &note.id {
            return Err(StorageError::Integrity(
                "revision-restore authorization belongs to another note".to_string(),
            ));
        }
        let _mutation_guard = self
            .begin_retained_protected_note_mutation(access)?
            .ok_or_else(|| {
                StorageError::Validation(
                    "encrypted note was locked before runtime normalization".to_string(),
                )
            })?;
        self.persist_protected_runtime_normalization_with_access(note, access)
    }

    fn persist_protected_runtime_normalization_with_access(
        &self,
        note: &Note,
        access: &VaultNoteAccess,
    ) -> StorageResult<()> {
        let note_id = &note.id;
        let connection = self.connection()?;
        let head = self
            .read_vault_graph_head(&connection, access.key(), note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let current_private: NotePrivateState = self.read_vault_object(
            &connection,
            access.key(),
            note_id,
            &head.manifest.note_private_state,
        )?;
        drop(connection);
        let private_state = NotePrivateState {
            format_version: NOTE_PRIVATE_STATE_FORMAT_VERSION,
            note_id: note_id.clone(),
            title: current_private.title,
            title_style: current_private.title_style,
            editor_state: note.editor_state().to_string(),
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        self.commit_vault_graph_update(
            &transaction,
            access,
            &head.root,
            &head.manifest,
            Some(&private_state),
            None,
            None,
            None,
        )?;
        let stub_changed = transaction.execute(
            "UPDATE notes SET updated_at = ?1 WHERE id = ?2",
            params![note.updated_at, note_id],
        )?;
        if stub_changed != 1 {
            return Err(StorageError::Integrity(
                "protected note stub disappeared during runtime normalization".to_string(),
            ));
        }
        transaction.commit()?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "encrypted note was locked during runtime normalization".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn protected_note_mutation_if_unlocked(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Option<crate::security::ProtectedNoteMutationGuard<'_>>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        drop(connection);
        self.begin_protected_note_mutation(note_id)?
            .map(Some)
            .ok_or_else(|| {
                StorageError::Validation("unlock this encrypted note before editing it".to_string())
            })
    }

    pub(super) fn persist_protected_note_content(
        &self,
        transaction: &Transaction<'_>,
        access: &VaultNoteAccess,
        note: &mut Note,
        revision_mode: Option<RevisionMode>,
        scope: NoteSaveScope,
    ) -> StorageResult<bool> {
        if !scope.touches_document() && !scope.touches_title() && !scope.touches_tags() {
            return Ok(false);
        }
        let note_id = note.id.clone();
        if access.permit().note_id() != &note_id {
            return Err(StorageError::Integrity(
                "protected note save authorization belongs to another note".to_string(),
            ));
        }

        // Read the encrypted root/manifest first, then open only object classes
        // required by this mutation. In particular, ordinary text/title typing
        // must not decrypt MetadataState or AttachmentIndex merely to rewrite
        // NotePrivateState.
        let head = self
            .read_vault_graph_head(transaction, access.key(), &note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;

        let needs_private_state = scope.touches_title() || scope.touches_document();
        let mut private_states = if needs_private_state {
            let current: NotePrivateState = self.read_vault_object(
                transaction,
                access.key(),
                &note_id,
                &head.manifest.note_private_state,
            )?;
            Some((current.clone(), current))
        } else {
            None
        };

        let mut metadata_states = if scope.touches_tags() {
            let current: MetadataState = self.read_vault_object(
                transaction,
                access.key(),
                &note_id,
                &head.manifest.metadata_state,
            )?;
            Some((current.clone(), current))
        } else {
            None
        };
        let mut attachment_indexes: Option<(AttachmentIndex, AttachmentIndex)> = None;

        if scope.touches_title() {
            note.title_style = std::mem::take(&mut note.title_style).normalized();
            let (_, next_private) = private_states.as_mut().ok_or_else(|| {
                StorageError::Integrity("protected title save is missing private state".to_string())
            })?;
            next_private.title = note.title.clone();
            next_private.title_style = note.title_style.clone();
        }

        if scope.touches_tags() {
            note.tags = super::tags::normalized_tags(&note.tags);
            let (_, next_metadata) = metadata_states.as_mut().ok_or_else(|| {
                StorageError::Integrity("protected tag save is missing metadata state".to_string())
            })?;
            next_metadata.tags = note.tags.clone();
        }

        if scope.touches_document() {
            let new_analysis =
                analyze_editor_state(note.editor_state()).map_err(StorageError::from)?;
            let projections = derive_editor_projections(note.editor_state()).map_err(|error| {
                StorageError::Integrity(format!(
                    "protected editor projection derivation failed: {error}"
                ))
            })?;
            let (current_private, next_private) = private_states.as_mut().ok_or_else(|| {
                StorageError::Integrity(
                    "protected document save is missing private state".to_string(),
                )
            })?;
            let old_analysis =
                analyze_editor_state(&current_private.editor_state).map_err(StorageError::from)?;
            next_private.editor_state = note.editor_state().to_string();

            if new_analysis.note_links != old_analysis.note_links {
                if metadata_states.is_none() {
                    let current: MetadataState = self.read_vault_object(
                        transaction,
                        access.key(),
                        &note_id,
                        &head.manifest.metadata_state,
                    )?;
                    metadata_states = Some((current.clone(), current));
                }
                metadata_states
                    .as_mut()
                    .expect("metadata state was initialized above")
                    .1
                    .outgoing_note_links = new_analysis.note_links.clone();
            }

            if new_analysis.attachment_ids != old_analysis.attachment_ids {
                let current: AttachmentIndex = self.read_vault_object(
                    transaction,
                    access.key(),
                    &note_id,
                    &head.manifest.attachment_index,
                )?;
                let mut next = current.clone();
                note.attachment_names =
                    reconcile_protected_attachment_index(&mut next, &new_analysis.attachment_ids)?;
                attachment_indexes = Some((current, next));
            }

            note.document = Arc::new(NoteDocument::new(
                projections.markdown,
                Arc::new(next_private.editor_state.clone()),
                projections.plain_text,
            ));
        }
        note.protection = NoteProtectionState::Unlocked;

        let now = unix_timestamp();
        let append_revision = if let Some(mode) = revision_mode {
            let due = match (mode, head.manifest.revision_head.as_ref()) {
                (RevisionMode::Autosave, Some(revision_head)) => {
                    let latest: RevisionObject =
                        self.read_vault_object(transaction, access.key(), &note_id, revision_head)?;
                    now.saturating_sub(latest.created_at)
                        >= super::history::AUTOSAVE_REVISION_INTERVAL_SECONDS
                }
                _ => true,
            };
            if due {
                if attachment_indexes.is_none() {
                    let current: AttachmentIndex = self.read_vault_object(
                        transaction,
                        access.key(),
                        &note_id,
                        &head.manifest.attachment_index,
                    )?;
                    attachment_indexes = Some((current.clone(), current));
                }
                let revision_attachments = attachment_indexes
                    .as_ref()
                    .expect("attachment index was initialized for revision creation")
                    .1
                    .attachments
                    .iter()
                    .filter(|attachment| attachment.embedded)
                    .cloned()
                    .collect();
                let snapshot = RevisionSnapshot::new(note, revision_attachments);
                self.prepare_next_vault_revision(
                    transaction,
                    access.key(),
                    &note_id,
                    &head.manifest,
                    snapshot,
                    now,
                )?
            } else {
                None
            }
        } else {
            None
        };
        let revision_recorded = append_revision.is_some();

        let private_changed = private_states
            .as_ref()
            .is_some_and(|(current, next)| current != next);
        let metadata_changed = metadata_states
            .as_ref()
            .is_some_and(|(current, next)| current != next);
        let index_changed = attachment_indexes
            .as_ref()
            .is_some_and(|(current, next)| current != next);

        if private_changed || metadata_changed || index_changed || append_revision.is_some() {
            self.commit_vault_graph_update(
                transaction,
                access,
                &head.root,
                &head.manifest,
                private_states
                    .as_ref()
                    .and_then(|(_, next)| private_changed.then_some(next)),
                metadata_states
                    .as_ref()
                    .and_then(|(_, next)| metadata_changed.then_some(next)),
                attachment_indexes
                    .as_ref()
                    .and_then(|(_, next)| index_changed.then_some(next)),
                append_revision.as_ref(),
            )?;
        }

        let changed = transaction.execute(
            "UPDATE notes SET updated_at = ?1 WHERE id = ?2",
            params![note.updated_at, &note_id],
        )?;
        if changed != 1 {
            return Err(StorageError::Integrity(
                "protected note public stub disappeared during save".to_string(),
            ));
        }
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "the encrypted note was locked while it was being saved".to_string(),
            ));
        }
        Ok(revision_recorded)
    }

    pub(super) fn mutate_protected_metadata_if_unlocked<R, F>(
        &self,
        note_id: &NoteId,
        mutate: F,
    ) -> StorageResult<Option<R>>
    where
        F: FnOnce(&mut MetadataState) -> StorageResult<R>,
    {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        let trashed: bool = connection.query_row(
            "SELECT trashed FROM notes WHERE id = ?1",
            [note_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )?;
        if trashed {
            return Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before editing metadata"
                    .to_string(),
            ));
        }
        drop(connection);

        let mutation_guard = self
            .begin_protected_note_mutation(note_id)?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note before editing metadata".to_string(),
                )
            })?;
        let access = mutation_guard.access();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result =
            self.mutate_protected_metadata_with_access(&transaction, access, note_id, mutate)?;
        transaction.commit()?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "the encrypted note was locked while metadata was being saved".to_string(),
            ));
        }
        Ok(Some(result))
    }

    pub(super) fn mutate_protected_metadata_with_access<R, F>(
        &self,
        transaction: &Transaction<'_>,
        access: &VaultNoteAccess,
        note_id: &NoteId,
        mutate: F,
    ) -> StorageResult<R>
    where
        F: FnOnce(&mut MetadataState) -> StorageResult<R>,
    {
        if access.permit().note_id() != note_id {
            return Err(StorageError::Integrity(
                "protected metadata mutation authorization belongs to another note".to_string(),
            ));
        }
        let head = self
            .read_vault_graph_head(transaction, access.key(), note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let current_metadata: MetadataState = self.read_vault_object(
            transaction,
            access.key(),
            note_id,
            &head.manifest.metadata_state,
        )?;
        let mut metadata = current_metadata.clone();
        let result = mutate(&mut metadata)?;
        if metadata != current_metadata {
            metadata
                .validate_for(note_id)
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
            self.commit_vault_graph_update(
                transaction,
                access,
                &head.root,
                &head.manifest,
                None,
                Some(&metadata),
                None,
                None,
            )?;
            transaction.execute(
                "UPDATE notes SET updated_at = ?1 WHERE id = ?2",
                params![unix_timestamp(), note_id],
            )?;
        }
        Ok(result)
    }

    pub(super) fn persist_protected_attachment_from_staging_if_unlocked(
        &self,
        attachment: &Attachment,
        staging_path: &Path,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Option<Attachment>> {
        if authorization.note_id() != &attachment.note_id {
            return Err(StorageError::Integrity(
                "attachment plaintext authorization belongs to a different note".to_string(),
            ));
        }
        let connection = self.connection()?;
        if !protected_note_exists(&connection, &attachment.note_id)? {
            return Ok(None);
        }
        let trashed: bool = connection.query_row(
            "SELECT trashed FROM notes WHERE id = ?1",
            [&attachment.note_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )?;
        if trashed {
            return Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before adding attachments"
                    .to_string(),
            ));
        }
        drop(connection);

        let access = authorization.access().ok_or_else(|| {
            StorageError::Validation(
                "unlock this encrypted note before adding attachments".to_string(),
            )
        })?;
        let _mutation = self
            .begin_plaintext_operation_mutation(authorization)?
            .ok_or_else(|| {
                StorageError::Validation(
                    "encrypted note authorization expired before attachment commit".to_string(),
                )
            })?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let head = self
            .read_vault_graph_head(&transaction, access.key(), &attachment.note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let current_index: AttachmentIndex = self.read_vault_object(
            &transaction,
            access.key(),
            &attachment.note_id,
            &head.manifest.attachment_index,
        )?;
        if current_index
            .attachments
            .iter()
            .any(|candidate| candidate.id == attachment.id)
        {
            return Err(StorageError::Validation(
                "attachment identity already exists in this encrypted note".to_string(),
            ));
        }
        if let Some(source_id) = attachment.source_attachment_id.as_deref() {
            if source_id == attachment.id {
                return Err(StorageError::Validation(
                    "an edited image cannot use itself as its source".to_string(),
                ));
            }
            if !current_index
                .attachments
                .iter()
                .any(|candidate| candidate.id == source_id)
            {
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

        let mut index = current_index.clone();
        let mut newly_encrypted = None;
        match index
            .blobs
            .iter()
            .find(|blob| blob.plaintext_sha256 == attachment.sha256)
        {
            Some(blob) if blob.plaintext_byte_size != attachment.byte_size => {
                return Err(StorageError::Integrity(
                    "protected attachment hash has inconsistent byte sizes".to_string(),
                ));
            }
            Some(blob) => {
                let ciphertext = self.blob_path_for_hash(&blob.ciphertext_sha256)?;
                if !ciphertext.is_file() {
                    return Err(StorageError::Integrity(
                        "protected attachment ciphertext blob is missing".to_string(),
                    ));
                }
            }
            None => {
                let staging = self.attachments_directory.join(".vault-incoming");
                crate::native::private_temp::ensure_private_directory(&staging)?;
                let blob = self.protect_note_blob_from_path(
                    &VaultService::new(),
                    access.key(),
                    ProtectedBlobInput {
                        note_id: &attachment.note_id,
                        staging: &staging,
                        source_path: staging_path,
                        plaintext_sha256: attachment.sha256.clone(),
                        plaintext_byte_size: attachment.byte_size,
                    },
                )?;
                index.blobs.push(blob.clone());
                newly_encrypted = Some(blob);
                let _ = fs::remove_dir(&staging);
            }
        }
        index.attachments.push(attachment.clone());
        index
            .validate_for(&attachment.note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;

        let result = (|| -> StorageResult<()> {
            self.commit_vault_graph_update(
                &transaction,
                access,
                &head.root,
                &head.manifest,
                None,
                None,
                Some(&index),
                None,
            )?;
            replace_protected_attachment_stubs(
                &transaction,
                &attachment.note_id,
                &index,
                unix_timestamp(),
            )?;
            transaction.execute(
                "UPDATE notes SET updated_at = ?1 WHERE id = ?2",
                params![unix_timestamp(), &attachment.note_id],
            )?;
            transaction.commit()?;
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(blob) = newly_encrypted {
                return Err(self.cleanup_uncommitted_protected_blobs(&[blob], error));
            }
            return Err(error);
        }
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "the encrypted note was locked while the attachment was being saved".to_string(),
            ));
        }
        Ok(Some(attachment.clone()))
    }

    pub(super) fn remove_protected_attachment_if_unlocked(
        &self,
        attachment_id: &str,
    ) -> StorageResult<Option<()>> {
        let connection = self.connection()?;
        let note_id = connection
            .query_row(
                "SELECT a.note_id FROM attachments a
                 JOIN vault_note_roots root ON root.note_id = a.note_id
                 WHERE a.id = ?1",
                [attachment_id],
                |row| row.get::<_, NoteId>(0),
            )
            .optional()?;
        let Some(note_id) = note_id else {
            return Ok(None);
        };
        let trashed: bool = connection.query_row(
            "SELECT trashed FROM notes WHERE id = ?1",
            [&note_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )?;
        if trashed {
            return Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before removing attachments"
                    .to_string(),
            ));
        }
        drop(connection);

        let mutation = self
            .begin_protected_note_mutation(&note_id)?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note before removing attachments".to_string(),
                )
            })?;
        let access = mutation.access();
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let head = self
            .read_vault_graph_head(&transaction, access.key(), &note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let current_index: AttachmentIndex = self.read_vault_object(
            &transaction,
            access.key(),
            &note_id,
            &head.manifest.attachment_index,
        )?;
        let attachment = current_index
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .cloned()
            .ok_or_else(|| {
                StorageError::Integrity(
                    "protected attachment stub has no encrypted metadata".to_string(),
                )
            })?;
        if attachment.embedded {
            return Err(StorageError::Validation(
                "embedded attachments must be removed from the document; the stored file is retained for undo and version history"
                    .to_string(),
            ));
        }
        if current_index
            .attachments
            .iter()
            .any(|candidate| candidate.source_attachment_id.as_deref() == Some(attachment_id))
        {
            return Err(StorageError::Validation(
                "source image is retained by an edited rendition".to_string(),
            ));
        }
        let mut index = current_index.clone();
        index
            .attachments
            .retain(|candidate| candidate.id != attachment_id);
        let current_metadata: MetadataState = self.read_vault_object(
            &transaction,
            access.key(),
            &note_id,
            &head.manifest.metadata_state,
        )?;
        let mut metadata = current_metadata.clone();
        metadata
            .pdf_annotations
            .retain(|annotation| annotation.attachment_id != attachment_id);
        let metadata_changed = metadata != current_metadata;
        self.commit_vault_graph_update(
            &transaction,
            access,
            &head.root,
            &head.manifest,
            None,
            metadata_changed.then_some(&metadata),
            Some(&index),
            None,
        )?;
        replace_protected_attachment_stubs(&transaction, &note_id, &index, unix_timestamp())?;
        transaction.execute(
            "UPDATE notes SET updated_at = ?1 WHERE id = ?2",
            params![unix_timestamp(), &note_id],
        )?;
        transaction.commit()?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "the encrypted note was locked while the attachment was being removed".to_string(),
            ));
        }
        Ok(Some(()))
    }

    pub(super) fn replace_protected_pdf_annotation_page_if_unlocked(
        &self,
        attachment_id: &str,
        page_index: i64,
        canonical_json: &str,
        annotations_empty: bool,
    ) -> StorageResult<Option<()>> {
        let connection = self.connection()?;
        let Some(note_id) = self.protected_note_id_for_attachment(&connection, attachment_id)?
        else {
            return Ok(None);
        };
        drop(connection);
        self.mutate_protected_metadata_if_unlocked(&note_id, |metadata| {
            metadata.pdf_annotations.retain(|annotation| {
                !(annotation.attachment_id == attachment_id && annotation.page_index == page_index)
            });
            if !annotations_empty {
                metadata.pdf_annotations.push(SensitivePdfAnnotation {
                    attachment_id: attachment_id.to_string(),
                    page_index,
                    annotations_json: canonical_json.to_string(),
                    updated_at: unix_timestamp(),
                });
            }
            Ok(())
        })
    }

    pub(super) fn protected_note_sources_if_unlocked(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Option<Vec<NoteSourceSummary>>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        drop(connection);
        let summaries = self
            .with_unlocked_metadata_state(note_id, |metadata| {
                let mut summaries = metadata
                    .sources
                    .into_iter()
                    .map(|link| NoteSourceSummary {
                        source: link.source,
                        citations: link.citations,
                    })
                    .collect::<Vec<_>>();
                summaries.sort_by(|left, right| {
                    left.source
                        .display_title()
                        .to_lowercase()
                        .cmp(&right.source.display_title().to_lowercase())
                        .then_with(|| left.source.id.cmp(&right.source.id))
                });
                Ok(summaries)
            })?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note to view its sources".to_string(),
                )
            })?;
        Ok(Some(summaries))
    }

    /// Read the authoritative outgoing-link set for an encrypted note without
    /// recreating a plaintext `note_links` shadow. `None` means the source note
    /// is ordinary; `Some(Vec::new())` is the fail-closed view of a locked
    /// encrypted source.
    pub(super) fn protected_outgoing_note_links_if_unlocked(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Option<Vec<NoteId>>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        drop(connection);
        let links = self
            .with_unlocked_metadata_state(note_id, |metadata| Ok(metadata.outgoing_note_links))?
            .unwrap_or_default();
        Ok(Some(links))
    }

    fn protected_note_id_for_attachment(
        &self,
        connection: &Connection,
        attachment_id: &str,
    ) -> StorageResult<Option<NoteId>> {
        connection
            .query_row(
                "SELECT a.note_id FROM attachments a
                 JOIN vault_note_roots root ON root.note_id = a.note_id
                 WHERE a.id = ?1",
                [attachment_id],
                |row| row.get::<_, NoteId>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub(super) fn protected_pdf_annotation_if_unlocked(
        &self,
        attachment_id: &str,
        page_index: i64,
    ) -> StorageResult<Option<Option<(String, i64)>>> {
        let connection = self.connection()?;
        let Some(note_id) = self.protected_note_id_for_attachment(&connection, attachment_id)?
        else {
            return Ok(None);
        };
        drop(connection);
        let annotation = self
            .with_unlocked_metadata_state(&note_id, |metadata| {
                Ok(metadata
                    .pdf_annotations
                    .into_iter()
                    .find(|annotation| {
                        annotation.attachment_id == attachment_id
                            && annotation.page_index == page_index
                    })
                    .map(|annotation| (annotation.annotations_json, annotation.updated_at)))
            })?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note to view its PDF annotations".to_string(),
                )
            })?;
        Ok(Some(annotation))
    }

    pub(super) fn protected_pdf_annotation_rows_if_unlocked(
        &self,
        attachment_id: &str,
        source_byte_limit: usize,
    ) -> StorageResult<Option<Vec<(u32, String)>>> {
        let connection = self.connection()?;
        let Some(note_id) = self.protected_note_id_for_attachment(&connection, attachment_id)?
        else {
            return Ok(None);
        };
        drop(connection);
        let rows = self
            .with_unlocked_metadata_state(&note_id, |metadata| {
                let mut rows = metadata
                    .pdf_annotations
                    .into_iter()
                    .filter(|annotation| annotation.attachment_id == attachment_id)
                    .filter_map(|annotation| {
                        u32::try_from(annotation.page_index)
                            .ok()
                            .map(|page_index| (page_index, annotation.annotations_json))
                    })
                    .collect::<Vec<_>>();
                rows.sort_by_key(|(page_index, _)| *page_index);
                let mut retained = Vec::with_capacity(rows.len());
                let mut source_bytes = 0_usize;
                for row in rows {
                    source_bytes = source_bytes.saturating_add(row.1.len());
                    retained.push(row);
                    if source_bytes > source_byte_limit {
                        break;
                    }
                }
                Ok(retained)
            })?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note to view its PDF annotations".to_string(),
                )
            })?;
        Ok(Some(rows))
    }

    pub(super) fn protected_pdf_annotation_pages_if_unlocked(
        &self,
        attachment_id: &str,
        limit: usize,
    ) -> StorageResult<Option<Vec<u32>>> {
        let connection = self.connection()?;
        let Some(note_id) = self.protected_note_id_for_attachment(&connection, attachment_id)?
        else {
            return Ok(None);
        };
        drop(connection);
        let pages = self
            .with_unlocked_metadata_state(&note_id, |metadata| {
                let mut pages = metadata
                    .pdf_annotations
                    .into_iter()
                    .filter(|annotation| annotation.attachment_id == attachment_id)
                    .filter_map(|annotation| u32::try_from(annotation.page_index).ok())
                    .collect::<Vec<_>>();
                pages.sort_unstable();
                pages.dedup();
                pages.truncate(limit);
                Ok(pages)
            })?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note to view its PDF annotations".to_string(),
                )
            })?;
        Ok(Some(pages))
    }

    pub(super) fn protected_attachments_if_unlocked(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Option<Vec<Attachment>>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        drop(connection);
        let attachments = self
            .with_unlocked_attachment_index(note_id, |index| Ok(index.attachments))?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note to view its attachments".to_string(),
                )
            })?;
        Ok(Some(attachments))
    }

    pub(super) fn protected_attachments_with_access(
        &self,
        note_id: &NoteId,
        access: &VaultNoteAccess,
    ) -> StorageResult<Option<Vec<Attachment>>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        if access.permit().note_id() != note_id {
            return Err(StorageError::Integrity(
                "plaintext operation Vault authorization belongs to a different note".to_string(),
            ));
        }
        let head = self
            .read_vault_graph_head(&connection, access.key(), note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let index: AttachmentIndex = self.read_vault_object(
            &connection,
            access.key(),
            note_id,
            &head.manifest.attachment_index,
        )?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(|message| StorageError::Integrity(message.to_string()))?
        {
            return Err(StorageError::Validation(
                "encrypted note authorization is no longer current".to_string(),
            ));
        }
        Ok(Some(index.attachments))
    }

    pub(super) fn protected_attachment_metadata_with_access(
        &self,
        note_id: &NoteId,
        attachment_id: &str,
        access: &VaultNoteAccess,
    ) -> StorageResult<Option<Attachment>> {
        let connection = self.connection()?;
        if !protected_note_exists(&connection, note_id)? {
            return Ok(None);
        }
        if access.permit().note_id() != note_id {
            return Err(StorageError::Integrity(
                "plaintext operation Vault authorization belongs to a different note".to_string(),
            ));
        }
        let head = self
            .read_vault_graph_head(&connection, access.key(), note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let index: AttachmentIndex = self.read_vault_object(
            &connection,
            access.key(),
            note_id,
            &head.manifest.attachment_index,
        )?;
        let attachment = index
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .cloned()
            .ok_or_else(|| {
                StorageError::Integrity("protected attachment metadata is missing".to_string())
            })?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            return Err(StorageError::Validation(
                "encrypted note was locked while reading attachment metadata".to_string(),
            ));
        }
        Ok(Some(attachment))
    }

    pub(super) fn protected_attachment_with_access(
        &self,
        attachment_id: &str,
        access: &VaultNoteAccess,
    ) -> StorageResult<Option<AuthorizedProtectedAttachmentResource>> {
        let connection = self.connection()?;
        let Some(note_id) = self.protected_note_id_for_attachment(&connection, attachment_id)?
        else {
            return Ok(None);
        };
        if access.permit().note_id() != &note_id {
            return Err(StorageError::Integrity(
                "plaintext operation Vault authorization belongs to a different note".to_string(),
            ));
        }
        let head = self
            .read_vault_graph_head(&connection, access.key(), &note_id)?
            .ok_or_else(|| {
                StorageError::Integrity("protected note graph is missing".to_string())
            })?;
        let index: AttachmentIndex = self.read_vault_object(
            &connection,
            access.key(),
            &note_id,
            &head.manifest.attachment_index,
        )?;
        let attachment = index
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .cloned()
            .ok_or_else(|| {
                StorageError::Integrity("protected attachment metadata is missing".to_string())
            })?;
        let blob = index
            .blobs
            .iter()
            .find(|blob| blob.plaintext_sha256 == attachment.sha256)
            .cloned()
            .ok_or_else(|| {
                StorageError::Integrity("protected attachment blob mapping is missing".to_string())
            })?;
        let path = self.materialize_protected_attachment(&note_id, &attachment, &blob, access)?;
        Ok(Some(AuthorizedProtectedAttachmentResource {
            note_id,
            attachment,
            path,
        }))
    }

    pub(super) fn protected_attachment_resource(
        &self,
        attachment_id: &str,
    ) -> StorageResult<Option<ProtectedAttachmentResource>> {
        let connection = self.connection()?;
        let Some(note_id) = self.protected_note_id_for_attachment(&connection, attachment_id)?
        else {
            return Ok(None);
        };
        drop(connection);
        let access = self
            .privacy_coordinator
            .acquire_note_access(&note_id)
            .map_err(vault_runtime_error)?
            .ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note before opening its attachments".to_string(),
                )
            })?;
        let resource = self
            .protected_attachment_with_access(attachment_id, &access)?
            .ok_or_else(|| {
                StorageError::Integrity(
                    "protected note state changed during authorized attachment read".to_string(),
                )
            })?;
        Ok(Some(ProtectedAttachmentResource {
            path: resource.path,
            permit: access.into_permit(),
        }))
    }

    fn with_unlocked_metadata_state<R>(
        &self,
        note_id: &NoteId,
        use_metadata: impl FnOnce(MetadataState) -> StorageResult<R>,
    ) -> StorageResult<Option<R>> {
        let Some(access) = self
            .privacy_coordinator
            .acquire_note_access(note_id)
            .map_err(vault_runtime_error)?
        else {
            return Ok(None);
        };
        let connection = self.connection()?;
        let Some(head) = self.read_vault_graph_head(&connection, access.key(), note_id)? else {
            return Ok(None);
        };
        let metadata: MetadataState = self.read_vault_object(
            &connection,
            access.key(),
            note_id,
            &head.manifest.metadata_state,
        )?;
        let result = use_metadata(metadata)?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            drop(result);
            return Ok(None);
        }
        Ok(Some(result))
    }

    fn with_unlocked_attachment_index<R>(
        &self,
        note_id: &NoteId,
        use_index: impl FnOnce(AttachmentIndex) -> StorageResult<R>,
    ) -> StorageResult<Option<R>> {
        let Some(access) = self
            .privacy_coordinator
            .acquire_note_access(note_id)
            .map_err(vault_runtime_error)?
        else {
            return Ok(None);
        };
        let connection = self.connection()?;
        let Some(head) = self.read_vault_graph_head(&connection, access.key(), note_id)? else {
            return Ok(None);
        };
        let index: AttachmentIndex = self.read_vault_object(
            &connection,
            access.key(),
            note_id,
            &head.manifest.attachment_index,
        )?;
        let result = use_index(index)?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            drop(result);
            return Ok(None);
        }
        Ok(Some(result))
    }

    fn with_unlocked_graph<R>(
        &self,
        note_id: &NoteId,
        use_graph: impl FnOnce(VaultCurrentGraph) -> StorageResult<R>,
    ) -> StorageResult<Option<R>> {
        let Some(access) = self
            .privacy_coordinator
            .acquire_note_access(note_id)
            .map_err(vault_runtime_error)?
        else {
            return Ok(None);
        };
        let Some(graph) = self.unlocked_graph_with_access(note_id, &access)? else {
            return Ok(None);
        };
        let result = use_graph(graph)?;
        if !self
            .privacy_coordinator
            .read_permit_is_publishable(access.permit())
            .map_err(vault_runtime_error)?
        {
            drop(result);
            return Ok(None);
        }
        Ok(Some(result))
    }

    fn unlocked_graph_with_access(
        &self,
        note_id: &NoteId,
        access: &VaultNoteAccess,
    ) -> StorageResult<Option<VaultCurrentGraph>> {
        let connection = self.connection()?;
        self.read_vault_current_graph(&connection, access.key(), note_id)
    }

    fn prepare_next_vault_revision(
        &self,
        connection: &Connection,
        note_key: &VaultKey,
        note_id: &NoteId,
        manifest: &VaultManifest,
        snapshot: RevisionSnapshot,
        created_at: i64,
    ) -> StorageResult<Option<RevisionObject>> {
        let (next_revision_no, previous_revision_object, current_checksum) =
            if let Some(head) = manifest.revision_head.as_ref() {
                let current: RevisionObject =
                    self.read_vault_object(connection, note_key, note_id, head)?;
                (
                    current.revision_no.saturating_add(1),
                    Some(head.clone()),
                    Some(current.snapshot_checksum),
                )
            } else {
                (1, None, None)
            };
        let revision = RevisionObject::new(
            note_id.clone(),
            new_id("rev"),
            next_revision_no,
            previous_revision_object,
            created_at,
            snapshot,
        )
        .map_err(|message| StorageError::Integrity(message.to_string()))?;
        if current_checksum.as_deref() == Some(revision.snapshot_checksum.as_str()) {
            return Ok(None);
        }
        Ok(Some(revision))
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_vault_graph_update(
        &self,
        transaction: &Transaction<'_>,
        access: &VaultNoteAccess,
        root: &VaultNoteRootRecord,
        old_manifest: &VaultManifest,
        private_state: Option<&NotePrivateState>,
        metadata_state: Option<&MetadataState>,
        attachment_index: Option<&AttachmentIndex>,
        appended_revision: Option<&RevisionObject>,
    ) -> StorageResult<VaultManifest> {
        let note_id = &root.note_id;
        let mut private_id = old_manifest.note_private_state.clone();
        let mut metadata_id = old_manifest.metadata_state.clone();
        let mut attachment_index_id = old_manifest.attachment_index.clone();
        if let Some(state) = private_state {
            state
                .validate_for(note_id)
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
            private_id = self.insert_vault_object(transaction, access.key(), note_id, state)?;
        }
        if let Some(state) = metadata_state {
            state
                .validate_for(note_id)
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
            metadata_id = self.insert_vault_object(transaction, access.key(), note_id, state)?;
        }
        if let Some(index) = attachment_index {
            index
                .validate_for(note_id)
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
            attachment_index_id =
                self.insert_vault_object(transaction, access.key(), note_id, index)?;
        }

        let mut revision_index_id = old_manifest.revision_index.clone();
        let mut revision_head = old_manifest.revision_head.clone();
        let mut revision_floor = old_manifest.revision_floor.clone();
        let mut revision_count = old_manifest.revision_count;
        if let Some(revision) = appended_revision {
            if revision.previous_revision_object != old_manifest.revision_head {
                return Err(StorageError::Integrity(
                    "new protected revision does not extend the current revision head".to_string(),
                ));
            }
            let revision_id =
                self.insert_vault_object(transaction, access.key(), note_id, revision)?;
            let mut revision_index =
                self.read_vault_revision_index(transaction, access.key(), note_id, old_manifest)?;
            revision_index.entries.insert(
                0,
                crate::security::sensitive_data::RevisionIndexEntry {
                    object_id: revision_id.clone(),
                    revision_no: revision.revision_no,
                    snapshot_byte_size: revision.snapshot_byte_size,
                    created_at: revision.created_at,
                },
            );
            revision_index
                .validate_for(note_id)
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
            revision_index_id =
                self.insert_vault_object(transaction, access.key(), note_id, &revision_index)?;
            revision_head = Some(revision_id.clone());
            if revision_floor.is_none() {
                revision_floor = Some(revision_id);
            }
            revision_count = revision_count.saturating_add(1);
        }

        let manifest = VaultManifest::new(
            note_id.clone(),
            old_manifest.schema_generation.saturating_add(1),
            private_id,
            metadata_id,
            attachment_index_id,
            revision_index_id,
            revision_head,
            revision_floor,
            revision_count,
        )?;
        self.commit_vault_manifest_root(transaction, access.key(), root, &manifest)?;
        if let Some(index) = attachment_index {
            self.replace_vault_blob_refs(transaction, note_id, index)?;
        }
        Ok(manifest)
    }

    fn protect_note_blobs(
        &self,
        vault: &VaultService,
        note_key: &VaultKey,
        note_id: &NoteId,
        attachments: &[Attachment],
        revisions: &[(NoteRevision, RevisionSnapshot)],
    ) -> StorageResult<Vec<VaultBlobReference>> {
        let mut blob_sizes = BTreeMap::<String, i64>::new();
        for attachment in attachments {
            blob_sizes
                .entry(attachment.sha256.clone())
                .and_modify(|size| {
                    if *size != attachment.byte_size {
                        *size = -1;
                    }
                })
                .or_insert(attachment.byte_size);
        }
        for (_, snapshot) in revisions {
            for attachment in &snapshot.attachments {
                blob_sizes
                    .entry(attachment.sha256.clone())
                    .and_modify(|size| {
                        if *size != attachment.byte_size {
                            *size = -1;
                        }
                    })
                    .or_insert(attachment.byte_size);
            }
        }

        let staging = self.attachments_directory.join(".vault-incoming");
        crate::native::private_temp::ensure_private_directory(&staging)?;
        let mut protected = Vec::with_capacity(blob_sizes.len());
        for (plaintext_sha256, plaintext_byte_size) in blob_sizes {
            let result = self.protect_single_note_blob(
                vault,
                note_key,
                note_id,
                &staging,
                plaintext_sha256,
                plaintext_byte_size,
            );
            match result {
                Ok(record) => protected.push(record),
                Err(error) => {
                    let cleanup = self.delete_ciphertext_blobs_if_unreferenced(
                        protected.iter().map(|blob| blob.ciphertext_sha256.as_str()),
                    );
                    let _ = fs::remove_dir(&staging);
                    if let Err(cleanup_error) = cleanup {
                        return Err(StorageError::Integrity(format!(
                            "protected attachment encryption failed ({error}) and ciphertext cleanup failed ({cleanup_error})"
                        )));
                    }
                    return Err(error);
                }
            }
        }
        let _ = fs::remove_dir(&staging);
        Ok(protected)
    }

    fn protect_single_note_blob(
        &self,
        vault: &VaultService,
        note_key: &VaultKey,
        note_id: &NoteId,
        staging: &std::path::Path,
        plaintext_sha256: String,
        plaintext_byte_size: i64,
    ) -> StorageResult<VaultBlobReference> {
        let relative = canonical_blob_relative_path(&plaintext_sha256).ok_or_else(|| {
            StorageError::Integrity("attachment has an invalid plaintext blob identity".to_string())
        })?;
        let source_path = self.attachments_directory.join(relative);
        self.protect_note_blob_from_path(
            vault,
            note_key,
            ProtectedBlobInput {
                note_id,
                staging,
                source_path: &source_path,
                plaintext_sha256,
                plaintext_byte_size,
            },
        )
    }

    fn protect_note_blob_from_path(
        &self,
        vault: &VaultService,
        note_key: &VaultKey,
        input: ProtectedBlobInput<'_>,
    ) -> StorageResult<VaultBlobReference> {
        let ProtectedBlobInput {
            note_id,
            staging,
            source_path,
            plaintext_sha256,
            plaintext_byte_size,
        } = input;
        if plaintext_byte_size < 0 {
            return Err(StorageError::Integrity(format!(
                "attachment blob {plaintext_sha256} has inconsistent sizes"
            )));
        }
        let (actual_hash, actual_size) = sha256_file(source_path)?;
        if actual_hash != plaintext_sha256
            || actual_size != u64::try_from(plaintext_byte_size).unwrap_or(u64::MAX)
        {
            return Err(StorageError::Integrity(format!(
                "attachment blob {plaintext_sha256} failed verification before encryption"
            )));
        }

        let staging_path = staging.join(format!("{}.tmp", new_id("vault")));
        let result = (|| -> StorageResult<VaultBlobReference> {
            let reader = BufReader::new(File::open(source_path)?);
            let writer = BufWriter::new(crate::native::private_temp::create_private_file(
                &staging_path,
            )?);
            let context_id = protected_blob_context_id(note_id, &plaintext_sha256);
            vault.seal_protected_blob(
                note_key,
                VaultContext::new(VaultDomain::Attachment, context_id.as_bytes()),
                reader,
                writer,
                actual_size,
                DEFAULT_PROTECTED_BLOB_CHUNK_BYTES,
            )?;
            File::options()
                .write(true)
                .open(&staging_path)?
                .sync_all()?;
            super::attachments::sync_parent_directory(&staging_path)?;
            let (ciphertext_sha256, ciphertext_size) = sha256_file(&staging_path)?;
            let ciphertext_byte_size = i64::try_from(ciphertext_size).map_err(|_| {
                StorageError::Validation("protected attachment is too large to index".to_string())
            })?;
            let final_path = self.blob_path_for_hash(&ciphertext_sha256)?;
            if final_path.exists() {
                let (hash, size) = sha256_file(&final_path)?;
                if hash != ciphertext_sha256 || size != ciphertext_size {
                    return Err(StorageError::Integrity(
                        "existing protected attachment blob failed verification".to_string(),
                    ));
                }
                fs::remove_file(&staging_path)?;
                super::attachments::sync_parent_directory(&staging_path)?;
            } else {
                if let Some(parent) = final_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&staging_path, &final_path)?;
                super::attachments::sync_parent_directory(&staging_path)?;
                super::attachments::sync_parent_directory(&final_path)?;
            }
            Ok(VaultBlobReference {
                plaintext_sha256,
                plaintext_byte_size,
                ciphertext_sha256,
                ciphertext_byte_size,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staging_path);
        }
        result
    }

    fn cleanup_uncommitted_protected_blobs(
        &self,
        blobs: &[VaultBlobReference],
        error: StorageError,
    ) -> StorageError {
        match self.delete_ciphertext_blobs_if_unreferenced(
            blobs.iter().map(|blob| blob.ciphertext_sha256.as_str()),
        ) {
            Ok(()) => error,
            Err(cleanup_error) => StorageError::Integrity(format!(
                "note encryption failed ({error}) and ciphertext cleanup failed ({cleanup_error})"
            )),
        }
    }

    /// Retire every plaintext file covered by one durable `vault_operations`
    /// journal row. Files are unlinked through `OwnedPlaintext`; only after all
    /// file cleanup succeeds are the now-empty operation directories removed.
    /// The journal row itself remains authoritative until the caller explicitly
    /// completes the Vault operation.
    fn discard_vault_operation_plaintext_directory(
        &self,
        note_id: &NoteId,
        operation_id: &str,
        directory: &Path,
    ) -> StorageResult<()> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(StorageError::Integrity(format!(
                    "Vault operation plaintext directory {} contains an unexpected non-file entry",
                    directory.display()
                )));
            }
            let path = entry.path();
            let mut owned_plaintext = OwnedPlaintext::durable_vault_operation_existing(
                path.clone(),
                operation_id.to_string(),
                note_id.clone(),
            );
            owned_plaintext.remove_file()?;
            super::attachments::sync_parent_directory(&path)?;
        }
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if let Some(parent) = directory.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(())
    }

    fn stage_plaintext_blobs(
        &self,
        vault: &VaultService,
        note_key: &VaultKey,
        note_id: &NoteId,
        index: &AttachmentIndex,
        operation_id: &str,
    ) -> StorageResult<()> {
        let staging = self.incoming_operation_directory(operation_id);
        crate::native::private_temp::ensure_private_directory(&staging)?;
        for blob in &index.blobs {
            let final_path = self.blob_path_for_hash(&blob.plaintext_sha256)?;
            if crate::security::managed_path_is_present(&final_path)? {
                let (hash, size) = sha256_file(&final_path)?;
                if hash == blob.plaintext_sha256
                    && size == u64::try_from(blob.plaintext_byte_size).unwrap_or(u64::MAX)
                {
                    continue;
                }
                return Err(StorageError::Integrity(format!(
                    "existing plaintext blob {} failed verification",
                    blob.plaintext_sha256
                )));
            }

            let cipher_path = self.blob_path_for_hash(&blob.ciphertext_sha256)?;
            let staging_path = staging.join(new_id("plain"));
            let mut owned_plaintext = OwnedPlaintext::durable_vault_operation_reserved(
                staging_path.clone(),
                operation_id.to_string(),
                note_id.clone(),
            );
            let result = (|| -> StorageResult<()> {
                let reader = BufReader::new(File::open(&cipher_path)?);
                let writer = BufWriter::new(owned_plaintext.create_file()?);
                let context_id = protected_blob_context_id(note_id, &blob.plaintext_sha256);
                vault.open_protected_blob(
                    note_key,
                    VaultContext::new(VaultDomain::Attachment, context_id.as_bytes()),
                    reader,
                    writer,
                )?;
                let (hash, size) = sha256_file(owned_plaintext.path())?;
                if hash != blob.plaintext_sha256
                    || size != u64::try_from(blob.plaintext_byte_size).unwrap_or(u64::MAX)
                {
                    return Err(StorageError::Integrity(format!(
                        "restored attachment blob {} failed verification",
                        blob.plaintext_sha256
                    )));
                }
                File::options()
                    .write(true)
                    .open(owned_plaintext.path())?
                    .sync_all()?;
                super::attachments::sync_parent_directory(owned_plaintext.path())?;
                Ok(())
            })();
            result?;
        }
        Ok(())
    }

    fn finalize_staged_plaintext_operation(
        &self,
        note_id: &NoteId,
        operation_id: &str,
    ) -> StorageResult<()> {
        let staging = self.incoming_operation_directory(operation_id);
        let entries = match fs::read_dir(&staging) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(StorageError::Integrity(
                    "vault plaintext staging contains an unexpected non-file entry".to_string(),
                ));
            }
            let staged_path = entry.path();
            let mut owned_plaintext = OwnedPlaintext::durable_vault_operation_existing(
                staged_path,
                operation_id.to_string(),
                note_id.clone(),
            );
            let (hash, _) = sha256_file(owned_plaintext.path())?;
            let final_path = self.blob_path_for_hash(&hash)?;
            if crate::security::managed_path_is_present(&final_path)? {
                let (existing_hash, _) = sha256_file(&final_path)?;
                if existing_hash != hash {
                    return Err(StorageError::Integrity(format!(
                        "restored plaintext attachment conflicts with {}",
                        final_path.display()
                    )));
                }
                owned_plaintext.remove_file()?;
                continue;
            }
            if let Some(parent) = final_path.parent() {
                fs::create_dir_all(parent)?;
            }
            owned_plaintext.rename_durable_vault_operation(final_path.clone())?;
            super::attachments::sync_parent_directory(&staging)?;
            super::attachments::sync_parent_directory(&final_path)?;
        }
        let _ = fs::remove_dir(&staging);
        if let Some(parent) = staging.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(())
    }

    fn restore_retiring_operation(
        &self,
        note_id: &NoteId,
        operation_id: &str,
    ) -> StorageResult<()> {
        let retiring = self.retiring_operation_directory(operation_id);
        let entries = match fs::read_dir(&retiring) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(StorageError::Integrity(
                    "vault retiring staging contains an unexpected non-file entry".to_string(),
                ));
            }
            let retired_path = entry.path();
            let mut owned_plaintext = OwnedPlaintext::durable_vault_operation_existing(
                retired_path,
                operation_id.to_string(),
                note_id.clone(),
            );
            let (hash, _) = sha256_file(owned_plaintext.path())?;
            let original_path = self.blob_path_for_hash(&hash)?;
            if crate::security::managed_path_is_present(&original_path)? {
                let (existing_hash, _) = sha256_file(&original_path)?;
                if existing_hash != hash {
                    return Err(StorageError::Integrity(format!(
                        "vault rollback conflicts with {}",
                        original_path.display()
                    )));
                }
                owned_plaintext.remove_file()?;
                continue;
            }
            if let Some(parent) = original_path.parent() {
                fs::create_dir_all(parent)?;
            }
            owned_plaintext.rename_durable_vault_operation(original_path.clone())?;
            super::attachments::sync_parent_directory(&retiring)?;
            super::attachments::sync_parent_directory(&original_path)?;
        }
        let _ = fs::remove_dir(&retiring);
        if let Some(parent) = retiring.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(())
    }

    fn stage_exclusive_plaintext_blobs(
        &self,
        note_id: &NoteId,
        index: &AttachmentIndex,
        operation_id: &str,
    ) -> StorageResult<Vec<RetiredPlaintextBlob>> {
        let retiring = self.retiring_operation_directory(operation_id);
        let connection = self.connection()?;
        let mut exclusive_blobs = Vec::new();
        for blob in &index.blobs {
            let external_references: i64 = connection.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM attachments WHERE sha256 = ?1 AND note_id <> ?2) +
                    (SELECT COUNT(*)
                     FROM revision_attachment_refs refs
                     JOIN note_revisions revisions ON revisions.id = refs.revision_id
                     WHERE refs.sha256 = ?1 AND revisions.note_id <> ?2)",
                params![&blob.plaintext_sha256, note_id],
                |row| row.get(0),
            )?;
            if external_references == 0 {
                exclusive_blobs.push((
                    blob.plaintext_sha256.clone(),
                    self.blob_path_for_hash(&blob.plaintext_sha256)?,
                ));
            }
        }
        drop(connection);

        let mut retired = Vec::with_capacity(exclusive_blobs.len());
        for (sha256, original_path) in exclusive_blobs {
            if !crate::security::managed_path_is_present(&original_path)? {
                restore_retired_plaintext_blobs(&mut retired)?;
                return Err(StorageError::Integrity(format!(
                    "plaintext attachment blob {sha256} disappeared during encryption"
                )));
            }
            crate::native::private_temp::ensure_private_directory(&retiring)?;
            let retired_path = retiring.join(new_id("retired"));
            let mut owned_plaintext = OwnedPlaintext::durable_vault_operation_existing(
                original_path.clone(),
                operation_id.to_string(),
                note_id.clone(),
            );
            if let Err(error) = owned_plaintext.rename_durable_vault_operation(retired_path.clone())
            {
                restore_retired_plaintext_blobs(&mut retired)?;
                return Err(error.into());
            }
            super::attachments::sync_parent_directory(&original_path)?;
            super::attachments::sync_parent_directory(&retired_path)?;
            retired.push(RetiredPlaintextBlob {
                original_path,
                plaintext: owned_plaintext,
            });
        }
        Ok(retired)
    }

    fn materialize_protected_attachment(
        &self,
        note_id: &NoteId,
        attachment: &Attachment,
        blob: &VaultBlobReference,
        access: &VaultNoteAccess,
    ) -> StorageResult<PathBuf> {
        if let Some(path) = self
            .privacy_coordinator
            .materialized_attachment(access.permit(), note_id, &attachment.id)
            .map_err(vault_runtime_error)?
        {
            return Ok(path);
        }
        let cipher_path = self.blob_path_for_hash(&blob.ciphertext_sha256)?;
        let temp_path = self.privacy_coordinator.allocate_vault_plaintext_path()?;

        // Establish typed Vault-session ownership before the file can exist or
        // receive a single plaintext byte. `OwnedPlaintext` retains the cleanup
        // obligation across crypto, verification and Lock races.
        let mut owned_plaintext = crate::security::OwnedPlaintext::reserve_vault_session(
            access.permit(),
            temp_path.clone(),
        )
        .map_err(vault_runtime_error)?;

        let result = (|| -> StorageResult<()> {
            let vault = VaultService::new();
            let reader = BufReader::new(File::open(&cipher_path)?);
            let writer = BufWriter::new(owned_plaintext.create_file()?);
            let context_id = protected_blob_context_id(note_id, &blob.plaintext_sha256);
            vault.open_protected_blob(
                access.key(),
                VaultContext::new(VaultDomain::Attachment, context_id.as_bytes()),
                reader,
                writer,
            )?;
            let (hash, size) = sha256_file(owned_plaintext.path())?;
            if hash != blob.plaintext_sha256
                || size != u64::try_from(blob.plaintext_byte_size).unwrap_or(u64::MAX)
            {
                return Err(StorageError::Integrity(
                    "materialized protected attachment failed verification".to_string(),
                ));
            }
            Ok(())
        })();
        if let Err(error) = result {
            return match owned_plaintext.remove_file() {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(StorageError::Integrity(format!(
                    "protected attachment materialization failed ({error}) and plaintext cleanup is incomplete ({cleanup_error})"
                ))),
            };
        }

        match owned_plaintext.promote_vault_materialized_attachment(note_id, attachment.id.clone())
        {
            Ok(path) => Ok(path),
            Err(message) => match owned_plaintext.remove_file() {
                Ok(()) => Err(vault_runtime_error(message)),
                Err(cleanup_error) => Err(StorageError::Integrity(format!(
                    "protected attachment publication was rejected ({message}) and plaintext cleanup is incomplete ({cleanup_error})"
                ))),
            },
        }
    }

    /// Apply revision retention to every currently unlocked protected note.
    ///
    /// This is deliberately an explicit/deferred maintenance path. Protected
    /// autosave appends at most one immutable revision after reading only the
    /// current head; it never walks history merely to enforce retention.
    pub(super) fn prune_unlocked_protected_revision_history(&self) -> StorageResult<usize> {
        let note_ids = {
            let connection = self.connection()?;
            let mut statement =
                connection.prepare("SELECT note_id FROM vault_note_roots ORDER BY note_id")?;
            let rows = statement.query_map([], |row| row.get::<_, NoteId>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let retention = self.revision_retention()?;
        let mut removed_revisions = 0_usize;
        for note_id in note_ids {
            let Some(mutation) = self.begin_protected_note_mutation(&note_id)? else {
                // A locked note has no plaintext key available by design. Its
                // retention policy is applied the next time explicit maintenance
                // runs while it is unlocked; startup never decrypts histories.
                continue;
            };
            removed_revisions = removed_revisions.saturating_add(
                self.prune_protected_revision_history_with_access(
                    &note_id,
                    mutation.access(),
                    &retention,
                )?,
            );
        }
        Ok(removed_revisions)
    }

    fn prune_protected_revision_history_with_access(
        &self,
        note_id: &NoteId,
        access: &VaultNoteAccess,
        retention: &RevisionRetention,
    ) -> StorageResult<usize> {
        // Caller holds the coordinator-owned protected mutation guard, which
        // supplies both Vault authorization and storage serialization.
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(head) = self.read_vault_graph_head(&transaction, access.key(), note_id)? else {
            return Ok(0);
        };
        let current_index: AttachmentIndex = self.read_vault_object(
            &transaction,
            access.key(),
            note_id,
            &head.manifest.attachment_index,
        )?;
        let chain =
            self.read_vault_revision_chain(&transaction, access.key(), note_id, &head.manifest)?;

        let retained_count = retained_protected_revision_count(&chain, retention);
        let removed = chain.len().saturating_sub(retained_count);
        let retained = &chain[..retained_count];

        // The encrypted attachment catalog intentionally contains both current
        // and revision-only blob mappings. Once an old revision tail is pruned,
        // remove mappings that are no longer needed by either current state or
        // any retained revision. Ciphertext deletion happens only after commit.
        let mut required_plaintext_hashes = current_index
            .attachments
            .iter()
            .map(|attachment| attachment.sha256.clone())
            .collect::<HashSet<_>>();
        for (_, revision) in retained {
            required_plaintext_hashes.extend(
                revision
                    .snapshot
                    .attachments
                    .iter()
                    .map(|attachment| attachment.sha256.clone()),
            );
        }
        let mut next_index = current_index.clone();
        let removed_ciphertext_hashes = next_index
            .blobs
            .iter()
            .filter(|blob| !required_plaintext_hashes.contains(&blob.plaintext_sha256))
            .map(|blob| blob.ciphertext_sha256.clone())
            .collect::<Vec<_>>();
        next_index
            .blobs
            .retain(|blob| required_plaintext_hashes.contains(&blob.plaintext_sha256));
        next_index
            .validate_for(note_id)
            .map_err(|message| StorageError::Integrity(message.to_string()))?;
        let index_changed = next_index != current_index;

        let (revision_head, revision_floor, revision_count) = if retained.is_empty() {
            (None, None, 0_u64)
        } else {
            (
                Some(retained[0].0.clone()),
                Some(retained[retained.len() - 1].0.clone()),
                u64::try_from(retained.len()).map_err(|_| {
                    StorageError::Integrity(
                        "protected revision count is outside the supported range".to_string(),
                    )
                })?,
            )
        };
        let revision_bounds_changed = revision_head != head.manifest.revision_head
            || revision_floor != head.manifest.revision_floor
            || revision_count != head.manifest.revision_count;

        if revision_bounds_changed || index_changed {
            let mut attachment_index_id = head.manifest.attachment_index.clone();
            if index_changed {
                attachment_index_id =
                    self.insert_vault_object(&transaction, access.key(), note_id, &next_index)?;
            }
            let mut revision_index_id = head.manifest.revision_index.clone();
            if revision_bounds_changed {
                let revision_index = RevisionIndex::from_newest_first(
                    note_id.clone(),
                    retained.iter().map(|(object_id, revision)| {
                        (
                            object_id.clone(),
                            revision.revision_no,
                            revision.snapshot_byte_size,
                            revision.created_at,
                        )
                    }),
                )
                .map_err(|message| StorageError::Integrity(message.to_string()))?;
                revision_index_id =
                    self.insert_vault_object(&transaction, access.key(), note_id, &revision_index)?;
            }
            let manifest = VaultManifest::new(
                note_id.clone(),
                head.manifest.schema_generation.saturating_add(1),
                head.manifest.note_private_state.clone(),
                head.manifest.metadata_state.clone(),
                attachment_index_id,
                revision_index_id,
                revision_head,
                revision_floor,
                revision_count,
            )?;
            self.commit_vault_manifest_root(&transaction, access.key(), &head.root, &manifest)?;
            if index_changed {
                self.replace_vault_blob_refs(&transaction, note_id, &next_index)?;
            }
        }

        // Crash/rollback cannot leave half-pruned history: reachability is
        // computed from the root visible in this same SQLite transaction.
        self.delete_unreachable_vault_objects(&transaction, access.key(), note_id)?;
        transaction.commit()?;

        if !removed_ciphertext_hashes.is_empty() {
            self.delete_ciphertext_blobs_if_unreferenced(
                removed_ciphertext_hashes.iter().map(String::as_str),
            )?;
        }
        Ok(removed)
    }

    fn blob_path_for_hash(&self, sha256: &str) -> StorageResult<PathBuf> {
        let relative = canonical_blob_relative_path(sha256).ok_or_else(|| {
            StorageError::Integrity("attachment has an invalid blob identity".to_string())
        })?;
        Ok(self.attachments_directory.join(relative))
    }

    fn delete_ciphertext_blobs_if_unreferenced<'a>(
        &self,
        hashes: impl IntoIterator<Item = &'a str>,
    ) -> StorageResult<()> {
        let connection = self.connection()?;
        let mut unique = HashSet::new();
        for hash in hashes {
            if !unique.insert(hash.to_string()) {
                continue;
            }
            let references: i64 = connection.query_row(
                "SELECT\n                    (SELECT COUNT(*) FROM attachments WHERE sha256 = ?1) +\n                    (SELECT COUNT(*) FROM revision_attachment_refs WHERE sha256 = ?1) +\n                    (SELECT COUNT(*) FROM vault_blob_refs WHERE sha256 = ?1)",
                [hash],
                |row| row.get(0),
            )?;
            if references == 0 {
                let path = self.blob_path_for_hash(hash)?;
                match crate::native::privacy::remove_file(&path) {
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(())
    }
}

fn retained_protected_revision_count(
    chain: &[(
        crate::security::vault_object_spec::VaultObjectId,
        RevisionObject,
    )],
    retention: &RevisionRetention,
) -> usize {
    if chain.is_empty() {
        return 0;
    }
    let max_count = usize::try_from(retention.max_revisions_per_note)
        .unwrap_or(usize::MAX)
        .max(1);
    let cutoff = (retention.max_age_days > 0).then(|| {
        unix_timestamp().saturating_sub(retention.max_age_days.saturating_mul(24 * 60 * 60))
    });
    let mut retained = 1_usize; // Never prune the newest retained revision.
    while retained < chain.len() && retained < max_count {
        if cutoff.is_some_and(|cutoff| chain[retained].1.created_at < cutoff) {
            break;
        }
        retained += 1;
    }
    retained
}

fn restore_retired_plaintext_blobs(retired: &mut [RetiredPlaintextBlob]) -> StorageResult<()> {
    for blob in retired.iter_mut().rev() {
        if !blob.plaintext.path_entry_exists()? {
            continue;
        }
        if let Some(parent) = blob.original_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let predecessor = blob.plaintext.path().to_path_buf();
        blob.plaintext
            .rename_durable_vault_operation(blob.original_path.clone())?;
        super::attachments::sync_parent_directory(&predecessor)?;
        super::attachments::sync_parent_directory(&blob.original_path)?;
    }
    Ok(())
}

fn discard_retired_plaintext_blobs(retired: &mut [RetiredPlaintextBlob]) -> StorageResult<()> {
    for blob in retired.iter_mut() {
        // E3 uses the common privacy remover through OwnedPlaintext: bounded
        // overwrite for practical small/medium plaintext, then unlink.
        let retired_path = blob.plaintext.path().to_path_buf();
        blob.plaintext.remove_file()?;
        super::attachments::sync_parent_directory(&retired_path)?;
    }
    if let Some(directory) = retired
        .first()
        .and_then(|blob| blob.plaintext.path().parent())
    {
        let _ = fs::remove_dir(directory);
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct AuthorizedProtectedAttachmentResource {
    pub note_id: NoteId,
    pub attachment: Attachment,
    pub path: PathBuf,
}

#[derive(Debug)]
pub(super) struct ProtectedAttachmentResource {
    pub path: PathBuf,
    pub permit: VaultReadPermit,
}

fn validate_password(password: &[u8]) -> StorageResult<()> {
    if password.is_empty() {
        return Err(StorageError::Validation(
            "encryption password cannot be empty".to_string(),
        ));
    }
    Ok(())
}

fn vault_runtime_error(message: &'static str) -> StorageError {
    StorageError::Integrity(message.to_string())
}

pub(super) fn protected_note_exists(
    connection: &Connection,
    note_id: &NoteId,
) -> StorageResult<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM vault_note_roots WHERE note_id = ?1)",
        [note_id],
        |row| row.get(0),
    )?)
}

fn ensure_note_protection_ready(connection: &Connection, note_id: &NoteId) -> StorageResult<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1)",
        [note_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::Validation(
            "note no longer exists".to_string(),
        ));
    }
    let pending: i64 = connection.query_row(
        "SELECT
            (SELECT COUNT(*) FROM attachment_ingest_jobs WHERE note_id = ?1) +
            (SELECT COUNT(*) FROM attachment_operations WHERE note_id = ?1) +
            (SELECT COUNT(*) FROM managed_plaintext_staging WHERE note_id = ?1)",
        [note_id],
        |row| row.get(0),
    )?;
    if pending != 0 {
        return Err(StorageError::Validation(
            "finish or clean pending plaintext staging/attachment operations before encrypting this note".to_string(),
        ));
    }
    Ok(())
}

fn reconcile_protected_attachment_index(
    index: &mut AttachmentIndex,
    document_attachment_ids: &[String],
) -> StorageResult<Vec<String>> {
    let document_ids = document_attachment_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let known_ids = index
        .attachments
        .iter()
        .map(|attachment| attachment.id.clone())
        .collect::<HashSet<_>>();
    if let Some(missing_id) = document_ids
        .iter()
        .find(|attachment_id| !known_ids.contains(*attachment_id))
    {
        return Err(StorageError::Validation(format!(
            "document references missing or foreign attachment {missing_id}"
        )));
    }

    let source_by_id = index
        .attachments
        .iter()
        .map(|attachment| {
            (
                attachment.id.clone(),
                attachment.source_attachment_id.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
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

    let now = unix_timestamp();
    for attachment in &mut index.attachments {
        let embedded = desired_ids.contains(&attachment.id);
        if attachment.embedded == embedded {
            continue;
        }
        attachment.embedded = embedded;
        attachment.unreferenced_at = (!embedded).then_some(now);
        attachment.updated_at = now;
    }
    Ok(index
        .attachments
        .iter()
        .filter(|attachment| document_ids.contains(&attachment.id))
        .map(|attachment| attachment.file_name.clone())
        .collect())
}

fn materialize_vault_note(mut base: Note, graph: &VaultCurrentGraph) -> StorageResult<Note> {
    let note_id = &base.id;
    graph
        .note_private_state
        .validate_for(note_id)
        .map_err(|message| StorageError::Integrity(message.to_string()))?;
    graph
        .metadata_state
        .validate_for(note_id)
        .map_err(|message| StorageError::Integrity(message.to_string()))?;
    graph
        .attachment_index
        .validate_for(note_id)
        .map_err(|message| StorageError::Integrity(message.to_string()))?;
    let projections = derive_editor_projections(&graph.note_private_state.editor_state)
        .map_err(StorageError::from)?;
    base.title = graph.note_private_state.title.clone();
    base.title_style = graph.note_private_state.title_style.clone();
    base.document = Arc::new(NoteDocument::new(
        projections.markdown,
        Arc::new(graph.note_private_state.editor_state.clone()),
        projections.plain_text,
    ));
    base.tags = graph.metadata_state.tags.clone();
    base.attachment_names = graph
        .attachment_index
        .attachments
        .iter()
        .filter(|attachment| attachment.embedded)
        .map(|attachment| attachment.file_name.clone())
        .collect();
    base.protection = NoteProtectionState::Unlocked;
    Ok(base)
}

fn replace_protected_attachment_stubs(
    transaction: &Transaction<'_>,
    note_id: &NoteId,
    index: &AttachmentIndex,
    now: i64,
) -> StorageResult<()> {
    let blob_map = index
        .blobs
        .iter()
        .map(|blob| (blob.plaintext_sha256.as_str(), blob))
        .collect::<HashMap<_, _>>();
    transaction.execute("DELETE FROM attachments WHERE note_id = ?1", [note_id])?;
    for attachment in &index.attachments {
        let blob = blob_map.get(attachment.sha256.as_str()).ok_or_else(|| {
            StorageError::Integrity(format!(
                "protected blob mapping missing for attachment {}",
                attachment.id
            ))
        })?;
        transaction.execute(
            "INSERT INTO attachments(
                id, note_id, file_name, mime_type, byte_size, sha256,
                image_pixel_width, image_pixel_height, image_format, image_orientation,
                image_has_alpha, image_animated, source_attachment_id, image_edit_recipe,
                embedded, unreferenced_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, NULL, ?4, ?5, NULL, NULL, NULL, NULL, NULL, NULL,
                       NULL, NULL, 0, ?6, ?6, ?6)",
            params![
                &attachment.id,
                note_id,
                LOCKED_ATTACHMENT_NAME,
                blob.ciphertext_byte_size,
                &blob.ciphertext_sha256,
                now,
            ],
        )?;
    }
    Ok(())
}

fn protected_blob_context_id(note_id: &NoteId, plaintext_sha256: &str) -> String {
    format!("{}:{plaintext_sha256}", note_id.as_str())
}

fn normalize_locked_note_label(value: &str) -> StorageResult<String> {
    let trimmed = value.trim();
    if trimmed.chars().any(char::is_control) {
        return Err(StorageError::Validation(
            "the locked-note label cannot contain control characters".to_string(),
        ));
    }
    if trimmed.chars().count() > 160 {
        return Err(StorageError::Validation(
            "the locked-note label must be 160 characters or fewer".to_string(),
        ));
    }
    Ok(if trimmed.is_empty() {
        "Locked note".to_string()
    } else {
        trimmed.to_string()
    })
}

fn protected_note_scrub_assignment(column: &str) -> StorageResult<&'static str> {
    match column {
        // `?4` is the user-approved non-secret locked-note label. The private
        // title has already been copied into encrypted NotePrivateState.
        "title" => Ok("title = ?4"),
        "title_style" => Ok("title_style = '{}'"),
        "body" => Ok("body = ''"),
        "editor_state" => Ok("editor_state = ?1"),
        "plain_text" => Ok("plain_text = ''"),
        other => Err(StorageError::Integrity(format!(
            "SensitiveNoteData declares note column {other} private, but the protected scrubber has no canonical value for it"
        ))),
    }
}

fn protected_attachment_scrub_assignment(column: &str) -> StorageResult<&'static str> {
    match column {
        "file_name" => Ok("file_name = ?1"),
        "mime_type" => Ok("mime_type = NULL"),
        "image_pixel_width" => Ok("image_pixel_width = NULL"),
        "image_pixel_height" => Ok("image_pixel_height = NULL"),
        "image_format" => Ok("image_format = NULL"),
        "image_orientation" => Ok("image_orientation = NULL"),
        "image_has_alpha" => Ok("image_has_alpha = NULL"),
        "image_animated" => Ok("image_animated = NULL"),
        "source_attachment_id" => Ok("source_attachment_id = NULL"),
        "image_edit_recipe" => Ok("image_edit_recipe = NULL"),
        "embedded" => Ok("embedded = 0"),
        other => Err(StorageError::Integrity(format!(
            "SensitiveNoteData declares attachment column {other} private, but the protected scrubber has no canonical value for it"
        ))),
    }
}

fn sanitize_protected_note_rows(
    transaction: &Transaction<'_>,
    note_id: &NoteId,
    locked_label: &str,
    index: &AttachmentIndex,
    now: i64,
) -> StorageResult<()> {
    let blob_map = index
        .blobs
        .iter()
        .map(|blob| (blob.plaintext_sha256.as_str(), blob))
        .collect::<HashMap<_, _>>();

    // The canonical SensitiveNoteData authority drives this executable plan.
    // Column assignments are also generated from its classified column lists,
    // so adding a private persistence field cannot silently omit the scrubber.
    for &step in ProtectedPublicPersistenceSpec::SCRUB_STEPS {
        match step {
            ProtectedScrubStep::AttachmentSensitiveColumns => {
                let assignments = ProtectedPublicPersistenceSpec::ATTACHMENT_SANITIZED_COLUMNS
                    .iter()
                    .map(|column| protected_attachment_scrub_assignment(column))
                    .collect::<StorageResult<Vec<_>>>()?
                    .join(", ");
                let sql = format!(
                    "UPDATE attachments SET {assignments}, byte_size = ?2, sha256 = ?3, \
                     unreferenced_at = ?4, created_at = ?4, updated_at = ?4 \
                     WHERE id = ?5 AND note_id = ?6"
                );
                for attachment in &index.attachments {
                    let blob = blob_map.get(attachment.sha256.as_str()).ok_or_else(|| {
                        StorageError::Integrity(format!(
                            "protected blob mapping missing for attachment {}",
                            attachment.id
                        ))
                    })?;
                    let changed = transaction.execute(
                        &sql,
                        params![
                            LOCKED_ATTACHMENT_NAME,
                            blob.ciphertext_byte_size,
                            &blob.ciphertext_sha256,
                            now,
                            &attachment.id,
                            note_id,
                        ],
                    )?;
                    if changed != 1 {
                        return Err(StorageError::Integrity(format!(
                            "attachment {} changed while note encryption was being committed",
                            attachment.id
                        )));
                    }
                }
            }
            ProtectedScrubStep::NoteSensitiveColumns => {
                let assignments = ProtectedPublicPersistenceSpec::NOTE_SANITIZED_COLUMNS
                    .iter()
                    .map(|column| protected_note_scrub_assignment(column))
                    .collect::<StorageResult<Vec<_>>>()?
                    .join(", ");
                let sql = format!("UPDATE notes SET {assignments}, updated_at = ?2 WHERE id = ?3");
                let changed = transaction.execute(
                    &sql,
                    params![EMPTY_EDITOR_STATE.trim(), now, note_id, locked_label],
                )?;
                if changed != 1 {
                    return Err(StorageError::Integrity(
                        "protected note row changed while privacy scrubbing was being committed"
                            .to_string(),
                    ));
                }
            }
            ProtectedScrubStep::FullTextIndex => {
                transaction.execute(
                    "DELETE FROM notes_fts WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = ?1)",
                    [note_id],
                )?;
            }
            ProtectedScrubStep::TagsAndOrphans => {
                transaction.execute("DELETE FROM note_tags WHERE note_id = ?1", [note_id])?;
                transaction.execute(
                    "DELETE FROM tags WHERE NOT EXISTS(SELECT 1 FROM note_tags WHERE note_tags.tag_id = tags.id)",
                    [],
                )?;
            }
            ProtectedScrubStep::Revisions => {
                transaction.execute(
                    "DELETE FROM revision_attachment_refs WHERE revision_id IN (SELECT id FROM note_revisions WHERE note_id = ?1)",
                    [note_id],
                )?;
                transaction.execute("DELETE FROM note_revisions WHERE note_id = ?1", [note_id])?;
            }
            ProtectedScrubStep::OutgoingLinks => {
                transaction.execute(
                    "DELETE FROM note_links WHERE source_note_id = ?1",
                    [note_id],
                )?;
            }
            ProtectedScrubStep::SourcesCitationsAndOrphans => {
                transaction.execute("DELETE FROM citations WHERE note_id = ?1", [note_id])?;
                transaction.execute("DELETE FROM note_sources WHERE note_id = ?1", [note_id])?;
                transaction.execute(
                    "DELETE FROM sources WHERE NOT EXISTS(SELECT 1 FROM note_sources WHERE note_sources.source_id = sources.id)",
                    [],
                )?;
            }
            ProtectedScrubStep::PdfAnnotations => {
                transaction.execute(
                    "DELETE FROM pdf_annotation_pages WHERE attachment_id IN (SELECT id FROM attachments WHERE note_id = ?1)",
                    [note_id],
                )?;
            }
            ProtectedScrubStep::SyncQueue => {
                transaction.execute(
                    "DELETE FROM sync_queue WHERE (entity_type = 'note' AND entity_id = ?1)\
                        OR (entity_type = 'attachment' AND entity_id IN (SELECT id FROM attachments WHERE note_id = ?1))",
                    [note_id],
                )?;
            }
            ProtectedScrubStep::PersistedFilterTags => {
                let _ = super::settings::scrub_persisted_note_list_filter_tags(transaction)?;
            }
        }
    }
    Ok(())
}

fn restore_protected_note_rows(
    storage: &Storage,
    transaction: &Transaction<'_>,
    base: &Note,
    graph: &VaultCurrentGraph,
    revisions_newest_first: &[(
        crate::security::vault_object_spec::VaultObjectId,
        RevisionObject,
    )],
) -> StorageResult<()> {
    let note = materialize_vault_note(base.clone(), graph)?;
    transaction.execute(
        "UPDATE notes SET title = ?1, title_style = ?2, body = ?3, editor_state = ?4,
            plain_text = ?5, updated_at = ?6 WHERE id = ?7",
        params![
            &note.title,
            serde_json::to_string(&note.title_style)?,
            note.body(),
            note.editor_state(),
            note.plain_text(),
            base.updated_at,
            &base.id,
        ],
    )?;
    storage.replace_note_tags(transaction, &base.id, &note.tags)?;

    transaction.execute("DELETE FROM attachments WHERE note_id = ?1", [&base.id])?;
    let mut attachments = graph.attachment_index.attachments.clone();
    attachments.sort_by_key(|attachment| attachment.source_attachment_id.is_some());
    for attachment in &attachments {
        storage.insert_attachment(transaction, attachment)?;
    }

    transaction.execute("DELETE FROM note_revisions WHERE note_id = ?1", [&base.id])?;
    for (_, revision) in revisions_newest_first.iter().rev() {
        let snapshot_json = serde_json::to_string(&revision.snapshot)?;
        transaction.execute(
            "INSERT INTO note_revisions(id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &revision.revision_id,
                &base.id,
                revision.revision_no,
                &revision.snapshot_checksum,
                &snapshot_json,
                revision.snapshot_byte_size,
                revision.created_at,
            ],
        )?;
        for attachment in &revision.snapshot.attachments {
            transaction.execute(
                "INSERT INTO revision_attachment_refs(revision_id, attachment_id, sha256, byte_size)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    &revision.revision_id,
                    &attachment.id,
                    &attachment.sha256,
                    attachment.byte_size,
                ],
            )?;
        }
    }

    transaction.execute(
        "DELETE FROM note_links WHERE source_note_id = ?1",
        [&base.id],
    )?;
    for target in &graph.metadata_state.outgoing_note_links {
        transaction.execute(
            "INSERT OR IGNORE INTO note_links(source_note_id, target_note_id) VALUES (?1, ?2)",
            params![&base.id, target],
        )?;
    }

    for source_link in &graph.metadata_state.sources {
        transaction.execute(
            "INSERT OR IGNORE INTO sources(id, kind, title, reference, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &source_link.source.id,
                source_link.source.kind.value(),
                &source_link.source.title,
                &source_link.source.reference,
                source_link.source.created_at,
                source_link.source.updated_at,
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO note_sources(note_id, source_id, created_at) VALUES (?1, ?2, ?3)",
            params![&base.id, &source_link.source.id, source_link.linked_at],
        )?;
        for citation in &source_link.citations {
            transaction.execute(
                "INSERT OR IGNORE INTO citations(id, note_id, source_id, locator, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    &citation.id,
                    &base.id,
                    &citation.source_id,
                    &citation.locator,
                    citation.created_at,
                ],
            )?;
        }
    }
    for annotation in &graph.metadata_state.pdf_annotations {
        transaction.execute(
            "INSERT OR REPLACE INTO pdf_annotation_pages(attachment_id, page_index, annotations_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                &annotation.attachment_id,
                annotation.page_index,
                &annotation.annotations_json,
                annotation.updated_at,
            ],
        )?;
    }

    let mut search_note = note;
    search_note.protection = NoteProtectionState::Unprotected;
    search_note.notebook_id = base.notebook_id.clone();
    search_note.pinned = base.pinned;
    search_note.archived = base.archived;
    search_note.trashed = base.trashed;
    search_note.purge_after = base.purge_after;
    search_note.created_at = base.created_at;
    search_note.updated_at = base.updated_at;
    storage.refresh_search_index(transaction, &search_note)?;
    Ok(())
}

fn load_revision_snapshots(
    storage: &Storage,
    connection: &Connection,
    note_id: &NoteId,
) -> StorageResult<Vec<(NoteRevision, RevisionSnapshot)>> {
    let mut statement = connection.prepare(
        "SELECT id, revision_no, checksum, snapshot_json, byte_size, created_at
         FROM note_revisions WHERE note_id = ?1 ORDER BY revision_no",
    )?;
    let rows = statement.query_map([note_id], |row| {
        Ok(NoteRevision {
            id: row.get(0)?,
            note_id: note_id.clone(),
            revision_no: row.get(1)?,
            checksum: row.get(2)?,
            snapshot_json: row.get(3)?,
            byte_size: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        let revision = row?;
        let snapshot = storage.revision_snapshot(&revision)?;
        result.push((revision, snapshot));
    }
    Ok(result)
}

fn load_source_links(
    connection: &Connection,
    note_id: &NoteId,
) -> StorageResult<Vec<SensitiveSourceLink>> {
    let mut statement = connection.prepare(
        "SELECT s.id, s.kind, s.title, s.reference, s.created_at, s.updated_at, ns.created_at\n         FROM note_sources ns JOIN sources s ON s.id = ns.source_id\n         WHERE ns.note_id = ?1 ORDER BY ns.created_at, s.id",
    )?;
    let sources = statement
        .query_map([note_id], |row| {
            let kind_value = row.get::<_, String>(1)?;
            let kind = SourceKind::parse(&kind_value).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid source kind {kind_value}"),
                    )),
                )
            })?;
            Ok((
                Source {
                    id: row.get::<_, SourceId>(0)?,
                    kind,
                    title: row.get(2)?,
                    reference: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                },
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut result = Vec::with_capacity(sources.len());
    for (source, linked_at) in sources {
        let mut citation_statement = connection.prepare(
            "SELECT id, locator, created_at FROM citations\n             WHERE note_id = ?1 AND source_id = ?2 ORDER BY created_at, id",
        )?;
        let citations = citation_statement
            .query_map(params![note_id, &source.id], |row| {
                Ok(Citation {
                    id: row.get(0)?,
                    note_id: note_id.clone(),
                    source_id: source.id.clone(),
                    locator: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        result.push(SensitiveSourceLink {
            source,
            linked_at,
            citations,
        });
    }
    Ok(result)
}

fn load_pdf_annotations(
    connection: &Connection,
    note_id: &NoteId,
) -> StorageResult<Vec<SensitivePdfAnnotation>> {
    let mut statement = connection.prepare(
        "SELECT p.attachment_id, p.page_index, p.annotations_json, p.updated_at\n         FROM pdf_annotation_pages p\n         JOIN attachments a ON a.id = p.attachment_id\n         WHERE a.note_id = ?1 ORDER BY p.attachment_id, p.page_index",
    )?;
    let rows = statement.query_map([note_id], |row| {
        Ok(SensitivePdfAnnotation {
            attachment_id: row.get(0)?,
            page_index: row.get(1)?,
            annotations_json: row.get(2)?,
            updated_at: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_outgoing_note_links(
    connection: &Connection,
    note_id: &NoteId,
) -> StorageResult<Vec<NoteId>> {
    let mut statement = connection.prepare(
        "SELECT target_note_id FROM note_links WHERE source_note_id = ?1 ORDER BY target_note_id",
    )?;
    let rows = statement.query_map([note_id], |row| row.get::<_, NoteId>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Arc, mpsc},
        thread,
        time::Duration,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn test_storage(label: &str) -> (Storage, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rustnote-e2-{label}-{}-{unique}",
            std::process::id()
        ));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("test storage should initialize");
        (storage, root)
    }

    fn database_and_wal_bytes(storage: &Storage) -> Vec<u8> {
        let mut bytes = fs::read(&storage.database_path).expect("read database");
        let wal_path = PathBuf::from(format!("{}-wal", storage.database_path.display()));
        if let Ok(wal) = fs::read(wal_path) {
            bytes.extend_from_slice(&wal);
        }
        bytes
    }

    fn contains_marker(bytes: &[u8], marker: &str) -> bool {
        bytes
            .windows(marker.len())
            .any(|window| window == marker.as_bytes())
    }

    fn file_bytes_if_present(path: &Path) -> Vec<u8> {
        match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("failed to read forensic path {}: {error}", path.display()),
        }
    }

    fn directory_tree_bytes(root: &Path) -> Vec<u8> {
        fn collect(path: &Path, bytes: &mut Vec<u8>) {
            let metadata = fs::symlink_metadata(path)
                .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", path.display()));
            assert!(
                !metadata.file_type().is_symlink(),
                "managed backup forensic scan refuses symlink {}",
                path.display(),
            );
            if metadata.is_file() {
                bytes.extend_from_slice(
                    &fs::read(path).unwrap_or_else(|error| {
                        panic!("failed to read {}: {error}", path.display())
                    }),
                );
                return;
            }
            assert!(
                metadata.is_dir(),
                "unexpected repository entry {}",
                path.display()
            );
            let mut entries = fs::read_dir(path)
                .unwrap_or_else(|error| panic!("failed to list {}: {error}", path.display()))
                .collect::<std::io::Result<Vec<_>>>()
                .unwrap_or_else(|error| panic!("failed to enumerate {}: {error}", path.display()));
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                collect(&entry.path(), bytes);
            }
        }

        let mut bytes = Vec::new();
        collect(root, &mut bytes);
        bytes
    }

    #[test]
    fn protect_and_lock_complete_without_reentering_privacy_admission() {
        let (storage, root) = test_storage("transition-non-reentrant");
        let note = storage.create_note(None).expect("create note");
        let protected = storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("Protect completes without self-wait");
        assert!(protected.protection.is_unlocked());
        let locked = storage
            .lock_protected_note(&note.id)
            .expect("Lock completes without self-wait");
        assert!(locked.protection.is_locked());
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ordinary_source_resolves_protected_target_title_only_while_target_is_unlocked() {
        let (storage, root) = test_storage("ordinary-link-protected-target");
        let mut target = storage.create_note(None).expect("create target");
        target.title = "Encrypted target title".to_string();
        target.updated_at = target.updated_at.saturating_add(1);
        let target = storage
            .save_note_owned_with_revision_status_scoped(
                target,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::TITLE,
            )
            .expect("save target title")
            .0;
        storage
            .protect_note_with_password(&target.id, b"correct horse battery staple")
            .expect("protect target");

        let mut source = storage.create_note(None).expect("create source");
        let editor_state = serde_json::json!({
            "schema_version": 2,
            "editor_engine": "prosemirror",
            "document_schema": "rustnote",
            "document_schema_version": 6,
            "document": {
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "protected target",
                        "marks": [{
                            "type": "note_link",
                            "attrs": {"noteId": target.id.as_str()}
                        }]
                    }]
                }]
            }
        })
        .to_string();
        let projections = derive_editor_projections(&editor_state).expect("link projection");
        source.replace_document(projections.markdown, editor_state, projections.plain_text);
        source.updated_at = source.updated_at.saturating_add(1);
        let source = storage
            .save_note_owned_with_revision_status_scoped(
                source,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::DOCUMENT,
            )
            .expect("save ordinary source link")
            .0;

        let unlocked = storage
            .list_note_link_targets(&source.id)
            .expect("unlocked protected target");
        let target_link = unlocked
            .iter()
            .find(|candidate| candidate.note_id == target.id)
            .expect("target link");
        assert_eq!(target_link.title.as_deref(), Some("Encrypted target title"));

        storage
            .lock_protected_note(&target.id)
            .expect("lock target");
        let locked = storage
            .list_note_link_targets(&source.id)
            .expect("locked protected target");
        let target_link = locked
            .iter()
            .find(|candidate| candidate.note_id == target.id)
            .expect("target link while locked");
        assert_eq!(target_link.title, None);

        storage
            .unlock_protected_note(&target.id, b"correct horse battery staple")
            .expect("unlock target");
        let unlocked_again = storage
            .list_note_link_targets(&source.id)
            .expect("re-unlocked protected target");
        let target_link = unlocked_again
            .iter()
            .find(|candidate| candidate.note_id == target.id)
            .expect("target link after unlock");
        assert_eq!(target_link.title.as_deref(), Some("Encrypted target title"));

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TestVaultGraphIds {
        manifest: String,
        private_state: String,
        metadata_state: String,
        attachment_index: String,
        revision_index: String,
        revision_head: Option<String>,
        revision_floor: Option<String>,
        revision_count: u64,
    }

    fn current_vault_graph_ids(storage: &Storage, note_id: &NoteId) -> TestVaultGraphIds {
        let access = storage
            .privacy_coordinator
            .acquire_note_access(note_id)
            .expect("Vault runtime state")
            .expect("protected note must be unlocked");
        let connection = storage.connection().expect("connection");
        let graph = storage
            .read_vault_current_graph(&connection, access.key(), note_id)
            .expect("read current graph")
            .expect("protected graph");
        TestVaultGraphIds {
            manifest: graph.root.manifest_object_id.as_str().to_string(),
            private_state: graph.manifest.note_private_state.as_str().to_string(),
            metadata_state: graph.manifest.metadata_state.as_str().to_string(),
            attachment_index: graph.manifest.attachment_index.as_str().to_string(),
            revision_index: graph.manifest.revision_index.as_str().to_string(),
            revision_head: graph
                .manifest
                .revision_head
                .as_ref()
                .map(|id| id.as_str().to_string()),
            revision_floor: graph
                .manifest
                .revision_floor
                .as_ref()
                .map(|id| id.as_str().to_string()),
            revision_count: graph.manifest.revision_count,
        }
    }

    fn editor_state_with_text(text: &str) -> String {
        serde_json::json!({
            "schema_version": 2,
            "editor_engine": "prosemirror",
            "document_schema": "rustnote",
            "document_schema_version": 6,
            "document": {
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{"type": "text", "text": text}]
                }]
            }
        })
        .to_string()
    }

    #[test]
    fn granular_protected_mutations_rewrite_only_their_object_class() {
        let (storage, root) = test_storage("granular-locality");
        let mut note = storage.create_note(None).expect("create note");
        note.title = "before protection".to_string();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save checkpoint")
            .0;
        let mut note = storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let before_title = current_vault_graph_ids(&storage, &note.id);
        note.title = "private title only".to_string();
        note.updated_at = note.updated_at.saturating_add(1);
        note = storage
            .save_note_owned_with_revision_status_scoped(
                note,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::TITLE,
            )
            .expect("save protected title")
            .0;
        let after_title = current_vault_graph_ids(&storage, &note.id);
        assert_ne!(after_title.manifest, before_title.manifest);
        assert_ne!(after_title.private_state, before_title.private_state);
        assert_eq!(after_title.metadata_state, before_title.metadata_state);
        assert_eq!(after_title.attachment_index, before_title.attachment_index);
        assert_eq!(after_title.revision_index, before_title.revision_index);
        assert_eq!(after_title.revision_head, before_title.revision_head);
        assert_eq!(after_title.revision_count, before_title.revision_count);

        let before_tags = after_title.clone();
        note.tags = vec!["private-tag".to_string()];
        note.updated_at = note.updated_at.saturating_add(1);
        note = storage
            .save_note_owned_with_revision_status_scoped(
                note,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::TAGS,
            )
            .expect("save protected tags")
            .0;
        let after_tags = current_vault_graph_ids(&storage, &note.id);
        assert_ne!(after_tags.manifest, before_tags.manifest);
        assert_eq!(after_tags.private_state, before_tags.private_state);
        assert_ne!(after_tags.metadata_state, before_tags.metadata_state);
        assert_eq!(after_tags.attachment_index, before_tags.attachment_index);
        assert_eq!(after_tags.revision_index, before_tags.revision_index);
        assert_eq!(after_tags.revision_head, before_tags.revision_head);

        let before_document = after_tags.clone();
        let editor_state = editor_state_with_text("granular protected typing");
        note.document = Arc::new(NoteDocument::new(
            String::new(),
            Arc::new(editor_state),
            String::new(),
        ));
        note.updated_at = note.updated_at.saturating_add(1);
        note = storage
            .save_note_owned_with_revision_status_scoped(
                note,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::DOCUMENT,
            )
            .expect("save protected document")
            .0;
        let after_document = current_vault_graph_ids(&storage, &note.id);
        assert_ne!(after_document.manifest, before_document.manifest);
        assert_ne!(after_document.private_state, before_document.private_state);
        assert_eq!(
            after_document.metadata_state,
            before_document.metadata_state
        );
        assert_eq!(
            after_document.attachment_index,
            before_document.attachment_index
        );
        assert_eq!(
            after_document.revision_index,
            before_document.revision_index
        );
        assert_eq!(after_document.revision_head, before_document.revision_head);

        let before_revision = after_document.clone();
        let editor_state = editor_state_with_text("granular protected checkpoint");
        note.document = Arc::new(NoteDocument::new(
            String::new(),
            Arc::new(editor_state),
            String::new(),
        ));
        note.updated_at = note.updated_at.saturating_add(1);
        let note_id = note.id.clone();
        storage
            .save_note_checkpoint_owned(note)
            .expect("append protected revision");
        let after_revision = current_vault_graph_ids(&storage, &note_id);
        assert_ne!(after_revision.manifest, before_revision.manifest);
        assert_ne!(after_revision.private_state, before_revision.private_state);
        assert_eq!(
            after_revision.metadata_state,
            before_revision.metadata_state
        );
        assert_eq!(
            after_revision.attachment_index,
            before_revision.attachment_index
        );
        assert_ne!(
            after_revision.revision_index,
            before_revision.revision_index
        );
        assert_ne!(after_revision.revision_head, before_revision.revision_head);
        assert_eq!(
            after_revision.revision_count,
            before_revision.revision_count.saturating_add(1)
        );

        let connection = storage.connection().expect("connection");
        let plaintext_revisions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_revisions WHERE note_id = ?1",
                [&note_id],
                |row| row.get(0),
            )
            .expect("plaintext revision count");
        assert_eq!(plaintext_revisions, 0);
        drop(connection);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only() {
        let (storage, root) = test_storage("granular-metadata-locality");
        let marker = format!(
            "RUSTNOTE-2B4-METADATA-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let note = storage.create_note(None).expect("create note");
        let pdf_path = root.join("metadata-locality.pdf");
        fs::write(&pdf_path, b"%PDF-1.4\nmetadata locality\n").expect("write pdf fixture");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "metadata-locality.pdf",
                Some("application/pdf"),
                &pdf_path,
            )
            .expect("add pdf attachment");
        let protected_note = storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        storage
            .save_note_checkpoint_owned(protected_note)
            .expect("seed recent protected revision baseline");

        let before_source = current_vault_graph_ids(&storage, &note.id);
        assert!(
            before_source.revision_head.is_some(),
            "metadata-locality autosave test requires a recent revision baseline"
        );
        let source = storage
            .create_source_and_attach(
                &note.id,
                SourceKind::Report,
                &marker,
                Some("https://example.invalid/protected-source"),
            )
            .expect("create protected source");
        let after_source = current_vault_graph_ids(&storage, &note.id);
        assert_ne!(
            after_source.manifest, before_source.manifest,
            "source mutation must publish a new manifest"
        );
        assert_eq!(
            after_source.private_state, before_source.private_state,
            "source mutation must not rewrite NotePrivateState"
        );
        assert_ne!(
            after_source.metadata_state, before_source.metadata_state,
            "source mutation must rewrite MetadataState"
        );
        assert_eq!(
            after_source.attachment_index, before_source.attachment_index,
            "source mutation must not rewrite AttachmentIndex"
        );
        assert_eq!(
            after_source.revision_head, before_source.revision_head,
            "source mutation must not append a revision"
        );
        assert_eq!(
            after_source.revision_count, before_source.revision_count,
            "source mutation must preserve revision count"
        );

        let before_citation = after_source.clone();
        storage
            .add_citation(&note.id, &source.id, "p. 42")
            .expect("add protected citation");
        let after_citation = current_vault_graph_ids(&storage, &note.id);
        assert_eq!(
            after_citation.private_state, before_citation.private_state,
            "citation mutation must not rewrite NotePrivateState"
        );
        assert_ne!(
            after_citation.metadata_state, before_citation.metadata_state,
            "citation mutation must rewrite MetadataState"
        );
        assert_eq!(
            after_citation.attachment_index, before_citation.attachment_index,
            "citation mutation must not rewrite AttachmentIndex"
        );
        assert_eq!(
            after_citation.revision_head, before_citation.revision_head,
            "citation mutation must not append a revision"
        );

        let before_annotation = after_citation.clone();
        storage
            .replace_protected_pdf_annotation_page_if_unlocked(
                &attachment.id,
                3,
                r#"[{"id":"protected-annotation","kind":"highlight"}]"#,
                false,
            )
            .expect("replace protected annotation")
            .expect("protected annotation path");
        let after_annotation = current_vault_graph_ids(&storage, &note.id);
        assert_eq!(
            after_annotation.private_state, before_annotation.private_state,
            "PDF annotation mutation must not rewrite NotePrivateState"
        );
        assert_ne!(
            after_annotation.metadata_state, before_annotation.metadata_state,
            "PDF annotation mutation must rewrite MetadataState"
        );
        assert_eq!(
            after_annotation.attachment_index, before_annotation.attachment_index,
            "PDF annotation mutation must not rewrite AttachmentIndex"
        );
        assert_eq!(
            after_annotation.revision_head, before_annotation.revision_head,
            "PDF annotation mutation must not append a revision"
        );

        let target = storage.create_note(None).expect("create note-link target");
        let mut linked_note = storage
            .get_note(&note.id)
            .expect("read protected note")
            .expect("protected note exists");
        let link_editor_state = serde_json::json!({
            "schema_version": 2,
            "editor_engine": "prosemirror",
            "document_schema": "rustnote",
            "document_schema_version": 6,
            "document": {
                "type": "doc",
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "linked target",
                        "marks": [{
                            "type": "note_link",
                            "attrs": {"noteId": target.id.as_str()}
                        }]
                    }]
                }]
            }
        })
        .to_string();
        let projections = derive_editor_projections(&link_editor_state).expect("link projection");
        linked_note.replace_document(
            projections.markdown,
            link_editor_state,
            projections.plain_text,
        );
        linked_note.updated_at = linked_note.updated_at.saturating_add(1);
        let before_link = after_annotation.clone();
        storage
            .save_note_owned_with_revision_status_scoped(
                linked_note,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::DOCUMENT,
            )
            .expect("save protected note link");
        let after_link = current_vault_graph_ids(&storage, &note.id);
        assert_ne!(
            after_link.private_state, before_link.private_state,
            "document link mutation must rewrite NotePrivateState"
        );
        assert_ne!(
            after_link.metadata_state, before_link.metadata_state,
            "document link mutation must rewrite MetadataState outgoing links"
        );
        assert_eq!(
            after_link.attachment_index, before_link.attachment_index,
            "document link mutation must not rewrite AttachmentIndex"
        );
        assert_eq!(
            after_link.revision_head, before_link.revision_head,
            "autosave link mutation must not append a revision inside the autosave interval"
        );

        let targets = storage
            .list_note_link_targets(&note.id)
            .expect("protected outgoing link targets while unlocked");
        assert!(
            targets
                .iter()
                .any(|candidate| candidate.note_id == target.id),
            "unlocked protected source must resolve its encrypted outgoing links"
        );
        let backlinks = storage
            .list_backlinks(&target.id)
            .expect("protected backlink while unlocked");
        assert!(
            backlinks
                .iter()
                .any(|candidate| candidate.source_note_id == note.id),
            "unlocked protected source must contribute backlinks from encrypted MetadataState"
        );

        storage
            .lock_protected_note(&note.id)
            .expect("lock protected source");
        assert!(
            storage
                .list_note_link_targets(&note.id)
                .expect("locked protected targets")
                .is_empty(),
            "locked protected source must not reveal encrypted outgoing links"
        );
        assert!(
            !storage
                .list_backlinks(&target.id)
                .expect("locked protected backlinks")
                .iter()
                .any(|candidate| candidate.source_note_id == note.id),
            "locked protected source must not reveal backlink membership"
        );
        storage
            .unlock_protected_note(&note.id, b"correct horse battery staple")
            .expect("unlock protected source");
        assert!(
            storage
                .list_note_link_targets(&note.id)
                .expect("restored protected targets")
                .iter()
                .any(|candidate| candidate.note_id == target.id),
            "unlock must restore encrypted outgoing-link behavior without a plaintext shadow"
        );

        let connection = storage.connection().expect("connection");
        let source_links: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_sources WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("note source count");
        let citation_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM citations WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("citation count");
        let annotation_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pdf_annotation_pages WHERE attachment_id = ?1",
                [&attachment.id],
                |row| row.get(0),
            )
            .expect("annotation count");
        let outgoing_link_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_links WHERE source_note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("outgoing note-link count");
        assert_eq!(
            source_links, 0,
            "protected sources must not create plaintext note_sources rows"
        );
        assert_eq!(
            citation_rows, 0,
            "protected citations must not create plaintext citation rows"
        );
        assert_eq!(
            annotation_rows, 0,
            "protected annotations must not create plaintext annotation rows"
        );
        assert_eq!(
            outgoing_link_rows, 0,
            "protected note links must not create plaintext note_links rows"
        );
        drop(connection);
        assert!(
            !contains_marker(&database_and_wal_bytes(&storage), &marker),
            "protected source metadata must not leak to ordinary SQLite/WAL",
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_manifest_swap_is_rollback_safe_and_missing_component_fails_closed() {
        let (storage, root) = test_storage("granular-root-atomicity");
        let note = storage.create_note(None).expect("create note");
        let note_id = note.id.clone();
        let note = storage
            .protect_note_with_password(&note_id, b"correct horse battery staple")
            .expect("protect note");
        let access = storage
            .privacy_coordinator
            .acquire_note_access(&note_id)
            .expect("runtime")
            .expect("unlocked access");
        let before = current_vault_graph_ids(&storage, &note_id);

        {
            let mut connection = storage.connection().expect("connection");
            let transaction = connection.transaction().expect("transaction");
            let graph = storage
                .read_vault_current_graph(&transaction, access.key(), &note_id)
                .expect("graph")
                .expect("protected graph");
            let mut private_state = graph.note_private_state.clone();
            private_state.title = "rolled back title".to_string();
            let new_private = storage
                .insert_vault_object(&transaction, access.key(), &note_id, &private_state)
                .expect("new private object");
            let manifest = VaultManifest::new(
                note.id.clone(),
                graph.manifest.schema_generation.saturating_add(1),
                new_private,
                graph.manifest.metadata_state.clone(),
                graph.manifest.attachment_index.clone(),
                graph.manifest.revision_index.clone(),
                graph.manifest.revision_head.clone(),
                graph.manifest.revision_floor.clone(),
                graph.manifest.revision_count,
            )
            .expect("manifest");
            storage
                .commit_vault_manifest_root(&transaction, access.key(), &graph.root, &manifest)
                .expect("swap root inside transaction");
            transaction.rollback().expect("rollback");
        }
        assert_eq!(current_vault_graph_ids(&storage, &note_id), before);

        // Model process death/restart after a pre-root transaction abort. The
        // previous committed root must remain the only graph visible after a
        // fresh Storage/Vault runtime is constructed and the note is unlocked.
        drop(access);
        drop(storage);
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("reopen storage after rolled-back manifest swap");
        storage
            .unlock_protected_note(&note_id, b"correct horse battery staple")
            .expect("unlock after restart");
        assert_eq!(
            current_vault_graph_ids(&storage, &note_id),
            before,
            "restart after a pre-commit crash boundary must retain the previous committed root",
        );
        let access = storage
            .privacy_coordinator
            .acquire_note_access(&note_id)
            .expect("runtime after reopen")
            .expect("unlocked access after reopen");

        // Also model restart after a successfully committed new root. A fresh
        // process must observe the new graph, never fall back to its predecessor.
        let mut committed_note = storage
            .get_note(&note_id)
            .expect("read reopened protected note")
            .expect("reopened note exists");
        committed_note.title = "committed title before restart".to_string();
        committed_note.updated_at = committed_note.updated_at.saturating_add(1);
        storage
            .save_note_owned_with_revision_status_scoped(
                committed_note,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::TITLE,
            )
            .expect("commit new root before restart");
        let committed = current_vault_graph_ids(&storage, &note_id);
        assert_ne!(committed.manifest, before.manifest);
        drop(access);
        drop(storage);
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("reopen storage after committed manifest swap");
        let reopened = storage
            .unlock_protected_note(&note_id, b"correct horse battery staple")
            .expect("unlock committed graph after restart");
        assert_eq!(reopened.title, "committed title before restart");
        assert_eq!(
            current_vault_graph_ids(&storage, &note_id),
            committed,
            "restart after manifest commit must expose the new committed root",
        );
        let access = storage
            .privacy_coordinator
            .acquire_note_access(&note_id)
            .expect("runtime after committed reopen")
            .expect("unlocked access after committed reopen");

        let missing_metadata = {
            let connection = storage.connection().expect("connection");
            let graph = storage
                .read_vault_current_graph(&connection, access.key(), &note_id)
                .expect("graph")
                .expect("protected graph");
            graph.manifest.metadata_state
        };
        let connection = storage.connection().expect("connection");
        connection
            .execute(
                "DELETE FROM vault_objects WHERE note_id = ?1 AND object_id = ?2",
                params![&note_id, missing_metadata.as_str()],
            )
            .expect("delete referenced object");
        drop(connection);
        let connection = storage.connection().expect("connection");
        assert!(
            storage
                .read_vault_current_graph(&connection, access.key(), &note_id)
                .is_err(),
            "missing referenced encrypted object must fail closed"
        );
        drop(connection);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn large_protected_revision_history_does_not_expand_autosave_mutation_scope() {
        let (storage, root) = test_storage("granular-large-history-locality");
        let note = storage.create_note(None).expect("create note");
        let mut note = storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        let baseline = current_vault_graph_ids(&storage, &note.id);

        const HISTORY_SIZE: u64 = 128;
        for index in 0..HISTORY_SIZE {
            note.title = format!("protected revision {index}");
            note.updated_at = note.updated_at.saturating_add(1);
            note = storage
                .save_note_checkpoint_owned(note)
                .unwrap_or_else(|error| panic!("append protected revision {index}: {error}"));
        }
        let after_history = current_vault_graph_ids(&storage, &note.id);
        assert_eq!(
            after_history.revision_count,
            baseline.revision_count.saturating_add(HISTORY_SIZE),
            "each checkpoint appends exactly one immutable RevisionObject",
        );

        let before_autosave = after_history.clone();
        let note_id = note.id.clone();
        note.title = "autosave after large protected history".to_string();
        note.updated_at = note.updated_at.saturating_add(1);
        storage
            .save_note_owned_with_revision_status_scoped(
                note,
                super::super::RevisionMode::Autosave,
                super::super::NoteSaveScope::TITLE,
            )
            .expect("autosave after large protected history");
        let after_autosave = current_vault_graph_ids(&storage, &note_id);
        assert_ne!(after_autosave.manifest, before_autosave.manifest);
        assert_ne!(after_autosave.private_state, before_autosave.private_state);
        assert_eq!(
            after_autosave.metadata_state,
            before_autosave.metadata_state
        );
        assert_eq!(
            after_autosave.attachment_index,
            before_autosave.attachment_index
        );
        assert_eq!(
            after_autosave.revision_index,
            before_autosave.revision_index
        );
        assert_eq!(after_autosave.revision_head, before_autosave.revision_head);
        assert_eq!(
            after_autosave.revision_count,
            before_autosave.revision_count
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_protected_revision_maintenance_prunes_tail_and_unreachable_objects() {
        let (storage, root) = test_storage("granular-revision-prune");
        let note = storage.create_note(None).expect("create note");
        let mut note = storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        for index in 0..14 {
            let editor_state = editor_state_with_text(&format!("protected revision {index}"));
            note.document = Arc::new(NoteDocument::new(
                String::new(),
                Arc::new(editor_state),
                String::new(),
            ));
            note.updated_at = note.updated_at.saturating_add(1);
            note = storage
                .save_note_checkpoint_owned(note)
                .expect("append protected checkpoint");
        }
        let before = current_vault_graph_ids(&storage, &note.id);
        assert!(before.revision_count > 10);

        let access = storage
            .privacy_coordinator
            .acquire_note_access(&note.id)
            .expect("runtime")
            .expect("unlocked access");
        {
            let mut connection = storage.connection().expect("connection");
            let transaction = connection.transaction().expect("transaction");
            let graph = storage
                .read_vault_current_graph(&transaction, access.key(), &note.id)
                .expect("graph")
                .expect("protected graph");
            let stale = storage
                .insert_vault_object(&transaction, access.key(), &note.id, &graph.metadata_state)
                .expect("unreachable object fixture");
            transaction.commit().expect("commit stale object fixture");
            let connection = storage.connection().expect("connection");
            let exists: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM vault_objects WHERE note_id = ?1 AND object_id = ?2",
                    params![&note.id, stale.as_str()],
                    |row| row.get(0),
                )
                .expect("stale object count");
            assert_eq!(exists, 1);
        }
        drop(access);

        storage
            .set_revision_retention(10, 0)
            .expect("prune protected revisions");
        let after = current_vault_graph_ids(&storage, &note.id);
        assert_eq!(after.revision_count, 10);
        assert_eq!(after.revision_head, before.revision_head);
        assert_ne!(after.revision_floor, before.revision_floor);
        let connection = storage.connection().expect("connection");
        let object_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM vault_objects WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("object count");
        assert_eq!(
            object_count, 15,
            "manifest + four mutable objects + ten revisions"
        );
        drop(connection);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lock_waits_for_scoped_protected_read_and_suppresses_late_publication() {
        let (storage, root) = test_storage("scoped-read-revocation");
        let mut note = storage.create_note(None).expect("create note");
        note.title = "revocation race secret".to_string();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save note")
            .0;
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let storage = Arc::new(storage);
        let (reader_started_tx, reader_started_rx) = mpsc::channel();
        let (release_reader_tx, release_reader_rx) = mpsc::channel();
        let storage_for_reader = Arc::clone(&storage);
        let note_for_reader = note.id.clone();
        let reader = thread::spawn(move || {
            storage_for_reader.with_unlocked_graph(&note_for_reader, |graph| {
                reader_started_tx.send(()).expect("reader started");
                release_reader_rx.recv().expect("release reader");
                Ok(graph.note_private_state.title)
            })
        });
        reader_started_rx
            .recv()
            .expect("reader entered scoped read");

        let (lock_done_tx, lock_done_rx) = mpsc::channel();
        let storage_for_lock = Arc::clone(&storage);
        let note_for_lock = note.id.clone();
        let lock = thread::spawn(move || {
            let result = storage_for_lock.lock_protected_note(&note_for_lock);
            lock_done_tx.send(result).expect("lock result");
        });

        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !storage
                .privacy_coordinator
                .contains_note_key(&note.id)
                .expect("runtime state")
            {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should publish revocation promptly"
        );
        assert!(
            lock_done_rx.try_recv().is_err(),
            "Lock must wait while the scoped decrypted protected state is in use"
        );

        release_reader_tx.send(()).expect("release reader");
        assert!(
            reader
                .join()
                .expect("join reader")
                .expect("reader result")
                .is_none(),
            "a read finishing after revocation starts must not publish derived plaintext"
        );
        let locked = lock_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lock completion")
            .expect("lock succeeds");
        assert!(locked.protection.is_locked());
        lock.join().expect("join lock");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_revision_preview_resource_holds_lock_until_derivation_lifetime_ends() {
        let (storage, root) = test_storage("revision-preview-lock-lifetime");
        let mut note = storage.create_note(None).expect("create note");
        note.title = "revision preview secret".to_string();
        note.updated_at = unix_timestamp();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save revision checkpoint")
            .0;
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        let revision_id = storage
            .list_revision_summaries(&note.id)
            .expect("list protected revisions")
            .first()
            .expect("protected revision")
            .id
            .clone();

        let storage = Arc::new(storage);
        let resource = storage
            .revision_snapshot_resource_for_note(&note.id, &revision_id)
            .expect("prepare protected revision preview resource");
        assert_eq!(resource.snapshot().note.title, "revision preview secret");

        let (lock_done_tx, lock_done_rx) = mpsc::channel();
        let storage_for_lock = Arc::clone(&storage);
        let note_for_lock = note.id.clone();
        let lock = thread::spawn(move || {
            let result = storage_for_lock.lock_protected_note(&note_for_lock);
            lock_done_tx.send(result).expect("lock result");
        });

        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !storage
                .privacy_coordinator
                .contains_note_key(&note.id)
                .expect("runtime state")
            {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should begin revocation while revision preview derivation is active",
        );
        assert!(
            lock_done_rx.try_recv().is_err(),
            "Lock must wait through the protected revision preview derivation lifetime",
        );

        drop(resource);
        let locked = lock_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lock completion")
            .expect("lock succeeds after revision preview lifetime ends");
        assert!(locked.protection.is_locked());
        lock.join().expect("join lock");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_attachment_command_resource_holds_lock_until_sensitive_lifetime_ends() {
        let (storage, root) = test_storage("attachment-command-lock-lifetime");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("command-secret.txt");
        fs::write(&source, b"protected attachment command secret").expect("write source");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "command-secret-name.txt",
                Some("text/plain"),
                &source,
            )
            .expect("add attachment");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let storage = Arc::new(storage);
        let resource = storage
            .attachment_store()
            .attachment_file_command_resource(&note.id, &attachment.id)
            .expect("prepare protected command resource");
        assert_eq!(resource.attachment().file_name, "command-secret-name.txt");
        assert!(
            resource.path().exists(),
            "protected command should own a materialized file"
        );

        let (lock_done_tx, lock_done_rx) = mpsc::channel();
        let storage_for_lock = Arc::clone(&storage);
        let note_for_lock = note.id.clone();
        let lock = thread::spawn(move || {
            let result = storage_for_lock.lock_protected_note(&note_for_lock);
            lock_done_tx.send(result).expect("lock result");
        });

        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !storage
                .privacy_coordinator
                .contains_note_key(&note.id)
                .expect("runtime state")
            {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should begin revocation while command is active"
        );
        assert!(
            lock_done_rx.try_recv().is_err(),
            "Lock must wait through the complete protected attachment command lifetime",
        );

        drop(resource);
        let locked = lock_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lock completion")
            .expect("lock succeeds after command lifetime ends");
        assert!(locked.protection.is_locked());
        lock.join().expect("join lock");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ordinary_attachment_command_resource_blocks_encrypt_until_command_lifetime_ends() {
        let (storage, root) = test_storage("attachment-command-encrypt-lifetime");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("ordinary-command-secret.txt");
        fs::write(&source, b"ordinary command staging secret").expect("write source");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "ordinary-command-secret.txt",
                Some("text/plain"),
                &source,
            )
            .expect("add attachment");

        let storage = Arc::new(storage);
        let resource = storage
            .attachment_store()
            .attachment_file_command_resource(&note.id, &attachment.id)
            .expect("prepare ordinary command resource");

        let (protect_done_tx, protect_done_rx) = mpsc::channel();
        let storage_for_protect = Arc::clone(&storage);
        let note_for_protect = note.id.clone();
        let protect = thread::spawn(move || {
            let result = storage_for_protect
                .protect_note_with_password(&note_for_protect, b"correct horse battery staple");
            protect_done_tx.send(result).expect("protect result");
        });

        thread::sleep(Duration::from_millis(25));
        assert!(
            protect_done_rx.try_recv().is_err(),
            "Encrypt must wait while an ordinary attachment command owns plaintext metadata/files",
        );
        drop(resource);
        let protected = protect_done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("protect completion")
            .expect("protect succeeds after command lifetime ends");
        assert!(protected.protection.is_unlocked());
        protect.join().expect("join protect");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_note_export_resource_holds_lock_until_native_lifetime_ends() {
        let (storage, root) = test_storage("note-export-lock-lifetime");
        let note = storage.create_note(None).expect("create note");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let storage = Arc::new(storage);
        let resource = storage
            .export_store()
            .note_export_resource(&note.id)
            .expect("prepare protected note export resource");
        assert!(resource.is_publishable());

        let (lock_done_tx, lock_done_rx) = mpsc::channel();
        let storage_for_lock = Arc::clone(&storage);
        let note_for_lock = note.id.clone();
        let lock = thread::spawn(move || {
            let result = storage_for_lock.lock_protected_note(&note_for_lock);
            lock_done_tx.send(result).expect("lock result");
        });

        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !resource.is_publishable() {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should revoke publication while protected note export is active",
        );
        assert!(
            lock_done_rx.try_recv().is_err(),
            "Lock must wait through the complete protected note native export lifetime",
        );

        drop(resource);
        let locked = lock_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("lock completion")
            .expect("lock succeeds after note export lifetime ends");
        assert!(locked.protection.is_locked());
        lock.join().expect("join lock");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ordinary_note_export_resource_blocks_encrypt_until_native_lifetime_ends() {
        let (storage, root) = test_storage("note-export-encrypt-lifetime");
        let note = storage.create_note(None).expect("create note");
        let storage = Arc::new(storage);
        let resource = storage
            .export_store()
            .note_export_resource(&note.id)
            .expect("prepare ordinary note export resource");

        let (protect_done_tx, protect_done_rx) = mpsc::channel();
        let storage_for_protect = Arc::clone(&storage);
        let note_for_protect = note.id.clone();
        let protect = thread::spawn(move || {
            let result = storage_for_protect
                .protect_note_with_password(&note_for_protect, b"correct horse battery staple");
            protect_done_tx.send(result).expect("protect result");
        });

        thread::sleep(Duration::from_millis(25));
        assert!(
            protect_done_rx.try_recv().is_err(),
            "Encrypt must wait while an ordinary note export owns plaintext lifetime",
        );
        drop(resource);
        let protected = protect_done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("protect completion")
            .expect("protect succeeds after note export lifetime ends");
        assert!(protected.protection.is_unlocked());
        protect.join().expect("join protect");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables() {
        let (storage, root) = test_storage("protected-revision-restore");
        let old_marker = format!(
            "RUSTNOTE-E2-RESTORE-OLD-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let new_marker = format!("{old_marker}-NEW");
        let old_tag = format!("{old_marker}-TAG");

        let mut note = storage.create_note(None).expect("create note");
        note.title = old_marker.clone();
        note.tags = vec![old_tag.clone()];
        note.updated_at = unix_timestamp();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save old checkpoint")
            .0;

        let mut latest = note.clone();
        latest.title = new_marker.clone();
        latest.tags = vec![format!("{new_marker}-TAG")];
        latest.updated_at = unix_timestamp().saturating_add(1);
        storage
            .save_note_owned_with_revision_status_for_test(
                latest,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save new checkpoint");

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note with revision history");
        let summaries = storage
            .list_revision_summaries(&note.id)
            .expect("list protected revisions");
        let old_revision_id = summaries
            .iter()
            .find_map(|summary| {
                let snapshot = storage
                    .revision_snapshot_for_note(&note.id, &summary.id)
                    .expect("read protected revision snapshot");
                (snapshot.note.title == old_marker).then(|| summary.id.clone())
            })
            .expect("old protected revision must exist");

        let (restored, authorization) = storage
            .restore_protected_revision_if_unlocked(&note.id, &old_revision_id)
            .expect("restore protected revision")
            .expect("protected restore path");
        assert_eq!(restored.note.title, old_marker);
        assert_eq!(restored.note.tags, vec![old_tag.clone()]);
        assert!(restored.note.protection.is_unlocked());

        let connection = storage.connection().expect("connection");
        let ordinary_row: (String, String, String) = connection
            .query_row(
                "SELECT title, body, plain_text FROM notes WHERE id = ?1",
                [&note.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("protected stub");
        assert_eq!(
            ordinary_row,
            ("Locked note".to_string(), String::new(), String::new())
        );
        let revision_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_revisions WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("ordinary revision count");
        assert_eq!(
            revision_count, 0,
            "protected restore must not repopulate note_revisions"
        );
        let tag_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_tags WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("ordinary tag count");
        assert_eq!(
            tag_count, 0,
            "protected restore must not repopulate note_tags"
        );
        let fts_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("ordinary FTS count");
        assert_eq!(
            fts_count, 0,
            "protected restore must not repopulate disk FTS"
        );
        drop(connection);
        // Restore intentionally returns a retained Vault authorization so UI
        // projection/publication can finish safely. This test validates the
        // committed graph, not that lifetime contract, so release it before
        // exercising Lock. A dedicated test below covers Lock waiting on the
        // retained authorization.
        drop(authorization);

        storage
            .lock_protected_note(&note.id)
            .expect("lock restored note");
        let unlocked = storage
            .unlock_protected_note(&note.id, b"correct horse battery staple")
            .expect("unlock restored note");
        assert_eq!(
            unlocked.title, old_marker,
            "restored state must be canonical ciphertext state"
        );
        assert_eq!(unlocked.tags, vec![old_tag]);

        let persisted = database_and_wal_bytes(&storage);
        assert!(
            !contains_marker(&persisted, &old_marker),
            "restored protected title must not reappear as plaintext in SQLite/WAL",
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_restore_normalization_reuses_retained_authorization_without_lock_cycle() {
        let (storage, root) = test_storage("protected-restore-normalization-authorization");
        let mut note = storage.create_note(None).expect("create note");
        note.title = "restore normalization old".to_string();
        note.updated_at = unix_timestamp();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save old revision")
            .0;
        let mut latest = note.clone();
        latest.title = "restore normalization latest".to_string();
        latest.updated_at = latest.updated_at.saturating_add(1);
        storage
            .save_note_owned_with_revision_status_for_test(
                latest,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save latest revision");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let revision_id = storage
            .list_revision_summaries(&note.id)
            .expect("list revisions")
            .into_iter()
            .find_map(|summary| {
                let snapshot = storage
                    .revision_snapshot_for_note(&note.id, &summary.id)
                    .expect("revision snapshot");
                (snapshot.note.title == "restore normalization old").then_some(summary.id)
            })
            .expect("old revision");
        let (restored, authorization) = storage
            .restore_protected_revision_if_unlocked(&note.id, &revision_id)
            .expect("restore revision")
            .expect("protected restore path");

        storage
            .persist_protected_runtime_normalization_after_restore(&restored.note, &authorization)
            .expect("normalization may reuse the retained Restore access");

        let storage = Arc::new(storage);
        let (lock_done_tx, lock_done_rx) = mpsc::channel();
        let lock_storage = Arc::clone(&storage);
        let lock_note = note.id.clone();
        let lock = thread::spawn(move || {
            let result = lock_storage.lock_protected_note(&lock_note);
            lock_done_tx.send(result).expect("lock result");
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while authorization
            .access()
            .permit()
            .is_publishable()
            .expect("restore authorization state")
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(
            !authorization
                .access()
                .permit()
                .is_publishable()
                .expect("Restore authorization revoked"),
            "Lock must revoke the retained Restore authorization before waiting for it"
        );
        assert!(
            lock_done_rx.try_recv().is_err(),
            "Lock must still wait while the Restore authorization is retained"
        );

        let normalization_storage = Arc::clone(&storage);
        let normalization_note = restored.note.clone();
        let normalization_authorization = authorization.clone();
        let (normalization_tx, normalization_rx) = mpsc::channel();
        let normalization = thread::spawn(move || {
            let result = normalization_storage
                .persist_protected_runtime_normalization_after_restore(
                    &normalization_note,
                    &normalization_authorization,
                );
            normalization_tx.send(result).expect("normalization result");
        });
        let normalization_result = normalization_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("normalization must fail closed rather than wait behind Lock");
        assert!(
            normalization_result.is_err(),
            "revoked Restore authorization must not write normalization"
        );
        normalization.join().expect("normalization thread");

        drop(authorization);
        let locked = lock_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Lock completes after Restore authorization release")
            .expect("Lock succeeds");
        assert!(locked.protection.is_locked());
        lock.join().expect("lock thread");

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn locked_note_public_label_is_visible_without_exposing_private_title() {
        let (storage, root) = test_storage("locked-public-label");
        let mut note = storage.create_note(None).expect("create note");
        let private_title = format!("PRIVATE-TITLE-{}", new_id("marker"));
        note.title = private_title.clone();
        note.updated_at = unix_timestamp();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save note")
            .0;

        storage
            .protect_note_with_password_and_public_label(
                &note.id,
                b"correct horse battery staple",
                "Tax records",
            )
            .expect("protect note with public label");
        storage.lock_protected_note(&note.id).expect("lock note");

        let locked = storage
            .get_note(&note.id)
            .expect("read locked note")
            .expect("locked note exists");
        assert!(locked.protection.is_locked());
        assert_eq!(locked.title, "Tax records");
        assert_eq!(locked.display_title(), "Tax records");

        let list_page = storage
            .list_notes_page_with_total(
                &crate::domain::NoteListQuery {
                    section: crate::domain::Section::Notes,
                    notebook_id: None,
                    search: String::new(),
                    sort: crate::domain::NoteSort::UpdatedNewest,
                    filters: crate::domain::NoteFilters::default(),
                },
                None,
            )
            .expect("list notes while protected");
        let locked_summary = list_page
            .notes
            .iter()
            .find(|summary| summary.id == note.id)
            .expect("locked summary remains listed");
        assert!(locked_summary.protection.is_locked());
        assert_eq!(locked_summary.title, "Tax records");
        assert_eq!(locked_summary.display_title(), "Tax records");
        assert!(locked_summary.preview_text.is_empty());
        assert!(locked_summary.tags.is_empty());
        assert!(locked_summary.attachment_names.is_empty());

        let connection = storage.connection().expect("connection");
        let row: (String, String, String) = connection
            .query_row(
                "SELECT title, body, plain_text FROM notes WHERE id = ?1",
                [&note.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("protected public stub");
        assert_eq!(
            row,
            ("Tax records".to_string(), String::new(), String::new())
        );
        drop(connection);

        let persisted = database_and_wal_bytes(&storage);
        assert!(
            !contains_marker(&persisted, &private_title),
            "the real encrypted title must not survive in SQLite/WAL plaintext",
        );

        let unlocked = storage
            .unlock_protected_note(&note.id, b"correct horse battery staple")
            .expect("unlock note");
        assert_eq!(unlocked.title, private_title);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_note_hides_plaintext_and_unlocks_only_with_password() {
        let (storage, root) = test_storage("note-roundtrip");
        let mut note = storage.create_note(None).expect("create note");
        note.title = "Highly confidential title".to_string();
        note.tags = vec!["secret-tag".to_string()];
        note.updated_at = unix_timestamp();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save note")
            .0;

        let protected = storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        assert!(protected.protection.is_unlocked());
        storage.lock_protected_note(&note.id).expect("lock note");

        let connection = storage.connection().expect("connection");
        let row: (String, String, String) = connection
            .query_row(
                "SELECT title, body, plain_text FROM notes WHERE id = ?1",
                [&note.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("protected row");
        assert_eq!(
            row,
            ("Locked note".to_string(), String::new(), String::new())
        );
        let fts_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("fts count");
        assert_eq!(fts_count, 0);
        drop(connection);

        assert!(
            storage
                .unlock_protected_note(&note.id, b"wrong password")
                .is_err()
        );
        let unlocked = storage
            .unlock_protected_note(&note.id, b"correct horse battery staple")
            .expect("unlock note");
        assert_eq!(unlocked.title, "Highly confidential title");
        assert_eq!(unlocked.tags, vec!["secret-tag"]);

        let restored = storage
            .remove_note_protection_with_password(&note.id, b"correct horse battery staple")
            .expect("remove protection");
        assert_eq!(restored.protection, NoteProtectionState::Unprotected);
        assert_eq!(restored.title, "Highly confidential title");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protection_commits_attachment_placeholder_under_schema_constraint() {
        let (storage, root) = test_storage("attachment-placeholder-constraint");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("constraint.txt");
        fs::write(&source, b"attachment placeholder constraint regression")
            .expect("write attachment source");
        let attachment = storage
            .add_attachment_file_as(&note.id, "constraint.txt", Some("text/plain"), &source)
            .expect("add attachment");

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("note with attachment must commit protection");

        let connection = storage.connection().expect("connection");
        let placeholder: (i64, Option<i64>) = connection
            .query_row(
                "SELECT embedded, unreferenced_at FROM attachments WHERE id = ?1",
                [&attachment.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("protected attachment placeholder");
        assert_eq!(placeholder.0, 0);
        assert!(
            placeholder.1.is_some(),
            "non-embedded protected placeholder must satisfy the attachment CHECK constraint"
        );
        drop(connection);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot() {
        let (storage, root) = test_storage("fts-forensic-scrub");
        let marker = format!(
            "RUSTNOTE-E3-FTS-FORENSIC-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let mut note = storage.create_note(None).expect("create note");
        note.title = marker.clone();
        note.updated_at = unix_timestamp();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save marker note")
            .0;

        assert!(
            contains_marker(&database_and_wal_bytes(&storage), &marker),
            "test marker must exist in SQLite/FTS bytes before protection"
        );

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect marker note");

        assert!(
            !contains_marker(&database_and_wal_bytes(&storage), &marker),
            "secure_delete + FTS rebuild + WAL truncation must remove protected plaintext bytes"
        );

        let backup_path = storage
            .maybe_create_automatic_backup()
            .expect("automatic backup after protection")
            .expect("first automatic backup must create a snapshot");
        let backup_bytes = fs::read(&backup_path).expect("read backup database");
        assert!(
            !contains_marker(&backup_bytes, &marker),
            "post-protection backup database must not preserve the protected marker"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups()
     {
        let (storage, root) = test_storage("no-vacuum-forensic-protect");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let historical_marker = format!("RUSTNOTE-E3-NOVAC-HISTORICAL-{nonce}");
        let current_marker = format!("RUSTNOTE-E3-NOVAC-CURRENT-{nonce}");

        let mut note = storage.create_note(None).expect("create note");
        note.title = historical_marker.clone();
        let mut note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save historical marker revision")
            .0;

        let pre_protection_backup = storage
            .maybe_create_automatic_backup()
            .expect("create pre-protection automatic backup")
            .expect("first automatic backup must create a snapshot");
        assert!(
            contains_marker(
                &fs::read(&pre_protection_backup).expect("read pre-protection backup"),
                &historical_marker,
            ),
            "the test must prove a Rustnote-managed backup contained historical plaintext before Protect",
        );

        note.title = current_marker.clone();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save current marker revision")
            .0;

        let before = database_and_wal_bytes(&storage);
        assert!(
            contains_marker(&before, &historical_marker),
            "historical revision plaintext must exist before the forensic Protect",
        );
        assert!(
            contains_marker(&before, &current_marker),
            "current plaintext must exist before the forensic Protect",
        );

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("production no-VACUUM Protect must complete");

        let database_bytes = fs::read(&storage.database_path).expect("read protected database");
        let wal_path = PathBuf::from(format!("{}-wal", storage.database_path.display()));
        let wal_bytes = file_bytes_if_present(&wal_path);
        for marker in [&historical_marker, &current_marker] {
            assert!(
                !contains_marker(&database_bytes, marker),
                "production no-VACUUM Protect must remove {marker} from the live database bytes",
            );
            assert!(
                !contains_marker(&wal_bytes, marker),
                "production no-VACUUM Protect must remove {marker} from WAL bytes",
            );
        }

        assert!(
            !pre_protection_backup.exists(),
            "Protect must invalidate the pre-protection managed backup snapshot",
        );
        let repository_bytes = directory_tree_bytes(&storage.automatic_backup_directory);
        for marker in [&historical_marker, &current_marker] {
            assert!(
                !contains_marker(&repository_bytes, marker),
                "managed backup repository must not retain {marker} after Protect",
            );
        }

        let post_protection_backup = storage
            .maybe_create_automatic_backup()
            .expect("create post-protection automatic backup")
            .expect("reset repository must permit a fresh automatic snapshot");
        let post_backup_bytes = fs::read(&post_protection_backup).expect("read protected backup");
        for marker in [&historical_marker, &current_marker] {
            assert!(
                !contains_marker(&post_backup_bytes, marker),
                "post-Protect backup snapshot must not contain {marker}",
            );
        }

        let repository_bytes = directory_tree_bytes(&storage.automatic_backup_directory);
        for marker in [&historical_marker, &current_marker] {
            assert!(
                !contains_marker(&repository_bytes, marker),
                "managed backup repository must remain free of {marker} after the new snapshot",
            );
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persistent_snapshots_fail_closed_while_vault_operation_is_pending() {
        let (storage, root) = test_storage("pending-vault-snapshot");
        let note = storage.create_note(None).expect("create note");
        let now = unix_timestamp();
        let connection = storage.connection().expect("connection");
        connection
            .execute(
                "INSERT INTO vault_operations(\n                     note_id, operation, phase, operation_id, details_json, created_at, updated_at\n                 ) VALUES (?1, 'protect', 'database_committed', 'test-pending-vault', '{}', ?2, ?2)",
                params![&note.id, now],
            )
            .expect("seed pending vault operation");
        drop(connection);

        let backup_error = storage
            .maybe_create_automatic_backup()
            .expect_err("automatic backup must refuse pending vault recovery");
        assert!(backup_error.to_string().contains("vault operation"));

        let archive_path = root.join("pending-vault.rustnote.zip");
        let archive_error = storage
            .export_complete_archive(&archive_path)
            .expect_err("complete archive must refuse pending vault recovery");
        assert!(archive_error.to_string().contains("vault operation"));
        assert!(!archive_path.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protection_scrubs_persisted_tag_filter_and_stale_rewrite_cannot_restore_it() {
        let (storage, root) = test_storage("tag-filter-residue");
        let secret_tag = "e3-secret-filter-tag".to_string();
        let public_tag = "public-filter-tag".to_string();

        let mut protected_note = storage.create_note(None).expect("create protected note");
        protected_note.tags = vec![secret_tag.clone()];
        let protected_note = storage
            .save_note_owned_with_revision_status_for_test(
                protected_note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save protected-note tags")
            .0;

        let mut ordinary_note = storage.create_note(None).expect("create ordinary note");
        ordinary_note.tags = vec![public_tag.clone()];
        storage
            .save_note_owned_with_revision_status_for_test(
                ordinary_note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save ordinary-note tags");

        let stale_filter = crate::domain::NoteFilters {
            tags: vec![secret_tag.clone(), public_tag.clone()],
            pinned: true,
            with_attachments: false,
            without_tags: false,
        }
        .to_storage();
        storage
            .set_note_list_preferences("updated-newest", &stale_filter)
            .expect("persist pre-protection filter");

        storage
            .protect_note_with_password(&protected_note.id, b"correct horse battery staple")
            .expect("protect note");

        let (_, filter_after_protection) = storage
            .note_list_preferences()
            .expect("read scrubbed preferences");
        let filters = crate::domain::NoteFilters::from_storage(&filter_after_protection);
        assert_eq!(filters.tags, vec![public_tag.clone()]);
        assert!(filters.pinned);

        // Simulate a delayed UI preference write captured before encryption.
        // The settings boundary must independently reject the now-private tag.
        storage
            .set_note_list_preferences("updated-newest", &stale_filter)
            .expect("persist stale filter");
        let (_, filter_after_stale_write) = storage
            .note_list_preferences()
            .expect("read re-sanitized preferences");
        let filters = crate::domain::NoteFilters::from_storage(&filter_after_stale_write);
        assert_eq!(filters.tags, vec![public_tag]);
        assert!(filters.pinned);

        let mut persisted_bytes = fs::read(&storage.database_path).expect("read database");
        let wal_path = PathBuf::from(format!("{}-wal", storage.database_path.display()));
        if let Ok(wal) = fs::read(&wal_path) {
            persisted_bytes.extend_from_slice(&wal);
        }
        assert!(
            !persisted_bytes
                .windows(secret_tag.len())
                .any(|window| window == secret_tag.as_bytes()),
            "protected-only tag filter text must not remain in SQLite/WAL",
        );

        let connection = storage.connection().expect("connection");
        let secret_tag_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE name = ?1 COLLATE NOCASE",
                [&secret_tag],
                |row| row.get(0),
            )
            .expect("secret tag count");
        assert_eq!(secret_tag_count, 0);
        drop(connection);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_note_scrubs_rustnote_managed_plaintext_residue() {
        let (storage, root) = test_storage("privacy-residue");
        let marker = format!(
            "RUSTNOTE-E3-PLAINTEXT-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let mut note = storage.create_note(None).expect("create note");
        note.title = marker.clone();
        note.updated_at = unix_timestamp();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save note")
            .0;

        let source = root.join("private-attachment.txt");
        fs::write(&source, marker.as_bytes()).expect("write source attachment");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "private-attachment.txt",
                Some("text/plain"),
                &source,
            )
            .expect("add attachment");

        let rendition_dir = storage.data_directory.join(
            crate::attachment_paths::image_rendition_cache_relative_directory(&attachment.sha256)
                .expect("rendition path"),
        );
        fs::create_dir_all(&rendition_dir).expect("create rendition cache");
        fs::write(rendition_dir.join("preview.bin"), marker.as_bytes())
            .expect("write rendition residue");

        let media_cache = crate::library_paths::media_compat_directory(&storage.data_directory);
        fs::create_dir_all(&media_cache).expect("create media cache");
        let media_residue = media_cache.join(format!("{}.e3-test.cache", attachment.sha256));
        fs::write(&media_residue, marker.as_bytes()).expect("write media residue");

        let orphaned_dir = storage.attachments_directory.join(".orphaned");
        fs::create_dir_all(&orphaned_dir).expect("create orphan quarantine");
        let orphaned_relative = format!(
            ".orphaned/{}-{}-{}",
            unix_timestamp(),
            new_id("orphan"),
            attachment.sha256
        );
        let mut orphaned_owner = storage
            .reserve_orphan_blob_owned_plaintext(&orphaned_relative)
            .expect("reserve orphaned residue owner");
        let orphaned_residue = orphaned_owner.path().to_path_buf();
        use std::io::Write as _;
        let mut orphaned_file = orphaned_owner
            .create_file()
            .expect("create orphaned residue");
        orphaned_file
            .write_all(marker.as_bytes())
            .expect("write orphaned residue");
        drop(orphaned_file);

        let unrelated_relative = format!(
            ".orphaned/{}-{}-{}",
            unix_timestamp(),
            new_id("orphan"),
            "0".repeat(64)
        );
        let mut unrelated_owner = storage
            .reserve_orphan_blob_owned_plaintext(&unrelated_relative)
            .expect("reserve unrelated orphan owner");
        let unrelated_orphan = unrelated_owner.path().to_path_buf();
        let mut unrelated_file = unrelated_owner
            .create_file()
            .expect("create unrelated orphan");
        unrelated_file
            .write_all(b"unrelated orphan history")
            .expect("write unrelated orphan");
        drop(unrelated_file);

        let handoff_root =
            crate::library_paths::attachment_handoff_directory(&storage.data_directory);
        let handoff_process_dir = handoff_root.join(std::process::id().to_string());
        let handoff_dir = handoff_process_dir.join(&attachment.sha256);
        fs::create_dir_all(&handoff_dir).expect("create handoff residue");
        fs::write(
            handoff_dir.join("private-attachment.txt"),
            marker.as_bytes(),
        )
        .expect("write handoff residue");

        // Pending attachment opens do not yet have a content hash. Their handoff
        // copies are keyed to the owning note so Encrypt can still invalidate them.
        let pending_handoff_dir = handoff_process_dir.join(
            crate::native::handoff::note_handoff_source_key(note.id.as_str()),
        );
        fs::create_dir_all(&pending_handoff_dir).expect("create pending handoff residue");
        fs::write(
            pending_handoff_dir.join("pending-private-attachment.txt"),
            marker.as_bytes(),
        )
        .expect("write pending handoff residue");

        let backup_residue = storage
            .automatic_backup_directory
            .join("Rustnote Repository")
            .join("snapshots")
            .join("pre-encryption-snapshot.txt");
        fs::create_dir_all(backup_residue.parent().expect("backup parent"))
            .expect("create backup residue");
        fs::write(&backup_residue, marker.as_bytes()).expect("write backup residue");

        let mut before = fs::read(&storage.database_path).unwrap_or_default();
        let wal_path = PathBuf::from(format!("{}-wal", storage.database_path.display()));
        if let Ok(wal) = fs::read(&wal_path) {
            before.extend_from_slice(&wal);
        }
        assert!(
            before
                .windows(marker.len())
                .any(|window| window == marker.as_bytes()),
            "test plaintext must exist in SQLite/WAL before protection"
        );

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note and scrub managed residue");

        assert!(
            !rendition_dir.exists(),
            "image rendition residue must be removed"
        );
        assert!(
            !media_residue.exists(),
            "media compatibility residue must be removed"
        );
        assert!(
            !orphaned_residue.exists(),
            "matching attachment quarantine residue must be removed",
        );
        assert!(
            unrelated_orphan.exists(),
            "targeted protection cleanup must preserve unrelated orphan history",
        );
        assert!(
            !handoff_dir.exists(),
            "attachment handoff residue must be removed"
        );
        assert!(
            !pending_handoff_dir.exists(),
            "note-scoped pending handoff residue must be removed",
        );
        assert!(
            !backup_residue.exists(),
            "pre-encryption backup residue must be removed"
        );

        let mut after = fs::read(&storage.database_path).expect("read scrubbed database");
        if let Ok(wal) = fs::read(&wal_path) {
            after.extend_from_slice(&wal);
        }
        assert!(
            !after
                .windows(marker.len())
                .any(|window| window == marker.as_bytes()),
            "SQLite database/WAL must not retain the protected note marker after E3 scrub"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_note_encrypts_attachment_blob_and_restores_it_losslessly() {
        let (storage, root) = test_storage("attachment-roundtrip");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("classified.txt");
        let plaintext = b"classified attachment bytes";
        fs::write(&source, plaintext).expect("write attachment source");
        let attachment = storage
            .add_attachment_file_as(&note.id, "classified.txt", Some("text/plain"), &source)
            .expect("add attachment");
        let plaintext_path = storage
            .blob_path_for_hash(&attachment.sha256)
            .expect("plaintext blob path");
        assert!(plaintext_path.exists());

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        storage.lock_protected_note(&note.id).expect("lock note");
        assert!(!plaintext_path.exists());

        let connection = storage.connection().expect("connection");
        let placeholder: (String, Option<String>, String, i64, Option<i64>) = connection
            .query_row(
                "SELECT file_name, mime_type, sha256, embedded, unreferenced_at FROM attachments WHERE id = ?1",
                [&attachment.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("protected attachment placeholder");
        assert_eq!(placeholder.0, LOCKED_ATTACHMENT_NAME);
        assert_eq!(placeholder.1, None);
        assert_ne!(placeholder.2, attachment.sha256);
        assert_eq!(placeholder.3, 0);
        assert!(placeholder.4.is_some());
        let protected_blob_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM vault_blob_refs WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("protected blob count");
        assert_eq!(protected_blob_count, 1);
        drop(connection);

        storage
            .unlock_protected_note(&note.id, b"correct horse battery staple")
            .expect("unlock note");
        let listed = storage
            .list_attachments(&note.id)
            .expect("list protected attachments");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].file_name, "classified.txt");
        assert_eq!(listed[0].sha256, attachment.sha256);
        let resource = storage
            .protected_attachment_resource(&attachment.id)
            .expect("protected attachment resource")
            .expect("protected attachment should exist");
        assert_eq!(
            fs::read(&resource.path).expect("read materialized attachment"),
            plaintext
        );
        // The materialized resource intentionally retains a VaultReadPermit for its
        // entire plaintext lifetime. Release it before Unprotect, which must drain
        // active Vault readers before replacing protected state with plaintext.
        drop(resource);

        storage
            .remove_note_protection_with_password(&note.id, b"correct horse battery staple")
            .expect("remove protection");
        assert_eq!(
            fs::read(&plaintext_path).expect("read restored attachment"),
            plaintext
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn failed_protected_attachment_authentication_leaves_no_untracked_plaintext() {
        let (storage, root) = test_storage("attachment-auth-failure-cleanup");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("tamper-source.txt");
        fs::write(
            &source,
            b"plaintext that must not survive failed authentication",
        )
        .expect("write attachment source");
        let attachment = storage
            .add_attachment_file_as(&note.id, "tamper-source.txt", Some("text/plain"), &source)
            .expect("add attachment");

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        storage.lock_protected_note(&note.id).expect("lock note");

        let ciphertext_hash: String = {
            let connection = storage.connection().expect("connection");
            connection
                .query_row(
                    "SELECT sha256 FROM vault_blob_refs WHERE note_id = ?1 LIMIT 1",
                    [&note.id],
                    |row| row.get(0),
                )
                .expect("protected ciphertext hash")
        };
        let cipher_path = storage
            .blob_path_for_hash(&ciphertext_hash)
            .expect("ciphertext path");
        let mut ciphertext = fs::read(&cipher_path).expect("read ciphertext");
        let last = ciphertext.last_mut().expect("non-empty ciphertext");
        *last ^= 0x01;
        fs::write(&cipher_path, ciphertext).expect("tamper ciphertext");

        storage
            .unlock_protected_note(&note.id, b"correct horse battery staple")
            .expect("unlock note");
        let probe = storage
            .privacy_coordinator
            .allocate_vault_plaintext_path()
            .expect("prepare vault temp owner");
        let temp_root = probe.parent().expect("vault temp root").to_path_buf();
        assert!(!probe.exists(), "path allocation must not create plaintext");

        let _error = storage
            .protected_attachment_resource(&attachment.id)
            .expect_err("tampered protected attachment must fail authentication");
        let unexpected = fs::read_dir(&temp_root)
            .expect("inspect vault temp root")
            .filter_map(Result::ok)
            .filter(|entry| !crate::native::temp_owner::is_owner_artifact_name(&entry.file_name()))
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert!(
            unexpected.is_empty(),
            "failed attachment authentication must not leave materialized plaintext: {unexpected:?}"
        );
        storage
            .lock_protected_note(&note.id)
            .expect("Lock must still complete after failed materialization cleanup");

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn retired_plaintext_restore_rejects_ambiguous_absence_and_retries_after_repair() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "rustnote-vault-retired-ambiguous-{}",
            new_id("rollback")
        ));
        fs::create_dir_all(&root).expect("test root");
        let loop_parent = root.join("loop");
        symlink("loop", &loop_parent).expect("symlink loop");
        let retired_path = loop_parent.join("retired.bin");
        let original_path = root.join("original.bin");
        let note_id = NoteId::parse("note-retired-ambiguous").expect("note id");
        let owned = OwnedPlaintext::durable_vault_operation_existing(
            retired_path.clone(),
            "vault-operation-ambiguous".to_string(),
            note_id,
        );
        let mut retired = vec![RetiredPlaintextBlob {
            original_path: original_path.clone(),
            plaintext: owned,
        }];

        assert!(
            restore_retired_plaintext_blobs(&mut retired).is_err(),
            "ambiguous metadata must not be treated as an already-restored plaintext path"
        );
        assert_eq!(retired[0].plaintext.path(), retired_path);

        fs::remove_file(&loop_parent).expect("remove symlink loop");
        fs::create_dir(&loop_parent).expect("repair parent");
        fs::write(&retired_path, b"retired plaintext").expect("materialize retired plaintext");
        restore_retired_plaintext_blobs(&mut retired).expect("retry rollback after repair");
        assert_eq!(retired[0].plaintext.path(), original_path);
        assert_eq!(
            fs::read(&original_path).expect("restored bytes"),
            b"retired plaintext"
        );
        assert!(!retired_path.exists());

        let mut restored = retired.pop().expect("retired owner").plaintext;
        restored.remove_file().expect("cleanup restored plaintext");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_recovery_restores_plaintext_retired_before_protection_commit() {
        let (storage, root) = test_storage("vault-recover-protect-prepared");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("recover.txt");
        let plaintext = b"recover me after interrupted protection";
        fs::write(&source, plaintext).expect("write source");
        let attachment = storage
            .add_attachment_file_as(&note.id, "recover.txt", Some("text/plain"), &source)
            .expect("add attachment");
        let canonical = storage
            .blob_path_for_hash(&attachment.sha256)
            .expect("canonical blob");

        let operation_id = "vault_test_protect_prepared";
        let details = VaultOperationDetails {
            ciphertext_hashes: Vec::new(),
        };
        storage
            .begin_vault_operation(&note.id, VAULT_OPERATION_PROTECT, operation_id, &details)
            .expect("begin journal");
        let retiring = storage.retiring_operation_directory(operation_id);
        fs::create_dir_all(&retiring).expect("retiring dir");
        fs::rename(&canonical, retiring.join("opaque-retired-file")).expect("retire plaintext");
        assert!(!canonical.exists());

        let data_directory = storage.data_directory.clone();
        let backup_directory = storage.automatic_backup_directory.clone();
        drop(storage);
        let storage = Storage::at_with_backup(data_directory, backup_directory)
            .expect("restart library and recover prepared protect");
        assert_eq!(fs::read(&canonical).expect("restored blob"), plaintext);
        let connection = storage.connection().expect("connection");
        let operations: i64 = connection
            .query_row("SELECT COUNT(*) FROM vault_operations", [], |row| {
                row.get(0)
            })
            .expect("operation count");
        assert_eq!(operations, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_recovery_finishes_plaintext_after_unprotect_commit() {
        let (storage, root) = test_storage("vault-recover-unprotect-committed");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("recover-after-commit.txt");
        let plaintext = b"publish me after interrupted deprotection";
        fs::write(&source, plaintext).expect("write source");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "recover-after-commit.txt",
                Some("text/plain"),
                &source,
            )
            .expect("add attachment");
        let canonical = storage
            .blob_path_for_hash(&attachment.sha256)
            .expect("canonical blob");

        let operation_id = "vault_test_unprotect_committed";
        let details = VaultOperationDetails {
            ciphertext_hashes: Vec::new(),
        };
        storage
            .begin_vault_operation(&note.id, VAULT_OPERATION_UNPROTECT, operation_id, &details)
            .expect("begin journal");
        let incoming = storage.incoming_operation_directory(operation_id);
        fs::create_dir_all(&incoming).expect("incoming dir");
        fs::rename(&canonical, incoming.join("opaque-plaintext-file")).expect("stage plaintext");
        let connection = storage.connection().expect("connection");
        connection
            .execute(
                "UPDATE vault_operations SET phase = ?1 WHERE note_id = ?2",
                params![VAULT_PHASE_DATABASE_COMMITTED, &note.id],
            )
            .expect("mark committed");
        drop(connection);
        assert!(!canonical.exists());

        let data_directory = storage.data_directory.clone();
        let backup_directory = storage.automatic_backup_directory.clone();
        drop(storage);
        let _storage = Storage::at_with_backup(data_directory, backup_directory)
            .expect("restart library and recover committed unprotect");
        assert_eq!(fs::read(&canonical).expect("published blob"), plaintext);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_recovery_finishes_protection_after_database_commit() {
        let (storage, root) = test_storage("vault-recover-protect-committed");
        let mut note = storage.create_note(None).expect("create note");
        note.title = "protected recovery marker".to_string();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save note")
            .0;
        let source = root.join("recover-protect-committed.txt");
        fs::write(&source, b"protected recovery attachment").expect("write source");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "recover-protect-committed.txt",
                Some("text/plain"),
                &source,
            )
            .expect("add attachment");

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        // Recreate the durable state that exists after the canonical protection
        // transaction commits but before filesystem/privacy finalization returns.
        let operation_id = "vault_test_protect_committed";
        let ciphertext_hashes = {
            let connection = storage.connection().expect("connection");
            let mut statement = connection
                .prepare("SELECT sha256 FROM vault_blob_refs WHERE note_id = ?1")
                .expect("prepare protected hashes");
            let rows = statement
                .query_map([&note.id], |row| row.get::<_, String>(0))
                .expect("query protected hashes");
            rows.collect::<Result<Vec<_>, _>>()
                .expect("collect protected hashes")
        };
        let details = VaultOperationDetails { ciphertext_hashes };
        storage
            .begin_vault_operation(&note.id, VAULT_OPERATION_PROTECT, operation_id, &details)
            .expect("begin journal");
        {
            let connection = storage.connection().expect("connection");
            connection
                .execute(
                    "UPDATE vault_operations SET phase = ?1 WHERE note_id = ?2",
                    params![VAULT_PHASE_DATABASE_COMMITTED, &note.id],
                )
                .expect("mark committed");
        }
        let retiring = storage.retiring_operation_directory(operation_id);
        fs::create_dir_all(&retiring).expect("retiring directory");
        fs::write(retiring.join("stale-retired-plaintext"), b"stale").expect("retiring residue");

        let rendition = storage.data_directory.join(
            crate::attachment_paths::image_rendition_cache_relative_directory(&attachment.sha256)
                .expect("rendition path"),
        );
        fs::create_dir_all(&rendition).expect("rendition directory");
        fs::write(rendition.join("stale-preview.bin"), b"stale preview").expect("preview residue");

        let data_directory = storage.data_directory.clone();
        let backup_directory = storage.automatic_backup_directory.clone();
        drop(storage);
        let storage = Storage::at_with_backup(data_directory, backup_directory)
            .expect("restart library and recover committed protect");

        assert!(
            !retiring.exists(),
            "retired plaintext staging must be removed"
        );
        assert!(
            !rendition.exists(),
            "restart recovery must scrub managed plaintext derivatives"
        );
        let connection = storage.connection().expect("connection");
        let operations: i64 = connection
            .query_row("SELECT COUNT(*) FROM vault_operations", [], |row| {
                row.get(0)
            })
            .expect("operation count");
        assert_eq!(operations, 0);
        assert!(
            protected_note_exists(&connection, &note.id).expect("protected state"),
            "committed protection must remain canonical after restart recovery"
        );
        drop(connection);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_recovery_rolls_back_prepared_unprotect_without_publishing_plaintext() {
        let (storage, root) = test_storage("vault-recover-unprotect-prepared");
        let note = storage.create_note(None).expect("create note");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let operation_id = "vault_test_unprotect_prepared";
        storage
            .begin_vault_operation(
                &note.id,
                VAULT_OPERATION_UNPROTECT,
                operation_id,
                &VaultOperationDetails {
                    ciphertext_hashes: Vec::new(),
                },
            )
            .expect("begin journal");
        let incoming = storage.incoming_operation_directory(operation_id);
        fs::create_dir_all(&incoming).expect("incoming directory");
        fs::write(incoming.join("staged-plaintext"), b"must not publish").expect("stage plaintext");

        let data_directory = storage.data_directory.clone();
        let backup_directory = storage.automatic_backup_directory.clone();
        drop(storage);
        let storage = Storage::at_with_backup(data_directory, backup_directory)
            .expect("restart library and recover prepared unprotect");

        assert!(
            !incoming.exists(),
            "prepared plaintext staging must be discarded"
        );
        let connection = storage.connection().expect("connection");
        let operations: i64 = connection
            .query_row("SELECT COUNT(*) FROM vault_operations", [], |row| {
                row.get(0)
            })
            .expect("operation count");
        assert_eq!(operations, 0);
        assert!(
            protected_note_exists(&connection, &note.id).expect("protected state"),
            "a crash before the unprotect database commit must leave the note protected"
        );
        drop(connection);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protect_waits_for_active_derivative_then_scrubs_cache_and_stage() {
        let (storage, root) = test_storage("vault-derivative-race");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("derivative-race.pdf");
        fs::write(&source, b"%PDF-1.4\nplaintext derivative race\n%%EOF\n").expect("write source");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "derivative-race.pdf",
                Some("application/pdf"),
                &source,
            )
            .expect("add attachment");

        let permit = storage
            .privacy_coordinator
            .acquire_plaintext_derivative(&note.id)
            .expect("derivative permit");
        let storage = Arc::new(storage);
        let storage_for_protect = Arc::clone(&storage);
        let note_for_protect = note.id.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let protect = thread::spawn(move || {
            let result = storage_for_protect
                .protect_note_with_password(&note_for_protect, b"correct horse battery staple");
            done_tx.send(result).expect("protect result");
        });

        assert!(
            done_rx.recv_timeout(Duration::from_millis(30)).is_err(),
            "Encrypt must wait for an already-authorized plaintext producer"
        );

        let rendition = storage.data_directory.join(
            crate::attachment_paths::image_rendition_cache_relative_directory(&attachment.sha256)
                .expect("rendition path"),
        );
        fs::create_dir_all(&rendition).expect("rendition directory");
        fs::write(
            rendition.join("late-preview.bin"),
            b"late plaintext preview",
        )
        .expect("late preview residue");
        let stage_token = crate::native::staging::stage_pdf_export(
            &attachment.id,
            b"%PDF-1.4\nlate staged plaintext\n%%EOF\n",
        )
        .expect("stage PDF derivative");

        drop(permit);
        let protected = done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("protection completion")
            .expect("protection succeeds");
        assert!(protected.protection.is_protected());
        protect.join().expect("protect thread");
        assert!(
            !rendition.exists(),
            "late cache derivative must be scrubbed before Encrypt returns"
        );
        assert!(
            crate::native::staging::save_staged_pdf_export(
                &stage_token,
                &attachment.id,
                &root.join("must-not-save.pdf"),
            )
            .is_err(),
            "late staged derivative must be invalidated before Encrypt returns"
        );

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protection_cleanup_failure_stays_journaled_until_recovery_succeeds() {
        let (storage, root) = test_storage("vault-cleanup-failure-recovery");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("cleanup-failure.txt");
        fs::write(&source, b"cleanup failure plaintext").expect("write source");
        storage
            .add_attachment_file_as(&note.id, "cleanup-failure.txt", Some("text/plain"), &source)
            .expect("add attachment");

        // Deterministically break one Rustnote-managed cleanup namespace on every
        // platform. A regular file where the media-cache directory must be makes
        // `read_dir` fail regardless of process privileges (unlike chmod-based
        // tests, which can silently pass when the test runner is root).
        let media_cache = crate::library_paths::media_compat_directory(&storage.data_directory);
        if media_cache.exists() {
            crate::native::privacy::remove_tree(&media_cache)
                .expect("remove media cache directory");
        }
        if let Some(parent) = media_cache.parent() {
            fs::create_dir_all(parent).expect("media cache parent");
        }
        fs::write(&media_cache, b"not a directory").expect("install deterministic cleanup fault");

        let result = storage.protect_note_with_password(&note.id, b"correct horse battery staple");
        assert!(
            result.is_err(),
            "Encrypt must report failure when managed plaintext cleanup cannot complete"
        );

        let connection = storage.connection().expect("connection");
        assert!(
            protected_note_exists(&connection, &note.id).expect("protected state"),
            "canonical encryption may already be committed when final cleanup fails"
        );
        let pending: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM vault_operations WHERE note_id = ?1 AND phase = ?2",
                params![&note.id, VAULT_PHASE_DATABASE_COMMITTED],
                |row| row.get(0),
            )
            .expect("pending finalization count");
        assert_eq!(
            pending, 1,
            "failed cleanup must retain durable recovery state"
        );
        drop(connection);

        fs::remove_file(&media_cache).expect("repair media cache path");
        let data_directory = storage.data_directory.clone();
        let backup_directory = storage.automatic_backup_directory.clone();
        drop(storage);
        let storage = Storage::at_with_backup(data_directory, backup_directory)
            .expect("restart after filesystem cleanup becomes possible");
        let connection = storage.connection().expect("connection");
        let pending: i64 = connection
            .query_row("SELECT COUNT(*) FROM vault_operations", [], |row| {
                row.get(0)
            })
            .expect("pending operation count");
        assert_eq!(pending, 0);
        drop(connection);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protected_unprotected_protected_cycle_preserves_granular_state_and_residue_rules() {
        let (storage, root) = test_storage("vault-repeat-cycle");
        let marker = "repeat-cycle-secret";
        let mut note = storage.create_note(None).expect("create note");
        note.title = marker.to_string();
        let note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save checkpoint")
            .0;
        let source = root.join("repeat-cycle.txt");
        fs::write(&source, marker.as_bytes()).expect("write source");
        let attachment = storage
            .add_attachment_file_as(&note.id, "repeat-cycle.txt", Some("text/plain"), &source)
            .expect("add attachment");

        for cycle in 0..2 {
            storage
                .protect_note_with_password(&note.id, b"correct horse battery staple")
                .unwrap_or_else(|error| panic!("protect cycle {cycle}: {error}"));
            storage
                .lock_protected_note(&note.id)
                .unwrap_or_else(|error| panic!("lock cycle {cycle}: {error}"));
            assert!(
                !contains_marker(&database_and_wal_bytes(&storage), marker),
                "protected cycle {cycle} must not leave plaintext in SQLite/WAL"
            );
            storage
                .unlock_protected_note(&note.id, b"correct horse battery staple")
                .unwrap_or_else(|error| panic!("unlock cycle {cycle}: {error}"));
            let restored = storage
                .remove_note_protection_with_password(&note.id, b"correct horse battery staple")
                .unwrap_or_else(|error| panic!("unprotect cycle {cycle}: {error}"));
            assert_eq!(restored.title, marker);
            let restored_attachment = storage
                .list_attachments(&note.id)
                .expect("list attachments")
                .into_iter()
                .find(|candidate| candidate.id == attachment.id)
                .expect("restored attachment");
            assert_eq!(
                fs::read(
                    storage
                        .attachment_file_path(&restored_attachment)
                        .expect("attachment path")
                )
                .expect("restored attachment bytes"),
                marker.as_bytes()
            );
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pending_attachment_mutations_block_encrypt_until_their_durable_state_is_resolved() {
        let (storage, root) = test_storage("vault-attachment-races");
        let note = storage.create_note(None).expect("create note");
        let first_source = root.join("first.txt");
        fs::write(&first_source, b"first attachment").expect("write first source");
        let first = storage
            .add_attachment_file_as(&note.id, "first.txt", Some("text/plain"), &first_source)
            .expect("add first attachment");

        let pending_source = root.join("pending.txt");
        fs::write(&pending_source, b"pending attachment").expect("write pending source");
        let prepared = storage
            .prepare_attachment_import_profiled(&note.id, &pending_source)
            .expect("prepare pending ingest");
        assert!(
            storage
                .protect_note_with_password(&note.id, b"correct horse battery staple")
                .is_err(),
            "Encrypt must not cross a durable pending attachment ingest"
        );
        storage
            .finalize_prepared_attachment_import_profiled(prepared)
            .expect("finalize pending ingest");

        let (remove_operation, removed_attachment) = storage
            .prepare_attachment_remove(&first.id)
            .expect("prepare attachment removal")
            .expect("removal operation");
        assert!(
            storage
                .protect_note_with_password(&note.id, b"correct horse battery staple")
                .is_err(),
            "Encrypt must not cross a durable attachment removal operation"
        );
        storage
            .finish_attachment_remove(&remove_operation, &removed_attachment)
            .expect("finish removal");

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("Encrypt succeeds after attachment operations settle");
        assert!(
            storage.remove_attachment(&first.id).is_ok(),
            "already-removed attachment remains idempotent"
        );
        let before_add = current_vault_graph_ids(&storage, &note.id);
        let protected_attachment = storage
            .add_attachment_file_as(
                &note.id,
                "protected-added.txt",
                Some("text/plain"),
                &pending_source,
            )
            .expect("unlocked protected note accepts encrypted attachment ingestion");
        let after_add = current_vault_graph_ids(&storage, &note.id);
        assert_ne!(after_add.manifest, before_add.manifest);
        assert_eq!(after_add.private_state, before_add.private_state);
        assert_eq!(after_add.metadata_state, before_add.metadata_state);
        assert_ne!(after_add.attachment_index, before_add.attachment_index);
        assert_eq!(after_add.revision_head, before_add.revision_head);
        assert_eq!(after_add.revision_count, before_add.revision_count);
        let connection = storage.connection().expect("connection");
        let stub_name: String = connection
            .query_row(
                "SELECT file_name FROM attachments WHERE id = ?1",
                [&protected_attachment.id],
                |row| row.get(0),
            )
            .expect("protected attachment stub");
        assert_eq!(stub_name, LOCKED_ATTACHMENT_NAME);
        drop(connection);

        storage
            .remove_attachment(&protected_attachment.id)
            .expect("remove protected attachment");
        let after_remove = current_vault_graph_ids(&storage, &note.id);
        assert_ne!(after_remove.manifest, after_add.manifest);
        assert_eq!(after_remove.private_state, after_add.private_state);
        assert_eq!(after_remove.metadata_state, after_add.metadata_state);
        assert_ne!(after_remove.attachment_index, after_add.attachment_index);
        assert_eq!(after_remove.revision_head, after_add.revision_head);
        assert_eq!(after_remove.revision_count, after_add.revision_count);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn revision_only_plaintext_is_removed_from_sqlite_and_wal_on_protection() {
        let (storage, root) = test_storage("vault-revision-forensic-residue");
        let marker = format!(
            "RUSTNOTE-E3-REVISION-ONLY-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let mut note = storage.create_note(None).expect("create note");
        note.title = marker.clone();
        let mut note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save secret revision")
            .0;
        note.title = "current non-secret title".to_string();
        note = storage
            .save_note_owned_with_revision_status_for_test(
                note,
                super::super::RevisionMode::Checkpoint,
            )
            .expect("save current revision")
            .0;
        assert_ne!(note.title, marker);
        assert!(
            contains_marker(&database_and_wal_bytes(&storage), &marker),
            "the historical revision marker must exist before protection"
        );

        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        assert!(
            !contains_marker(&database_and_wal_bytes(&storage), &marker),
            "revision-only plaintext must not survive protection in SQLite/WAL"
        );

        storage
            .remove_note_protection_with_password(&note.id, b"correct horse battery staple")
            .expect("restore note");
        let connection = storage.connection().expect("connection");
        let revision_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_revisions WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("revision count");
        assert!(
            revision_rows >= 2,
            "revision history must round-trip through Vault"
        );
        drop(connection);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_backup_reset_never_deletes_user_selected_parent_contents() {
        let (storage, root) = test_storage("vault-backup-parent-sentinel");
        let sentinel = storage
            .automatic_backup_directory
            .join("unrelated-user-file.txt");
        fs::write(&sentinel, b"must survive").expect("write sentinel");

        storage
            .reset_backup_repository_for_vault_unlocked()
            .expect("reset Rustnote child repository");

        assert_eq!(
            fs::read(&sentinel).expect("sentinel survives"),
            b"must survive"
        );
        assert!(
            storage
                .automatic_backup_directory
                .join("Rustnote Repository")
                .join("repository.json")
                .exists(),
            "Rustnote should recreate only its dedicated child repository"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vault_backup_reset_does_not_treat_root_level_legacy_layout_as_owned() {
        let (storage, root) = test_storage("vault-backup-no-root-legacy-compat");
        let parent = &storage.automatic_backup_directory;
        let unrelated = parent.join("family-photo.txt");
        let legacy_like_snapshot = parent.join("snapshots").join("old.sqlite3");
        fs::write(&unrelated, b"unrelated user data").expect("write unrelated sentinel");
        fs::create_dir_all(legacy_like_snapshot.parent().expect("snapshot parent"))
            .expect("create root-level lookalike");
        fs::write(
            &legacy_like_snapshot,
            b"not owned by the current repository authority",
        )
        .expect("write root-level lookalike");
        fs::write(
            parent.join("repository.json"),
            br#"{"format_version":1,"last_attempt_at":null,"last_result":"obsolete","snapshots":[]}"#,
        )
        .expect("write obsolete root-level metadata");

        storage
            .reset_backup_repository_for_vault_unlocked()
            .expect("reset dedicated child repository");

        assert_eq!(
            fs::read(&unrelated).expect("unrelated data survives"),
            b"unrelated user data"
        );
        assert_eq!(
            fs::read(&legacy_like_snapshot).expect("root-level lookalike survives"),
            b"not owned by the current repository authority"
        );
        assert!(parent.join("repository.json").exists());
        assert!(
            parent
                .join("Rustnote Repository")
                .join("repository.json")
                .exists(),
            "only the externally-owned dedicated child is reset",
        );
        assert!(
            parent.join(".rustnote-backup-repository-owner").is_file(),
            "external ownership authority must survive child reset",
        );
        let _ = fs::remove_dir_all(root);
    }
}

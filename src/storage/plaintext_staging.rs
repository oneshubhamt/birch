//! Durable ownership for Rustnote-managed plaintext files inside the library.
//!
//! Security invariant (2a.16): a library-resident plaintext staging file is
//! never created before an ownership row exists, and that row is never removed
//! until either (a) deletion of the plaintext has succeeded or (b) ownership is
//! transferred atomically, in the same SQLite transaction, to a more specific
//! durable ingest/operation record or canonical database reference.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::domain::NoteId;

use super::{Storage, StorageError, StorageResult, new_id, unix_timestamp};

pub(super) const PLAINTEXT_STAGE_ATTACHMENT_INCOMING: &str = "attachment_incoming";
pub(super) const PLAINTEXT_STAGE_PORTABILITY_IMPORT: &str = "portability_import";
pub(super) const PLAINTEXT_STAGE_PORTABILITY_BLOB: &str = "portability_blob";

#[derive(Clone, Debug)]
struct ManagedPlaintextStage {
    id: String,
    kind: String,
    note_id: Option<NoteId>,
    relative_path: String,
    transfer_destination: Option<String>,
    transfer_state: Option<String>,
}

impl Storage {
    pub(super) fn reserve_attachment_owned_plaintext(
        &self,
        note_id: &NoteId,
        staging_relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        let stage_id = self.reserve_attachment_plaintext_stage(note_id, staging_relative_path)?;
        let path = self.managed_plaintext_stage_path(
            PLAINTEXT_STAGE_ATTACHMENT_INCOMING,
            staging_relative_path,
        )?;
        Ok(crate::security::OwnedPlaintext::durable_managed(
            path,
            stage_id,
            PLAINTEXT_STAGE_ATTACHMENT_INCOMING.to_string(),
            Some(note_id.clone()),
            staging_relative_path.to_string(),
        ))
    }

    pub(super) fn reserve_portability_owned_plaintext(
        &self,
        relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        let stage_id = self.reserve_portability_plaintext_stage(relative_path)?;
        let path =
            self.managed_plaintext_stage_path(PLAINTEXT_STAGE_PORTABILITY_IMPORT, relative_path)?;
        Ok(crate::security::OwnedPlaintext::durable_managed(
            path,
            stage_id,
            PLAINTEXT_STAGE_PORTABILITY_IMPORT.to_string(),
            None,
            relative_path.to_string(),
        ))
    }

    pub(super) fn reserve_canonical_blob_owned_plaintext(
        &self,
        relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        let stage_id = self.reserve_canonical_blob_plaintext_stage(relative_path)?;
        let path =
            self.managed_plaintext_stage_path(PLAINTEXT_STAGE_PORTABILITY_BLOB, relative_path)?;
        Ok(crate::security::OwnedPlaintext::durable_managed(
            path,
            stage_id,
            PLAINTEXT_STAGE_PORTABILITY_BLOB.to_string(),
            None,
            relative_path.to_string(),
        ))
    }

    pub(super) fn reserve_portability_blob_owned_plaintext(
        &self,
        relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        let stage_id = self.reserve_portability_blob_plaintext_stage(relative_path)?;
        let path =
            self.managed_plaintext_stage_path(PLAINTEXT_STAGE_PORTABILITY_BLOB, relative_path)?;
        Ok(crate::security::OwnedPlaintext::durable_managed(
            path,
            stage_id,
            PLAINTEXT_STAGE_PORTABILITY_BLOB.to_string(),
            None,
            relative_path.to_string(),
        ))
    }

    pub(super) fn resume_owned_plaintext(
        &self,
        stage_id: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        let stage = self.get_managed_plaintext_stage(stage_id)?.ok_or_else(|| {
            StorageError::Integrity("durable plaintext ownership disappeared".to_string())
        })?;
        if stage.transfer_state.is_some() || stage.transfer_destination.is_some() {
            return Err(StorageError::Integrity(
                "prepared plaintext transfer must be reconciled before generic cleanup".to_string(),
            ));
        }
        let path = self.managed_plaintext_stage_path(&stage.kind, &stage.relative_path)?;
        Ok(crate::security::OwnedPlaintext::durable_managed(
            path,
            stage.id,
            stage.kind,
            stage.note_id,
            stage.relative_path,
        ))
    }

    pub(super) fn resume_portability_blob_owned_plaintext(
        &self,
        stage_id: &str,
        relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        validate_library_blob_plaintext_relative_path(relative_path)?;
        let stage = self.get_managed_plaintext_stage(stage_id)?.ok_or_else(|| {
            StorageError::Integrity("durable library blob ownership disappeared".to_string())
        })?;
        if stage.kind != PLAINTEXT_STAGE_PORTABILITY_BLOB
            || stage.note_id.is_some()
            || stage.relative_path != relative_path
        {
            return Err(StorageError::Integrity(
                "durable library blob ownership does not match its expected scope".to_string(),
            ));
        }
        let path = self.managed_plaintext_stage_path(&stage.kind, &stage.relative_path)?;
        Ok(crate::security::OwnedPlaintext::durable_managed(
            path,
            stage.id,
            stage.kind,
            None,
            stage.relative_path,
        ))
    }

    /// Resume a canonical attachment blob that must already have the generic
    /// durable library-blob owner. Steady-state quarantine is deliberately
    /// unable to manufacture ownership after plaintext exists; every normal
    /// database reference-removal path must establish this owner before commit.
    pub(super) fn require_canonical_blob_owned_plaintext(
        &self,
        relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        validate_portability_blob_relative_path(relative_path)?;
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                "SELECT id, kind, note_id, transfer_destination, transfer_state
                 FROM managed_plaintext_staging WHERE relative_path = ?1",
                [relative_path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<NoteId>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((stage_id, kind, note_id, transfer_destination, transfer_state)) = existing else {
            return Err(StorageError::Integrity(
                "canonical attachment plaintext has no durable cleanup owner".to_string(),
            ));
        };
        if kind != PLAINTEXT_STAGE_PORTABILITY_BLOB
            || note_id.is_some()
            || transfer_destination.is_some()
            || transfer_state.is_some()
        {
            return Err(StorageError::Integrity(
                "canonical attachment blob has conflicting plaintext ownership".to_string(),
            ));
        }
        Ok(crate::security::OwnedPlaintext::durable_managed_existing(
            self.attachments_directory.join(relative_path),
            stage_id,
            PLAINTEXT_STAGE_PORTABILITY_BLOB.to_string(),
            None,
            relative_path.to_string(),
        ))
    }

    /// Reserve the quarantine successor before a canonical plaintext blob is
    /// renamed. The durable row is the cleanup authority for the quarantined
    /// file across process death and remains until explicit purge/protection
    /// cleanup removes the file.
    pub(super) fn reserve_orphan_blob_owned_plaintext(
        &self,
        relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        validate_orphan_blob_relative_path(relative_path)?;
        let stage_id = self.reserve_managed_plaintext_stage(
            PLAINTEXT_STAGE_PORTABILITY_BLOB,
            None,
            relative_path,
        )?;
        Ok(crate::security::OwnedPlaintext::durable_managed(
            self.attachments_directory.join(relative_path),
            stage_id,
            PLAINTEXT_STAGE_PORTABILITY_BLOB.to_string(),
            None,
            relative_path.to_string(),
        ))
    }

    /// Resume a quarantine file that must already have a durable owner. Normal
    /// cleanup is deliberately unable to manufacture ownership after plaintext
    /// exists; missing ownership is an integrity failure.
    pub(super) fn require_orphan_blob_owned_plaintext(
        &self,
        relative_path: &str,
    ) -> StorageResult<crate::security::OwnedPlaintext> {
        validate_orphan_blob_relative_path(relative_path)?;
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                "SELECT id, kind, note_id FROM managed_plaintext_staging WHERE relative_path = ?1",
                [relative_path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<NoteId>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((stage_id, kind, note_id)) = existing else {
            return Err(StorageError::Integrity(
                "quarantined plaintext has no durable cleanup owner".to_string(),
            ));
        };
        if kind != PLAINTEXT_STAGE_PORTABILITY_BLOB || note_id.is_some() {
            return Err(StorageError::Integrity(
                "orphan blob has conflicting plaintext ownership".to_string(),
            ));
        }
        Ok(crate::security::OwnedPlaintext::durable_managed_existing(
            self.attachments_directory.join(relative_path),
            stage_id,
            PLAINTEXT_STAGE_PORTABILITY_BLOB.to_string(),
            None,
            relative_path.to_string(),
        ))
    }

    /// Verify that every Rustnote-managed attachment plaintext path visible at
    /// startup already has an authoritative owner. This is deliberately a
    /// verifier, not an adoption path: recovery may reconcile ownership that was
    /// durably recorded before a crash, but it may never manufacture ownership
    /// after discovering plaintext on disk.
    pub(super) fn verify_managed_attachment_plaintext_ownership_unlocked(
        &self,
    ) -> StorageResult<()> {
        let quarantine = self.attachments_directory.join(".orphaned");
        match fs::read_dir(&quarantine) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let metadata = fs::symlink_metadata(entry.path())?;
                    if !metadata.is_file() || metadata.file_type().is_symlink() {
                        return Err(StorageError::Integrity(format!(
                            "attachment quarantine contains an unexpected non-file entry: {}",
                            entry.path().display()
                        )));
                    }
                    let relative_path = entry
                        .path()
                        .strip_prefix(&self.attachments_directory)
                        .map_err(|_| {
                            StorageError::Integrity(
                                "quarantined attachment escaped library ownership".to_string(),
                            )
                        })?
                        .to_str()
                        .ok_or_else(|| {
                            StorageError::Integrity(
                                "quarantined attachment path is not UTF-8".to_string(),
                            )
                        })?
                        .replace('\\', "/");
                    validate_orphan_blob_relative_path(&relative_path)?;
                    let existing: Option<(String, Option<NoteId>)> = self
                        .connection()?
                        .query_row(
                            "SELECT kind, note_id FROM managed_plaintext_staging
                             WHERE relative_path = ?1",
                            [&relative_path],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()?;
                    match existing {
                        Some((kind, note_id))
                            if kind == PLAINTEXT_STAGE_PORTABILITY_BLOB && note_id.is_none() => {}
                        Some(_) => {
                            return Err(StorageError::Integrity(
                                "orphan blob has conflicting plaintext ownership".to_string(),
                            ));
                        }
                        None => {
                            return Err(StorageError::Integrity(
                                "quarantined plaintext exists without prior durable ownership"
                                    .to_string(),
                            ));
                        }
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        let mut pending_add_hashes = std::collections::HashSet::new();
        {
            let connection = self.connection()?;
            let mut statement = connection.prepare(
                "SELECT attachment_json FROM attachment_operations WHERE operation = 'add'",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            for attachment_json in rows {
                let attachment: crate::domain::Attachment = serde_json::from_str(&attachment_json?)
                    .map_err(|error| {
                        StorageError::Integrity(format!(
                            "pending attachment Add ownership record is corrupt: {error}"
                        ))
                    })?;
                pending_add_hashes.insert(attachment.sha256);
            }
        }

        let blobs_root = self.attachments_directory.join("blobs");
        let prefixes = match fs::read_dir(&blobs_root) {
            Ok(prefixes) => prefixes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for prefix in prefixes {
            let prefix = prefix?;
            if !prefix.file_type()?.is_dir() {
                return Err(StorageError::Integrity(format!(
                    "attachment blob root contains an unexpected non-directory entry: {}",
                    prefix.path().display()
                )));
            }
            for blob in fs::read_dir(prefix.path())? {
                let blob = blob?;
                let metadata = fs::symlink_metadata(blob.path())?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(StorageError::Integrity(format!(
                        "attachment blob path is not a regular file: {}",
                        blob.path().display()
                    )));
                }
                let sha256 = blob
                    .file_name()
                    .to_str()
                    .ok_or_else(|| {
                        StorageError::Integrity(
                            "canonical attachment blob identity is not UTF-8".to_string(),
                        )
                    })?
                    .to_string();
                let relative_path = crate::attachment_paths::canonical_blob_relative_path(&sha256)
                    .ok_or_else(|| {
                        StorageError::Integrity(
                            "invalid canonical attachment blob identity".to_string(),
                        )
                    })?;
                let actual_relative = blob
                    .path()
                    .strip_prefix(&self.attachments_directory)
                    .map_err(|_| {
                        StorageError::Integrity(
                            "canonical attachment blob escaped library ownership".to_string(),
                        )
                    })?
                    .to_str()
                    .ok_or_else(|| {
                        StorageError::Integrity(
                            "canonical attachment blob path is not UTF-8".to_string(),
                        )
                    })?
                    .replace('\\', "/");
                if actual_relative != relative_path {
                    return Err(StorageError::Integrity(
                        "canonical attachment blob path does not match its content identity"
                            .to_string(),
                    ));
                }

                let connection = self.connection()?;
                let references: i64 = connection.query_row(
                    "SELECT
                        (SELECT COUNT(*) FROM attachments WHERE sha256 = ?1)
                        + (SELECT COUNT(*) FROM revision_attachment_refs WHERE sha256 = ?1)
                        + (SELECT COUNT(*) FROM vault_blob_refs WHERE sha256 = ?1)",
                    [&sha256],
                    |row| row.get(0),
                )?;
                let existing: Option<(String, Option<NoteId>)> = connection
                    .query_row(
                        "SELECT kind, note_id FROM managed_plaintext_staging
                         WHERE relative_path = ?1",
                        [&relative_path],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                drop(connection);

                let specialized_owner = references != 0 || pending_add_hashes.contains(&sha256);
                match (specialized_owner, existing) {
                    (true, None) => {}
                    (true, Some(_)) => {
                        return Err(StorageError::Integrity(
                            "canonical attachment blob has conflicting durable owners".to_string(),
                        ));
                    }
                    (false, Some((kind, note_id)))
                        if kind == PLAINTEXT_STAGE_PORTABILITY_BLOB && note_id.is_none() => {}
                    (false, Some(_)) => {
                        return Err(StorageError::Integrity(
                            "canonical attachment blob has conflicting plaintext ownership"
                                .to_string(),
                        ));
                    }
                    (false, None) => {
                        return Err(StorageError::Integrity(
                            "canonical attachment plaintext exists without prior durable ownership"
                                .to_string(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn discard_owned_plaintext(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
    ) -> StorageResult<()> {
        let owner = owned.durable_ownership().ok_or_else(|| {
            StorageError::Integrity(
                "plaintext is not owned by the durable staging ledger".to_string(),
            )
        })?;
        let stage_id = owner.stage_id.clone();
        let expected_path = self.managed_plaintext_stage_path(&owner.kind, &owner.relative_path)?;
        if expected_path != owned.path() {
            return Err(StorageError::Integrity(
                "durable plaintext owner path does not match its ledger namespace".to_string(),
            ));
        }
        let stage = self
            .get_managed_plaintext_stage(&stage_id)?
            .ok_or_else(|| {
                StorageError::Integrity(
                    "durable plaintext ownership disappeared before retirement".to_string(),
                )
            })?;
        validate_managed_plaintext_transfer_record(&stage)?;
        if stage.kind != owner.kind
            || stage.note_id.as_ref() != owner.note_id.as_ref()
            || stage.relative_path != owner.relative_path
        {
            return Err(StorageError::Integrity(
                "durable plaintext ownership no longer matches its typed owner".to_string(),
            ));
        }
        if self.managed_plaintext_stage_is_required_transfer_successor(&stage)? {
            return Err(StorageError::Integrity(
                "cannot retire plaintext successor ownership while a prepared transfer depends on it"
                    .to_string(),
            ));
        }
        if stage.transfer_state.is_some()
            && !owned.path_entry_exists()?
            && !self.managed_plaintext_transfer_successor_is_owned(&stage)?
        {
            return Err(StorageError::Integrity(
                "cannot retire transferred plaintext predecessor after successor ownership disappeared"
                    .to_string(),
            ));
        }

        owned.remove_file()?;
        let removed = self.connection()?.execute(
            "DELETE FROM managed_plaintext_staging WHERE id = ?1",
            [&stage_id],
        )?;
        if removed != 1 {
            return Err(StorageError::Integrity(
                "durable plaintext ownership disappeared before retirement".to_string(),
            ));
        }
        owned
            .retire_durable_owner_after_cleanup()
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    pub(super) fn cleanup_owned_plaintext_after_error(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
        primary: StorageError,
    ) -> StorageError {
        match self.discard_owned_plaintext(owned) {
            Ok(()) => primary,
            Err(cleanup) => StorageError::Integrity(format!(
                "{primary}; plaintext cleanup is incomplete and remains durably owned: {cleanup}"
            )),
        }
    }

    pub(super) fn release_uncreated_owned_plaintext(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
    ) -> StorageResult<()> {
        let owner = owned.durable_ownership().ok_or_else(|| {
            StorageError::Integrity(
                "plaintext is not owned by the durable staging ledger".to_string(),
            )
        })?;
        let stage_id = owner.stage_id.clone();
        let expected_path = self.managed_plaintext_stage_path(&owner.kind, &owner.relative_path)?;
        if expected_path != owned.path() {
            return Err(StorageError::Integrity(
                "cannot release uncreated plaintext ownership for a mismatched path".to_string(),
            ));
        }

        // Keep the durable owner row uncommitted until absence has been proved
        // conclusively and the typed owner has accepted retirement. Any metadata
        // error rolls the transaction back, preserving crash-recovery ownership.
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        owned.ensure_path_absent()?;
        let removed = transaction.execute(
            "DELETE FROM managed_plaintext_staging WHERE id = ?1",
            [&stage_id],
        )?;
        if removed != 1 {
            return Err(StorageError::Integrity(
                "uncreated plaintext staging reservation disappeared".to_string(),
            ));
        }
        owned.mark_uncreated_durable_owner_retired()?;
        transaction.commit()?;
        Ok(())
    }

    pub(super) fn transfer_owned_plaintext_stage(
        &self,
        transaction: &Transaction<'_>,
        owned: &mut crate::security::OwnedPlaintext,
    ) -> StorageResult<()> {
        let owner = owned.durable_ownership().ok_or_else(|| {
            StorageError::Integrity(
                "plaintext is not owned by the durable staging ledger".to_string(),
            )
        })?;
        let stage_id = owner.stage_id.clone();
        let kind = owner.kind.clone();
        let note_id = owner.note_id.clone();
        let relative_path = owner.relative_path.clone();
        self.transfer_managed_plaintext_stage(
            transaction,
            &stage_id,
            &kind,
            note_id.as_ref(),
            &relative_path,
        )
    }

    pub(super) fn complete_owned_plaintext_transfer(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
    ) -> StorageResult<()> {
        owned
            .mark_transferred()
            .map_err(|message| StorageError::Integrity(message.to_string()))
    }

    fn reserve_attachment_plaintext_stage(
        &self,
        note_id: &NoteId,
        staging_relative_path: &str,
    ) -> StorageResult<String> {
        super::attachments::validate_staging_relative_path(staging_relative_path)?;
        self.reserve_managed_plaintext_stage(
            PLAINTEXT_STAGE_ATTACHMENT_INCOMING,
            Some(note_id),
            staging_relative_path,
        )
    }

    fn reserve_portability_plaintext_stage(&self, relative_path: &str) -> StorageResult<String> {
        validate_portability_stage_relative_path(relative_path)?;
        self.reserve_managed_plaintext_stage(
            PLAINTEXT_STAGE_PORTABILITY_IMPORT,
            None,
            relative_path,
        )
    }

    /// Reserve durable ownership for a plaintext canonical attachment blob
    /// before a workflow creates or replaces that path. The persisted kind name
    /// predates its use by archive restore, but the ownership semantics are
    /// intentionally workflow-neutral.
    fn reserve_canonical_blob_plaintext_stage(&self, relative_path: &str) -> StorageResult<String> {
        validate_portability_blob_relative_path(relative_path)?;
        self.reserve_managed_plaintext_stage(PLAINTEXT_STAGE_PORTABILITY_BLOB, None, relative_path)
    }

    fn reserve_portability_blob_plaintext_stage(
        &self,
        relative_path: &str,
    ) -> StorageResult<String> {
        self.reserve_canonical_blob_plaintext_stage(relative_path)
    }

    /// Reserve a same-filesystem temporary path used while replacing a
    /// canonical attachment blob during whole-library restore. The durable
    /// owner exists before the first byte is copied. After an atomic rename to
    /// the canonical path, the temporary owner can be released because the
    /// canonical database/staging owner already exists.
    pub(super) fn reserve_restore_blob_temporary_owned_plaintext(
        &self,
        sha256: &str,
    ) -> StorageResult<(String, crate::security::OwnedPlaintext)> {
        let canonical = crate::attachment_paths::canonical_blob_relative_path(sha256)
            .ok_or_else(|| StorageError::Integrity("invalid restore blob hash".to_string()))?;
        let parent = Path::new(&canonical).parent().ok_or_else(|| {
            StorageError::Integrity("restore blob path has no parent".to_string())
        })?;
        let file_name = format!(".{sha256}.restore-{}.tmp", new_id("blob"));
        let relative_path = parent.join(file_name);
        let relative_path = relative_path
            .to_str()
            .ok_or_else(|| {
                StorageError::Integrity("restore blob temporary path is not UTF-8".to_string())
            })?
            .to_string();
        validate_restore_blob_temporary_relative_path(&relative_path)?;
        let stage_id = self.reserve_managed_plaintext_stage(
            PLAINTEXT_STAGE_PORTABILITY_BLOB,
            None,
            &relative_path,
        )?;
        let path = self.attachments_directory.join(&relative_path);
        let owned = crate::security::OwnedPlaintext::durable_managed(
            path,
            stage_id,
            PLAINTEXT_STAGE_PORTABILITY_BLOB.to_string(),
            None,
            relative_path.clone(),
        );
        Ok((relative_path, owned))
    }

    /// Atomically adopt a canonical blob whose database references are about
    /// to be removed by `transaction`. The generic owner is inserted (or an
    /// existing matching owner reused) before that transaction can commit, so
    /// canonical ownership can never disappear first.
    pub(super) fn ensure_portability_blob_plaintext_stage_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        relative_path: &str,
    ) -> StorageResult<String> {
        ensure_canonical_blob_plaintext_stage_in_transaction(transaction, relative_path)
    }

    pub(super) fn portability_blob_plaintext_stage_id(
        &self,
        relative_path: &str,
    ) -> StorageResult<Option<String>> {
        validate_portability_blob_relative_path(relative_path)?;
        Ok(self
            .connection()?
            .query_row(
                "SELECT id FROM managed_plaintext_staging
                 WHERE kind = ?1 AND note_id IS NULL AND relative_path = ?2",
                params![PLAINTEXT_STAGE_PORTABILITY_BLOB, relative_path],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn reserve_managed_plaintext_stage(
        &self,
        kind: &str,
        note_id: Option<&NoteId>,
        relative_path: &str,
    ) -> StorageResult<String> {
        // Resolve before inserting so an invalid namespace can never become a
        // durable cleanup instruction.
        let _ = self.managed_plaintext_stage_path(kind, relative_path)?;
        let id = new_id("plainstage");
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO managed_plaintext_staging(id, kind, note_id, relative_path, created_at)\n             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&id, kind, note_id, relative_path, unix_timestamp()],
        )?;
        Ok(id)
    }

    /// Prepare and execute a crash-recoverable durable rename. The predecessor
    /// row records its canonical successor before the filesystem rename. The
    /// successor must already be owned either by the canonical database or by
    /// a separate durable staging row.
    pub(super) fn rename_owned_restore_plaintext_to_canonical(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
        destination_relative: &str,
    ) -> StorageResult<()> {
        validate_portability_blob_relative_path(destination_relative)?;
        self.rename_owned_library_blob_to_preowned(owned, destination_relative)
    }

    pub(super) fn rename_owned_canonical_blob_to_orphan(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
        destination_relative: &str,
    ) -> StorageResult<()> {
        validate_orphan_blob_relative_path(destination_relative)?;
        self.rename_owned_library_blob_to_preowned(owned, destination_relative)
    }

    fn rename_owned_library_blob_to_preowned(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
        destination_relative: &str,
    ) -> StorageResult<()> {
        let owner = owned.durable_ownership().ok_or_else(|| {
            StorageError::Integrity("blob rename predecessor is not durably owned".to_string())
        })?;
        if owner.kind != PLAINTEXT_STAGE_PORTABILITY_BLOB || owner.note_id.is_some() {
            return Err(StorageError::Integrity(
                "blob rename predecessor has an invalid durable ownership scope".to_string(),
            ));
        }
        validate_library_blob_transfer(&owner.relative_path, destination_relative)?;
        let source_stage_id = owner.stage_id.clone();
        let source_relative = owner.relative_path.clone();
        let source_path = self.managed_plaintext_stage_path(&owner.kind, &source_relative)?;
        if source_path != owned.path() {
            return Err(StorageError::Integrity(
                "blob rename predecessor path does not match durable ownership".to_string(),
            ));
        }

        if !self
            .plaintext_transfer_successor_is_owned(destination_relative, Some(&source_stage_id))?
        {
            return Err(StorageError::Integrity(
                "blob rename destination has no authoritative successor owner".to_string(),
            ));
        }

        let connection = self.connection()?;
        let updated = connection.execute(
            "UPDATE managed_plaintext_staging
             SET transfer_destination = ?1, transfer_state = 'prepared'
             WHERE id = ?2 AND kind = ?3 AND relative_path = ?4
               AND transfer_destination IS NULL AND transfer_state IS NULL",
            params![
                destination_relative,
                &source_stage_id,
                PLAINTEXT_STAGE_PORTABILITY_BLOB,
                &source_relative
            ],
        )?;
        if updated != 1 {
            return Err(StorageError::Integrity(
                "restore plaintext transfer could not be durably prepared".to_string(),
            ));
        }
        drop(connection);

        let destination = self
            .managed_plaintext_stage_path(PLAINTEXT_STAGE_PORTABILITY_BLOB, destination_relative)?;
        owned.rename_durable_to_preowned(&destination)?;
        Ok(())
    }

    pub(super) fn complete_owned_restore_plaintext_rename(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
    ) -> StorageResult<()> {
        self.complete_owned_library_blob_rename(owned)
    }

    pub(super) fn complete_owned_library_blob_rename(
        &self,
        owned: &mut crate::security::OwnedPlaintext,
    ) -> StorageResult<()> {
        let owner = owned.durable_ownership().ok_or_else(|| {
            StorageError::Integrity("blob rename predecessor is not durably owned".to_string())
        })?;
        let stage_id = owner.stage_id.clone();
        let source_path = self.managed_plaintext_stage_path(&owner.kind, &owner.relative_path)?;
        if source_path != owned.path() {
            return Err(StorageError::Integrity(
                "restore rename predecessor path no longer matches durable ownership".to_string(),
            ));
        }
        owned.ensure_path_absent()?;
        let connection = self.connection()?;
        let transfer: Option<(Option<String>, Option<String>)> = connection
            .query_row(
                "SELECT transfer_destination, transfer_state
                 FROM managed_plaintext_staging WHERE id = ?1",
                [&stage_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((Some(destination), Some(state))) = transfer else {
            return Err(StorageError::Integrity(
                "blob plaintext transfer record disappeared before retirement".to_string(),
            ));
        };
        if state != "prepared"
            || validate_library_blob_transfer(&owner.relative_path, &destination).is_err()
        {
            return Err(StorageError::Integrity(
                "blob plaintext transfer record is corrupt".to_string(),
            ));
        }
        drop(connection);
        if !self.plaintext_transfer_successor_is_owned(&destination, Some(&stage_id))? {
            return Err(StorageError::Integrity(
                "blob plaintext successor ownership disappeared before predecessor retirement"
                    .to_string(),
            ));
        }
        // Retire the predecessor row only inside a transaction that still has
        // a conclusive absence proof. If metadata becomes unreadable, rollback
        // preserves the durable cleanup obligation for startup recovery.
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        owned.ensure_path_absent()?;
        let removed = transaction.execute(
            "DELETE FROM managed_plaintext_staging WHERE id = ?1",
            [&stage_id],
        )?;
        if removed != 1 {
            return Err(StorageError::Integrity(
                "restore plaintext predecessor ownership could not be retired".to_string(),
            ));
        }
        owned.retire_transferred_durable_owner()?;
        transaction.commit()?;
        Ok(())
    }

    /// Transfer the generic ownership row to a more specific durable owner.
    /// Call this only inside the same transaction that creates that owner.
    fn transfer_managed_plaintext_stage(
        &self,
        transaction: &Transaction<'_>,
        stage_id: &str,
        expected_kind: &str,
        expected_note_id: Option<&NoteId>,
        expected_relative_path: &str,
    ) -> StorageResult<()> {
        let row = transaction
            .query_row(
                "SELECT kind, note_id, relative_path\n                 FROM managed_plaintext_staging WHERE id = ?1",
                [stage_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<NoteId>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((kind, note_id, relative_path)) = row else {
            return Err(StorageError::Integrity(
                "plaintext staging ownership disappeared before transfer".to_string(),
            ));
        };
        if kind != expected_kind
            || note_id.as_ref() != expected_note_id
            || relative_path != expected_relative_path
        {
            return Err(StorageError::Integrity(
                "plaintext staging ownership does not match the file being transferred".to_string(),
            ));
        }
        let removed = transaction.execute(
            "DELETE FROM managed_plaintext_staging WHERE id = ?1",
            [stage_id],
        )?;
        if removed != 1 {
            return Err(StorageError::Integrity(
                "plaintext staging ownership could not be transferred".to_string(),
            ));
        }
        Ok(())
    }

    /// Release a reservation only when the caller has conclusive evidence that
    /// file creation never succeeded. This is intentionally separate from the
    /// normal discard path so existing plaintext can never be forgotten.
    pub(super) fn release_uncreated_managed_plaintext_stage(
        &self,
        stage_id: &str,
    ) -> StorageResult<()> {
        let Some(stage) = self.get_managed_plaintext_stage(stage_id)? else {
            return Err(StorageError::Integrity(
                "uncreated plaintext staging reservation disappeared".to_string(),
            ));
        };
        let path = self.managed_plaintext_stage_path(&stage.kind, &stage.relative_path)?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(StorageError::Integrity(format!(
                    "cannot release plaintext staging ownership while its path exists: {}",
                    path.display()
                )));
            }
            Err(error) => return Err(error.into()),
        }
        let removed = self.connection()?.execute(
            "DELETE FROM managed_plaintext_staging WHERE id = ?1",
            [stage_id],
        )?;
        if removed != 1 {
            return Err(StorageError::Integrity(
                "uncreated plaintext staging reservation disappeared".to_string(),
            ));
        }
        Ok(())
    }

    fn discard_managed_plaintext_stage(&self, stage_id: &str) -> StorageResult<()> {
        if self.get_managed_plaintext_stage(stage_id)?.is_none() {
            return Ok(());
        }
        let mut owned = self.resume_owned_plaintext(stage_id)?;
        self.discard_owned_plaintext(&mut owned)
    }

    pub(super) fn discard_managed_plaintext_staging_for_note(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<usize> {
        let stages = self.list_managed_plaintext_stages(Some(note_id))?;
        let mut removed = 0;
        for stage in stages {
            self.discard_managed_plaintext_stage(&stage.id)?;
            removed += 1;
        }
        Ok(removed)
    }

    pub(super) fn pending_managed_attachment_staging_paths(&self) -> StorageResult<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT relative_path FROM managed_plaintext_staging\n             WHERE kind = 'attachment_incoming'\n             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Startup is a recovery boundary: a generic owner means no specialized
    /// transaction ever claimed the file. Such staging is disposable and must
    /// be removed before the library opens. Failure is deliberately fatal.
    pub(super) fn recover_managed_plaintext_staging_unlocked(&self) -> StorageResult<usize> {
        // Recovery reconciles only ownership that was durably recorded before
        // the crash. It never creates an owner merely because plaintext was
        // discovered on disk. Startup verifies that invariant separately.
        let stages = self.list_managed_plaintext_stages(None)?;
        for stage in &stages {
            validate_managed_plaintext_transfer_record(stage)?;
        }

        let mut removed = 0;
        // Reconcile prepared predecessor→successor transfers first. A successor
        // staging row is itself an ownership anchor and must not be retired until
        // every predecessor that names it has been reconciled.
        for stage in stages.iter().filter(|stage| stage.transfer_state.is_some()) {
            if !self.managed_plaintext_transfer_successor_is_owned(stage)? {
                return Err(StorageError::Integrity(
                    "managed plaintext transfer successor is no longer authoritatively owned"
                        .to_string(),
                ));
            }
            let destination_relative = stage.transfer_destination.as_deref().ok_or_else(|| {
                StorageError::Integrity(
                    "prepared plaintext transfer has no destination".to_string(),
                )
            })?;
            let source_path =
                self.managed_plaintext_stage_path(&stage.kind, &stage.relative_path)?;
            let destination_path = self.managed_plaintext_stage_path(
                PLAINTEXT_STAGE_PORTABILITY_BLOB,
                destination_relative,
            )?;

            match fs::symlink_metadata(&source_path) {
                Ok(metadata) => {
                    if !metadata.is_file() || metadata.file_type().is_symlink() {
                        return Err(StorageError::Integrity(format!(
                            "prepared plaintext transfer predecessor is not a regular file: {}",
                            source_path.display()
                        )));
                    }
                    let mut owned = crate::security::OwnedPlaintext::durable_managed_existing(
                        source_path.clone(),
                        stage.id.clone(),
                        stage.kind.clone(),
                        stage.note_id.clone(),
                        stage.relative_path.clone(),
                    );
                    owned.rename_durable_to_preowned(&destination_path)?;
                    super::attachments::sync_parent_directory(&destination_path)?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let metadata = fs::symlink_metadata(&destination_path).map_err(|error| {
                        if error.kind() == io::ErrorKind::NotFound {
                            StorageError::Integrity(format!(
                                "prepared plaintext transfer lost both predecessor and successor: {}",
                                destination_path.display()
                            ))
                        } else {
                            StorageError::Io(error)
                        }
                    })?;
                    if !metadata.is_file() || metadata.file_type().is_symlink() {
                        return Err(StorageError::Integrity(format!(
                            "prepared plaintext transfer successor is not a regular file: {}",
                            destination_path.display()
                        )));
                    }
                }
                Err(error) => return Err(error.into()),
            }

            self.connection()?.execute(
                "DELETE FROM managed_plaintext_staging WHERE id = ?1",
                [&stage.id],
            )?;
            removed += 1;
        }

        // Quarantine rows are long-lived cleanup owners, not abandoned staging.
        // If deletion completed before its owner row could retire, reconcile
        // that harmless stale obligation; otherwise retain the owner and file.
        for stage in stages.iter().filter(|stage| {
            stage.transfer_state.is_none()
                && validate_orphan_blob_relative_path(&stage.relative_path).is_ok()
        }) {
            let path = self.managed_plaintext_stage_path(&stage.kind, &stage.relative_path)?;
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(StorageError::Integrity(format!(
                        "owned attachment quarantine path is not a regular file: {}",
                        path.display()
                    )));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    self.release_uncreated_managed_plaintext_stage(&stage.id)?;
                    removed += 1;
                }
                Err(error) => return Err(error.into()),
            }
        }

        // Only after all transfer predecessors are gone may ordinary abandoned
        // staging (including former successor-only cleanup anchors) be retired.
        for stage in stages.iter().filter(|stage| {
            stage.transfer_state.is_none()
                && validate_orphan_blob_relative_path(&stage.relative_path).is_err()
        }) {
            self.discard_managed_plaintext_stage(&stage.id)?;
            removed += 1;
        }
        Ok(removed)
    }

    fn managed_plaintext_stage_is_required_transfer_successor(
        &self,
        stage: &ManagedPlaintextStage,
    ) -> StorageResult<bool> {
        let connection = self.connection()?;
        let required: i64 = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM managed_plaintext_staging
             WHERE transfer_destination = ?1 AND transfer_state = 'prepared' AND id <> ?2)",
            params![&stage.relative_path, &stage.id],
            |row| row.get(0),
        )?;
        Ok(required != 0)
    }

    fn managed_plaintext_transfer_successor_is_owned(
        &self,
        stage: &ManagedPlaintextStage,
    ) -> StorageResult<bool> {
        let Some(destination) = stage.transfer_destination.as_deref() else {
            return Ok(stage.transfer_state.is_none());
        };
        if stage.transfer_state.as_deref() != Some("prepared") {
            return Ok(false);
        }
        validate_library_blob_plaintext_relative_path(destination)?;
        self.plaintext_transfer_successor_is_owned(destination, Some(&stage.id))
    }

    fn plaintext_transfer_successor_is_owned(
        &self,
        destination: &str,
        excluded_stage_id: Option<&str>,
    ) -> StorageResult<bool> {
        validate_library_blob_plaintext_relative_path(destination)?;
        let connection = self.connection()?;
        if validate_portability_blob_relative_path(destination).is_ok() {
            let sha256 = Path::new(destination)
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    StorageError::Integrity(
                        "managed plaintext transfer destination has no canonical hash".to_string(),
                    )
                })?;
            let canonical_references: i64 = connection.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM attachments WHERE sha256 = ?1)
                    + (SELECT COUNT(*) FROM revision_attachment_refs WHERE sha256 = ?1)
                    + (SELECT COUNT(*) FROM vault_blob_refs WHERE sha256 = ?1)",
                [sha256],
                |row| row.get(0),
            )?;
            if canonical_references != 0 {
                return Ok(true);
            }
        }
        let excluded_stage_id = excluded_stage_id.unwrap_or("");
        let staged_successor: i64 = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM managed_plaintext_staging
             WHERE kind = ?1 AND note_id IS NULL AND relative_path = ?2 AND id <> ?3
               AND transfer_destination IS NULL AND transfer_state IS NULL)",
            params![
                PLAINTEXT_STAGE_PORTABILITY_BLOB,
                destination,
                excluded_stage_id
            ],
            |row| row.get(0),
        )?;
        Ok(staged_successor != 0)
    }

    fn get_managed_plaintext_stage(
        &self,
        stage_id: &str,
    ) -> StorageResult<Option<ManagedPlaintextStage>> {
        let connection = self.connection()?;
        Ok(connection
            .query_row(
                "SELECT id, kind, note_id, relative_path, transfer_destination, transfer_state\n                 FROM managed_plaintext_staging WHERE id = ?1",
                [stage_id],
                managed_plaintext_stage_from_row,
            )
            .optional()?)
    }

    fn list_managed_plaintext_stages(
        &self,
        note_id: Option<&NoteId>,
    ) -> StorageResult<Vec<ManagedPlaintextStage>> {
        let connection = self.connection()?;
        if let Some(note_id) = note_id {
            let mut statement = connection.prepare(
                "SELECT id, kind, note_id, relative_path, transfer_destination, transfer_state\n                 FROM managed_plaintext_staging\n                 WHERE note_id = ?1\n                 ORDER BY created_at, id",
            )?;
            let rows = statement.query_map([note_id], managed_plaintext_stage_from_row)?;
            return Ok(rows.collect::<Result<Vec<_>, _>>()?);
        }
        let mut statement = connection.prepare(
            "SELECT id, kind, note_id, relative_path, transfer_destination, transfer_state\n             FROM managed_plaintext_staging\n             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], managed_plaintext_stage_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn managed_plaintext_stage_path(
        &self,
        kind: &str,
        relative_path: &str,
    ) -> StorageResult<PathBuf> {
        match kind {
            PLAINTEXT_STAGE_ATTACHMENT_INCOMING => {
                super::attachments::validate_staging_relative_path(relative_path)?;
                Ok(self.attachments_directory.join(relative_path))
            }
            PLAINTEXT_STAGE_PORTABILITY_IMPORT => {
                validate_portability_stage_relative_path(relative_path)?;
                Ok(self.data_directory.join(relative_path))
            }
            PLAINTEXT_STAGE_PORTABILITY_BLOB => {
                validate_library_blob_plaintext_relative_path(relative_path)?;
                Ok(self.attachments_directory.join(relative_path))
            }
            _ => Err(StorageError::Integrity(
                "managed plaintext staging has an unknown namespace".to_string(),
            )),
        }
    }
}

/// Create or reuse durable cleanup ownership for a canonical plaintext blob
/// inside an arbitrary library transaction. This is used both by portability
/// rollback and by whole-library restore, where the *incoming* database must
/// already own old-library blobs at the instant it becomes canonical.
pub(super) fn ensure_canonical_blob_plaintext_stage_in_transaction(
    transaction: &Transaction<'_>,
    relative_path: &str,
) -> StorageResult<String> {
    validate_portability_blob_relative_path(relative_path)?;
    if let Some((id, kind, note_id)) = transaction
        .query_row(
            "SELECT id, kind, note_id FROM managed_plaintext_staging WHERE relative_path = ?1",
            [relative_path],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<NoteId>>(2)?,
                ))
            },
        )
        .optional()?
    {
        if kind != PLAINTEXT_STAGE_PORTABILITY_BLOB || note_id.is_some() {
            return Err(StorageError::Integrity(
                "canonical blob has conflicting plaintext ownership".to_string(),
            ));
        }
        return Ok(id);
    }

    let id = new_id("plainstage");
    transaction.execute(
        "INSERT INTO managed_plaintext_staging(id, kind, note_id, relative_path, created_at)
         VALUES (?1, ?2, NULL, ?3, ?4)",
        params![
            &id,
            PLAINTEXT_STAGE_PORTABILITY_BLOB,
            relative_path,
            unix_timestamp()
        ],
    )?;
    Ok(id)
}

/// If the current transaction has removed the final persistent reference to a
/// canonical plaintext attachment blob, create/reuse its generic durable owner
/// before that transaction can commit. This bridges database ownership to the
/// later quarantine transfer without any unowned interval.
pub(super) fn ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
    transaction: &Transaction<'_>,
    sha256: &str,
) -> StorageResult<()> {
    if !crate::attachment_paths::is_valid_sha256(sha256) {
        return Err(StorageError::Integrity(
            "invalid canonical attachment blob identity".to_string(),
        ));
    }
    let references: i64 = transaction.query_row(
        "SELECT
            (SELECT COUNT(*) FROM attachments WHERE sha256 = ?1)
            + (SELECT COUNT(*) FROM revision_attachment_refs WHERE sha256 = ?1)
            + (SELECT COUNT(*) FROM vault_blob_refs WHERE sha256 = ?1)",
        [sha256],
        |row| row.get(0),
    )?;
    if references != 0 {
        return Ok(());
    }

    // A pending Add operation is itself the durable cleanup owner for both its
    // staging predecessor and the canonical blob it is preparing to publish.
    // Do not create a second generic owner for the same canonical plaintext: a
    // later generic-stage recovery could otherwise delete the blob while the
    // Add operation still legitimately owns it. Corrupt operation payloads are
    // an integrity failure rather than permission to assume the blob is free.
    let mut pending_adds = transaction
        .prepare("SELECT attachment_json FROM attachment_operations WHERE operation = 'add'")?;
    let rows = pending_adds.query_map([], |row| row.get::<_, String>(0))?;
    for attachment_json in rows {
        let attachment: crate::domain::Attachment = serde_json::from_str(&attachment_json?)
            .map_err(|error| {
                StorageError::Integrity(format!(
                    "pending attachment Add ownership record is corrupt: {error}"
                ))
            })?;
        if attachment.sha256 == sha256 {
            return Ok(());
        }
    }
    drop(pending_adds);

    let relative_path =
        crate::attachment_paths::canonical_blob_relative_path(sha256).ok_or_else(|| {
            StorageError::Integrity("invalid canonical attachment blob identity".to_string())
        })?;
    ensure_canonical_blob_plaintext_stage_in_transaction(transaction, &relative_path)?;
    Ok(())
}

fn validate_managed_plaintext_transfer_record(stage: &ManagedPlaintextStage) -> StorageResult<()> {
    match (&stage.transfer_destination, &stage.transfer_state) {
        (None, None) => Ok(()),
        (Some(destination), Some(state))
            if stage.kind == PLAINTEXT_STAGE_PORTABILITY_BLOB
                && state == "prepared"
                && destination != &stage.relative_path
                && validate_library_blob_transfer(&stage.relative_path, destination).is_ok() =>
        {
            Ok(())
        }
        _ => Err(StorageError::Integrity(
            "managed plaintext transfer record is corrupt".to_string(),
        )),
    }
}

fn managed_plaintext_stage_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ManagedPlaintextStage> {
    Ok(ManagedPlaintextStage {
        id: row.get(0)?,
        kind: row.get(1)?,
        note_id: row.get(2)?,
        relative_path: row.get(3)?,
        transfer_destination: row.get(4)?,
        transfer_state: row.get(5)?,
    })
}

fn validate_portability_stage_relative_path(relative_path: &str) -> StorageResult<()> {
    let path = Path::new(relative_path);
    let mut components = path.components();
    let Some(Component::Normal(name)) = components.next() else {
        return Err(StorageError::Integrity(
            "invalid portability plaintext staging path".to_string(),
        ));
    };
    if components.next().is_some() {
        return Err(StorageError::Integrity(
            "invalid portability plaintext staging path".to_string(),
        ));
    }
    let Some(name) = name.to_str() else {
        return Err(StorageError::Integrity(
            "invalid portability plaintext staging path".to_string(),
        ));
    };
    if !name.starts_with(".portability-import-") || !name.ends_with(".rustnote") {
        return Err(StorageError::Integrity(
            "invalid portability plaintext staging path".to_string(),
        ));
    }
    Ok(())
}

fn validate_portability_blob_relative_path(relative_path: &str) -> StorageResult<()> {
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StorageError::Integrity(
            "invalid portability blob plaintext path".to_string(),
        ));
    }
    let Some(hash) = path.file_name().and_then(|value| value.to_str()) else {
        return Err(StorageError::Integrity(
            "invalid portability blob plaintext path".to_string(),
        ));
    };
    if crate::attachment_paths::canonical_blob_relative_path(hash).as_deref() != Some(relative_path)
    {
        return Err(StorageError::Integrity(
            "invalid portability blob plaintext path".to_string(),
        ));
    }
    Ok(())
}

fn validate_library_blob_plaintext_relative_path(relative_path: &str) -> StorageResult<()> {
    if validate_portability_blob_relative_path(relative_path).is_ok()
        || validate_restore_blob_temporary_relative_path(relative_path).is_ok()
        || validate_orphan_blob_relative_path(relative_path).is_ok()
    {
        return Ok(());
    }
    Err(StorageError::Integrity(
        "invalid library blob plaintext path".to_string(),
    ))
}

fn validate_library_blob_transfer(source: &str, destination: &str) -> StorageResult<()> {
    let restore_to_canonical = validate_restore_blob_temporary_relative_path(source).is_ok()
        && validate_portability_blob_relative_path(destination).is_ok();
    let canonical_to_orphan = validate_portability_blob_relative_path(source).is_ok()
        && validate_orphan_blob_relative_path(destination).is_ok();
    if restore_to_canonical || canonical_to_orphan {
        return Ok(());
    }
    Err(StorageError::Integrity(
        "invalid managed library-blob ownership transfer".to_string(),
    ))
}

fn validate_orphan_blob_relative_path(relative_path: &str) -> StorageResult<()> {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err(StorageError::Integrity(
            "invalid orphan attachment plaintext path".to_string(),
        ));
    }
    let mut components = path.components();
    let Some(Component::Normal(directory)) = components.next() else {
        return Err(StorageError::Integrity(
            "invalid orphan attachment plaintext path".to_string(),
        ));
    };
    if directory.to_str() != Some(".orphaned") {
        return Err(StorageError::Integrity(
            "invalid orphan attachment plaintext path".to_string(),
        ));
    }
    let Some(Component::Normal(file_name)) = components.next() else {
        return Err(StorageError::Integrity(
            "invalid orphan attachment plaintext path".to_string(),
        ));
    };
    if components.next().is_some() {
        return Err(StorageError::Integrity(
            "invalid orphan attachment plaintext path".to_string(),
        ));
    }
    let file_name = file_name.to_str().ok_or_else(|| {
        StorageError::Integrity("invalid orphan attachment plaintext path".to_string())
    })?;
    let Some((prefix, sha256)) = file_name.rsplit_once('-') else {
        return Err(StorageError::Integrity(
            "invalid orphan attachment plaintext path".to_string(),
        ));
    };
    if prefix.is_empty() || !crate::attachment_paths::is_valid_sha256(sha256) {
        return Err(StorageError::Integrity(
            "invalid orphan attachment plaintext path".to_string(),
        ));
    }
    Ok(())
}

fn validate_restore_blob_temporary_relative_path(relative_path: &str) -> StorageResult<()> {
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StorageError::Integrity(
            "invalid restore blob temporary path".to_string(),
        ));
    }

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            StorageError::Integrity("invalid restore blob temporary path".to_string())
        })?;
    let Some(rest) = file_name.strip_prefix('.') else {
        return Err(StorageError::Integrity(
            "invalid restore blob temporary path".to_string(),
        ));
    };
    let Some((sha256, suffix)) = rest.split_once(".restore-blob_") else {
        return Err(StorageError::Integrity(
            "invalid restore blob temporary path".to_string(),
        ));
    };
    let Some(id) = suffix.strip_suffix(".tmp") else {
        return Err(StorageError::Integrity(
            "invalid restore blob temporary path".to_string(),
        ));
    };
    let mut id_parts = id.split('_');
    let valid_id = matches!(
        (id_parts.next(), id_parts.next(), id_parts.next()),
        (Some(a), Some(b), None)
            if !a.is_empty()
                && !b.is_empty()
                && a.bytes().all(|byte| byte.is_ascii_hexdigit())
                && b.bytes().all(|byte| byte.is_ascii_hexdigit())
    );
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) || !valid_id {
        return Err(StorageError::Integrity(
            "invalid restore blob temporary path".to_string(),
        ));
    }

    let canonical = crate::attachment_paths::canonical_blob_relative_path(sha256)
        .ok_or_else(|| StorageError::Integrity("invalid restore blob hash".to_string()))?;
    if path.parent() != Path::new(&canonical).parent() {
        return Err(StorageError::Integrity(
            "restore blob temporary path is outside the canonical blob directory".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portability_stage_path_rejects_nested_paths() {
        assert!(
            validate_portability_stage_relative_path("nested/.portability-import-x.rustnote")
                .is_err()
        );
        assert!(validate_portability_stage_relative_path(".portability-import-x.rustnote").is_ok());
    }

    #[test]
    fn portability_blob_stage_accepts_only_canonical_blob_paths() {
        let hash = "a".repeat(64);
        let canonical = crate::attachment_paths::canonical_blob_relative_path(&hash).unwrap();
        assert!(validate_portability_blob_relative_path(&canonical).is_ok());
        assert!(validate_portability_blob_relative_path(&format!("blobs/ff/{hash}")).is_err());
        assert!(validate_portability_blob_relative_path(&format!("../{hash}")).is_err());
    }

    #[test]
    fn restore_blob_temporary_stage_is_narrow_and_same_directory() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("restore-temp")
        ));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let hash = "a".repeat(64);
        let (relative, mut owned) = storage
            .reserve_restore_blob_temporary_owned_plaintext(&hash)
            .expect("reserve restore temporary owner");
        let path = owned.path().to_path_buf();
        let canonical =
            crate::attachment_paths::canonical_blob_relative_path(&hash).expect("canonical path");
        assert_eq!(
            Path::new(&relative).parent(),
            Path::new(&canonical).parent()
        );
        assert!(validate_restore_blob_temporary_relative_path(&relative).is_ok());
        assert!(!path.exists());
        storage
            .release_uncreated_owned_plaintext(&mut owned)
            .expect("release unused restore temporary owner");
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn uncreated_reservation_cannot_be_released_if_plaintext_path_exists() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("release")
        ));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let relative = ".portability-import-owned.rustnote";
        let stage_id = storage
            .reserve_portability_plaintext_stage(relative)
            .expect("reserve owner");
        let path = storage.data_directory.join(relative);
        fs::write(&path, b"owned plaintext").expect("create owned plaintext");

        assert!(
            storage
                .release_uncreated_managed_plaintext_stage(&stage_id)
                .is_err(),
            "an existing path must keep its cleanup owner"
        );
        assert!(
            storage
                .get_managed_plaintext_stage(&stage_id)
                .expect("owner lookup")
                .is_some()
        );

        storage
            .discard_managed_plaintext_stage(&stage_id)
            .expect("discard owned plaintext");
        assert!(!path.exists());
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn durable_restore_rename_requires_successor_owner_and_keeps_predecessor_on_failure() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("rename-owner")
        ));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let bytes = b"durable rename plaintext";
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let digest = hasher.finalize();
            digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        let canonical =
            crate::attachment_paths::canonical_blob_relative_path(&hash).expect("canonical path");
        let (_temporary_relative, mut owned) = storage
            .reserve_restore_blob_temporary_owned_plaintext(&hash)
            .expect("reserve predecessor");
        if let Some(parent) = owned.path().parent() {
            fs::create_dir_all(parent).expect("temporary parent");
        }
        let mut file = owned.create_file().expect("materialize predecessor");
        use std::io::Write;
        file.write_all(bytes).expect("write predecessor");
        drop(file);
        let predecessor = owned.path().to_path_buf();

        assert!(
            storage
                .rename_owned_restore_plaintext_to_canonical(&mut owned, &canonical)
                .is_err(),
            "rename must fail before touching the filesystem when no successor owner exists"
        );
        assert_eq!(
            owned.lifecycle(),
            crate::security::PlaintextLifecycle::Materialized
        );
        assert_eq!(owned.path(), predecessor);
        assert!(predecessor.exists());
        let transfer = storage
            .connection()
            .expect("connection")
            .query_row(
                "SELECT transfer_destination, transfer_state FROM managed_plaintext_staging WHERE id = ?1",
                [owned.identity()],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .expect("transfer row");
        assert_eq!(transfer, (None, None));

        storage
            .discard_owned_plaintext(&mut owned)
            .expect("discard predecessor");
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn orphan_transfer_recovers_after_rename_before_predecessor_retirement() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("orphan-transfer-crash")
        ));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let hash = "c".repeat(64);
        let canonical =
            crate::attachment_paths::canonical_blob_relative_path(&hash).expect("canonical path");
        let mut predecessor = storage
            .reserve_portability_blob_owned_plaintext(&canonical)
            .expect("reserve canonical predecessor");
        if let Some(parent) = predecessor.path().parent() {
            fs::create_dir_all(parent).expect("canonical parent");
        }
        let mut file = predecessor
            .create_file()
            .expect("materialize canonical predecessor");
        use std::io::Write;
        file.write_all(b"owned orphan transfer")
            .expect("write predecessor");
        drop(file);
        let orphan_relative = format!(".orphaned/1-orphan-test-{hash}");
        fs::create_dir_all(storage.attachments_directory.join(".orphaned")).expect("orphan parent");
        let successor = storage
            .reserve_orphan_blob_owned_plaintext(&orphan_relative)
            .expect("reserve orphan successor");
        storage
            .rename_owned_canonical_blob_to_orphan(&mut predecessor, &orphan_relative)
            .expect("rename into owned quarantine");
        let predecessor_id = predecessor.identity().to_string();
        let successor_id = successor.identity().to_string();
        let orphan_path = storage.attachments_directory.join(&orphan_relative);
        assert!(orphan_path.exists());
        drop(successor);
        drop(predecessor);
        drop(storage);

        let recovered = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("startup recovery");
        assert!(
            orphan_path.exists(),
            "recovery must retain the owned successor file"
        );
        assert!(
            recovered
                .get_managed_plaintext_stage(&predecessor_id)
                .expect("predecessor lookup")
                .is_none(),
            "recovery must retire the absent predecessor owner"
        );
        assert!(
            recovered
                .get_managed_plaintext_stage(&successor_id)
                .expect("successor lookup")
                .is_some(),
            "quarantined plaintext must keep its durable successor owner"
        );
        let mut owned = recovered
            .require_orphan_blob_owned_plaintext(&orphan_relative)
            .expect("resume orphan owner");
        recovered
            .discard_owned_plaintext(&mut owned)
            .expect("delete quarantined plaintext");
        assert!(!orphan_path.exists());
        drop(recovered);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_verification_rejects_preexisting_unowned_orphan_plaintext() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("orphan-startup-fail-closed")
        ));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let hash = "d".repeat(64);
        let orphan_relative = format!(".orphaned/1-unowned-orphan-{hash}");
        let orphan_path = storage.attachments_directory.join(&orphan_relative);
        fs::write(&orphan_path, b"unowned quarantine residue")
            .expect("simulate invariant violation");

        assert!(
            storage
                .verify_managed_attachment_plaintext_ownership_unlocked()
                .is_err(),
            "startup must fail closed rather than adopt already-existing plaintext"
        );
        assert!(
            orphan_path.exists(),
            "verification must not erase ownership evidence"
        );
        assert!(
            storage
                .require_orphan_blob_owned_plaintext(&orphan_relative)
                .is_err(),
            "verification must not manufacture a durable owner after plaintext exists"
        );
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_durable_transfer_record_fails_closed_without_forgetting_plaintext() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("corrupt-transfer")
        ));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let hash = "b".repeat(64);
        let canonical =
            crate::attachment_paths::canonical_blob_relative_path(&hash).expect("canonical path");
        let (_temporary_relative, mut owned) = storage
            .reserve_restore_blob_temporary_owned_plaintext(&hash)
            .expect("reserve predecessor");
        if let Some(parent) = owned.path().parent() {
            fs::create_dir_all(parent).expect("temporary parent");
        }
        let mut file = owned.create_file().expect("materialize predecessor");
        use std::io::Write;
        file.write_all(b"secret").expect("write predecessor");
        drop(file);
        let predecessor = owned.path().to_path_buf();
        storage
            .connection()
            .expect("connection")
            .execute(
                "UPDATE managed_plaintext_staging SET transfer_destination = ?1 WHERE id = ?2",
                params![canonical, owned.identity()],
            )
            .expect("corrupt transfer record");

        assert!(
            storage
                .recover_managed_plaintext_staging_unlocked()
                .is_err()
        );
        assert!(
            predecessor.exists(),
            "corrupt ownership must not delete plaintext"
        );
        assert!(
            storage
                .get_managed_plaintext_stage(owned.identity())
                .expect("owner lookup")
                .is_some(),
            "corrupt ownership must remain durable for diagnosis/retry"
        );

        storage
            .connection()
            .expect("connection")
            .execute(
                "UPDATE managed_plaintext_staging SET transfer_destination = NULL, transfer_state = NULL WHERE id = ?1",
                [owned.identity()],
            )
            .expect("repair transfer record");
        storage
            .discard_owned_plaintext(&mut owned)
            .expect("discard repaired predecessor");
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_durable_filesystem_rename_keeps_predecessor_and_prepared_transfer_owner() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("rename-failure")
        ));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let note = storage.create_note(None).expect("test note");
        let external = root.join("source.bin");
        let bytes = b"rename failure ownership";
        fs::write(&external, bytes).expect("source bytes");
        let attachment = storage
            .add_attachment_file_as(
                &note.id,
                "source.bin",
                Some("application/octet-stream"),
                &external,
            )
            .expect("attachment");
        let canonical = storage
            .attachment_file_path(&attachment)
            .expect("canonical attachment path");
        fs::remove_file(&canonical).expect("remove canonical file");
        fs::create_dir(&canonical).expect("install rename obstacle");

        let canonical_relative =
            crate::attachment_paths::canonical_blob_relative_path(&attachment.sha256)
                .expect("canonical relative path");
        let (_temporary_relative, mut owned) = storage
            .reserve_restore_blob_temporary_owned_plaintext(&attachment.sha256)
            .expect("reserve predecessor");
        let mut file = owned.create_file().expect("materialize predecessor");
        use std::io::Write;
        file.write_all(bytes).expect("write predecessor");
        drop(file);
        let predecessor = owned.path().to_path_buf();

        assert!(
            storage
                .rename_owned_restore_plaintext_to_canonical(&mut owned, &canonical_relative)
                .is_err(),
            "filesystem rename failure must be surfaced"
        );
        assert!(
            predecessor.exists(),
            "predecessor plaintext must remain owned"
        );
        assert_eq!(owned.path(), predecessor);
        assert_eq!(
            owned.lifecycle(),
            crate::security::PlaintextLifecycle::Materialized
        );
        let transfer = storage
            .connection()
            .expect("connection")
            .query_row(
                "SELECT transfer_destination, transfer_state FROM managed_plaintext_staging WHERE id = ?1",
                [owned.identity()],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .expect("transfer row");
        assert_eq!(transfer.0.as_deref(), Some(canonical_relative.as_str()));
        assert_eq!(transfer.1.as_deref(), Some("prepared"));

        storage
            .discard_owned_plaintext(&mut owned)
            .expect("discard failed-rename predecessor");
        fs::remove_dir(&canonical).expect("remove rename obstacle");
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restart_completes_prepared_transfer_when_canonical_bytes_are_missing() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("restart-before-rename")
        ));
        let data = root.join("data");
        let backups = root.join("backups");
        let bytes = b"restart before rename";

        let (canonical, predecessor, stage_id) = {
            let storage =
                Storage::at_with_backup(data.clone(), backups.clone()).expect("test storage");
            let note = storage.create_note(None).expect("test note");
            let external = root.join("source.bin");
            fs::write(&external, bytes).expect("source bytes");
            let attachment = storage
                .add_attachment_file_as(
                    &note.id,
                    "source.bin",
                    Some("application/octet-stream"),
                    &external,
                )
                .expect("attachment");
            let canonical = storage
                .attachment_file_path(&attachment)
                .expect("canonical attachment path");
            fs::remove_file(&canonical).expect("simulate missing canonical bytes");
            let canonical_relative =
                crate::attachment_paths::canonical_blob_relative_path(&attachment.sha256)
                    .expect("canonical relative path");
            let (_temporary_relative, mut owned) = storage
                .reserve_restore_blob_temporary_owned_plaintext(&attachment.sha256)
                .expect("reserve predecessor");
            let mut file = owned.create_file().expect("materialize predecessor");
            use std::io::Write;
            file.write_all(bytes).expect("write predecessor");
            drop(file);
            let predecessor = owned.path().to_path_buf();
            let stage_id = owned.identity().to_string();
            storage
                .connection()
                .expect("connection")
                .execute(
                    "UPDATE managed_plaintext_staging SET transfer_destination = ?1, transfer_state = 'prepared' WHERE id = ?2",
                    params![canonical_relative, &stage_id],
                )
                .expect("prepare transfer record");
            assert!(predecessor.exists());
            assert!(!canonical.exists());
            // Simulate process death after durable preparation but before fs::rename.
            (canonical, predecessor, stage_id)
        };

        let reopened =
            Storage::at_with_backup(data.clone(), backups.clone()).expect("restart recovery");
        assert!(
            canonical.exists(),
            "restart must complete the prepared transfer"
        );
        assert_eq!(fs::read(&canonical).expect("recovered bytes"), bytes);
        assert!(
            !predecessor.exists(),
            "predecessor must be consumed by recovery rename"
        );
        assert!(
            reopened
                .get_managed_plaintext_stage(&stage_id)
                .expect("predecessor owner lookup")
                .is_none(),
            "recovery must retire the prepared predecessor only after bytes reach the successor"
        );
        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restart_recovers_durable_rename_after_filesystem_transfer_before_owner_retirement() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("restart-transfer")
        ));
        let data = root.join("data");
        let backups = root.join("backups");
        let bytes = b"restart-safe canonical plaintext";

        let (canonical, predecessor, stage_id) = {
            let storage =
                Storage::at_with_backup(data.clone(), backups.clone()).expect("test storage");
            let note = storage.create_note(None).expect("test note");
            let external = root.join("source.bin");
            fs::write(&external, bytes).expect("source bytes");
            let attachment = storage
                .add_attachment_file_as(
                    &note.id,
                    "source.bin",
                    Some("application/octet-stream"),
                    &external,
                )
                .expect("attachment");
            let canonical = storage
                .attachment_file_path(&attachment)
                .expect("canonical attachment path");
            fs::remove_file(&canonical).expect("simulate missing canonical bytes");

            let (_relative, mut owned) = storage
                .reserve_restore_blob_temporary_owned_plaintext(&attachment.sha256)
                .expect("reserve predecessor");
            if let Some(parent) = owned.path().parent() {
                fs::create_dir_all(parent).expect("temporary parent");
            }
            let mut file = owned.create_file().expect("materialize predecessor");
            use std::io::Write;
            file.write_all(bytes).expect("write predecessor");
            drop(file);
            let predecessor = owned.path().to_path_buf();
            let stage_id = owned.identity().to_string();
            let relative =
                crate::attachment_paths::canonical_blob_relative_path(&attachment.sha256)
                    .expect("canonical relative path");
            storage
                .rename_owned_restore_plaintext_to_canonical(&mut owned, &relative)
                .expect("filesystem transfer");
            assert!(!predecessor.exists());
            assert!(canonical.exists());
            // Simulate process death here: deliberately do not call
            // complete_owned_restore_plaintext_rename.
            (canonical, predecessor, stage_id)
        };

        let reopened =
            Storage::at_with_backup(data.clone(), backups.clone()).expect("restart recovery");
        assert!(
            canonical.exists(),
            "canonical successor must survive restart recovery"
        );
        assert!(!predecessor.exists());
        assert!(
            reopened
                .get_managed_plaintext_stage(&stage_id)
                .expect("predecessor owner lookup")
                .is_none(),
            "restart must retire the transferred predecessor owner"
        );
        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_transfer_pins_successor_owner_until_predecessor_is_reconciled() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-plaintext-owner-test-{}",
            new_id("successor-pin")
        ));
        let data = root.join("data");
        let backups = root.join("backups");
        let hash = "b".repeat(64);

        let (canonical, predecessor_id, successor_id) = {
            let storage =
                Storage::at_with_backup(data.clone(), backups.clone()).expect("test storage");
            let canonical_relative = crate::attachment_paths::canonical_blob_relative_path(&hash)
                .expect("canonical relative path");
            let mut successor_owned = storage
                .reserve_canonical_blob_owned_plaintext(&canonical_relative)
                .expect("reserve successor owner");
            let successor_id = successor_owned.identity().to_string();
            let (_temporary_relative, mut owned) = storage
                .reserve_restore_blob_temporary_owned_plaintext(&hash)
                .expect("reserve predecessor owner");
            if let Some(parent) = owned.path().parent() {
                fs::create_dir_all(parent).expect("temporary parent");
            }
            let mut file = owned.create_file().expect("materialize predecessor");
            use std::io::Write;
            file.write_all(b"successor pin plaintext")
                .expect("write predecessor");
            drop(file);
            let predecessor_id = owned.identity().to_string();
            let canonical = storage.attachments_directory.join(&canonical_relative);

            storage
                .rename_owned_restore_plaintext_to_canonical(&mut owned, &canonical_relative)
                .expect("prepare and execute transfer");
            assert!(canonical.exists());
            assert!(
                storage
                    .discard_owned_plaintext(&mut successor_owned)
                    .is_err(),
                "prepared predecessor must pin successor ownership"
            );
            assert!(
                canonical.exists(),
                "failed successor retirement must preserve bytes"
            );
            assert!(
                storage
                    .get_managed_plaintext_stage(&successor_id)
                    .expect("successor owner lookup")
                    .is_some()
            );

            // Simulate process death after filesystem transfer but before the
            // predecessor transfer is finalized.
            (canonical, predecessor_id, successor_id)
        };

        let reopened =
            Storage::at_with_backup(data.clone(), backups.clone()).expect("restart recovery");
        assert!(
            !canonical.exists(),
            "abandoned successor bytes may be cleaned only after predecessor reconciliation"
        );
        assert!(
            reopened
                .get_managed_plaintext_stage(&predecessor_id)
                .expect("predecessor owner lookup")
                .is_none()
        );
        assert!(
            reopened
                .get_managed_plaintext_stage(&successor_id)
                .expect("successor owner lookup")
                .is_none()
        );
        drop(reopened);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_plaintext_cleanup_keeps_durable_owner_for_retry() {
        let root =
            std::env::temp_dir().join(format!("rustnote-plaintext-owner-test-{}", new_id("retry")));
        let storage =
            Storage::at_with_backup(root.join("data"), root.join("backups")).expect("test storage");
        let note = storage.create_note(None).expect("test note");
        let relative_path = ".incoming/forced-cleanup-failure.tmp";
        let stage_id = storage
            .reserve_attachment_plaintext_stage(&note.id, relative_path)
            .expect("reserve owner before plaintext");
        let path = storage
            .managed_plaintext_stage_path(PLAINTEXT_STAGE_ATTACHMENT_INCOMING, relative_path)
            .expect("resolve stage");
        fs::create_dir_all(&path).expect("create non-file cleanup obstacle");

        assert!(storage.discard_managed_plaintext_stage(&stage_id).is_err());
        assert!(
            storage
                .get_managed_plaintext_stage(&stage_id)
                .expect("owner lookup")
                .is_some(),
            "failed cleanup must retain durable ownership"
        );

        fs::remove_dir(&path).expect("remove cleanup obstacle");
        fs::write(&path, b"retry plaintext").expect("replace obstacle with owned file");
        storage
            .discard_managed_plaintext_stage(&stage_id)
            .expect("retry cleanup");
        assert!(!path.exists());
        assert!(
            storage
                .get_managed_plaintext_stage(&stage_id)
                .expect("owner lookup after retry")
                .is_none()
        );

        drop(storage);
        let _ = fs::remove_dir_all(root);
    }
}

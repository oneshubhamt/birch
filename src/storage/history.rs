use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, params, params_from_iter,
};

use crate::domain::{
    Attachment, DEFAULT_NOTEBOOK_ID, Note, NoteId, NoteRevision, NoteRevisionRestoreResult,
    NoteRevisionSummary, RevisionAttachment, RevisionRetention, RevisionSnapshot,
    RevisionStorageUsage,
};
use crate::{
    editor_document::validate_editor_state,
    editor_projection::derive_editor_projections,
    security::{VaultNoteAccess, VaultReadPermit},
};

use super::attachments::{sha256_bytes, sha256_file, validate_sha256};
use super::settings::trash_deadline_at;
use super::{RevisionMode, Storage, StorageError, StorageResult, new_id, unix_timestamp};

pub(super) const REVISION_MAX_KEY: &str = "revision.max_per_note";
pub(super) const REVISION_AGE_KEY: &str = "revision.max_age_days";
pub(super) const REVISION_BYTES_KEY: &str = "revision.max_total_bytes";
pub(super) const AUTOSAVE_REVISION_INTERVAL_SECONDS: i64 = 5 * 60;
const REVISION_DELETE_BATCH_SIZE: usize = 400;

/// A revision snapshot whose protected plaintext lifetime remains authorized.
///
/// Ordinary revisions carry no Vault permit. Protected revisions retain their
/// read permit until the caller has finished every derived plaintext operation
/// (for example Markdown/HTML preview generation) and has either published or
/// discarded the result.
#[derive(Debug)]
pub(crate) struct RevisionSnapshotResource {
    snapshot: RevisionSnapshot,
    _vault_permit: Option<VaultReadPermit>,
}

impl RevisionSnapshotResource {
    pub(super) fn ordinary(snapshot: RevisionSnapshot) -> Self {
        Self {
            snapshot,
            _vault_permit: None,
        }
    }

    pub(super) fn protected(snapshot: RevisionSnapshot, permit: VaultReadPermit) -> Self {
        Self {
            snapshot,
            _vault_permit: Some(permit),
        }
    }

    pub(crate) fn snapshot(&self) -> &RevisionSnapshot {
        &self.snapshot
    }

    pub(crate) fn is_publishable(&self) -> bool {
        self._vault_permit
            .as_ref()
            .map(|permit| permit.is_publishable().unwrap_or(false))
            .unwrap_or(true)
    }
}

/// Opaque authorization retained by the UI while a protected revision-restore
/// result is still being projected or published. Dropping this value releases
/// the Vault read permit; Lock therefore cannot complete midway through the
/// sensitive post-storage lifetime.
#[derive(Clone, Debug)]
pub(crate) struct RevisionRestoreAuthorization {
    access: Arc<VaultNoteAccess>,
}

impl RevisionRestoreAuthorization {
    pub(super) fn protected(access: VaultNoteAccess) -> Self {
        Self {
            access: Arc::new(access),
        }
    }

    pub(super) fn access(&self) -> &VaultNoteAccess {
        self.access.as_ref()
    }
}

struct RevisionPruneCandidate {
    id: String,
    snapshot_bytes: i64,
    historical_blob_refs: Vec<RevisionBlobReference>,
}

struct RevisionBlobReference {
    sha256: String,
    reference_count: i64,
}

struct HistoricalBlobBudget {
    byte_size: i64,
    remaining_references: i64,
}

impl Storage {
    pub fn list_revision_summaries(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Vec<NoteRevisionSummary>> {
        if let Some(summaries) = self.protected_revision_summaries_if_unlocked(note_id)? {
            return Ok(summaries);
        }
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, note_id, revision_no, byte_size, created_at
             FROM note_revisions WHERE note_id = ?1 ORDER BY revision_no DESC",
        )?;
        let rows = statement.query_map([note_id], revision_summary_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn revision_snapshot_for_note(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> StorageResult<RevisionSnapshot> {
        if let Some(snapshot) =
            self.protected_revision_snapshot_if_unlocked(note_id, revision_id)?
        {
            return Ok(snapshot);
        }
        self.revision_snapshot_by_id(revision_id)
    }

    pub(crate) fn revision_snapshot_resource_for_note(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> StorageResult<RevisionSnapshotResource> {
        if let Some(resource) =
            self.protected_revision_snapshot_resource_if_unlocked(note_id, revision_id)?
        {
            return Ok(resource);
        }
        self.revision_snapshot_by_id(revision_id)
            .map(RevisionSnapshotResource::ordinary)
    }

    pub fn revision_snapshot_by_id(&self, revision_id: &str) -> StorageResult<RevisionSnapshot> {
        let connection = self.connection()?;
        let revision = connection
            .query_row(
                "SELECT id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at
                 FROM note_revisions WHERE id = ?1",
                [revision_id],
                revision_from_row,
            )
            .optional()?
            .ok_or_else(|| StorageError::Validation("revision no longer exists".to_string()))?;
        self.revision_snapshot(&revision)
    }

    pub fn revision_count(&self, note_id: &NoteId) -> StorageResult<usize> {
        let connection = self.connection()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM note_revisions WHERE note_id = ?1",
            [note_id],
            |row| row.get(0),
        )?;
        usize::try_from(count).map_err(|_| {
            StorageError::Integrity("revision count is outside the supported range".to_string())
        })
    }
    pub fn revision_snapshot(&self, revision: &NoteRevision) -> StorageResult<RevisionSnapshot> {
        validate_sha256(&revision.checksum)?;
        if sha256_bytes(revision.snapshot_json.as_bytes()) != revision.checksum {
            return Err(StorageError::Integrity(format!(
                "revision {} failed its checksum",
                revision.id
            )));
        }
        let snapshot: RevisionSnapshot = serde_json::from_str(&revision.snapshot_json)?;
        if snapshot.format_version != RevisionSnapshot::FORMAT_VERSION {
            return Err(StorageError::Validation(format!(
                "unsupported revision snapshot version {}",
                snapshot.format_version
            )));
        }
        if snapshot.note.id != revision.note_id {
            return Err(StorageError::Integrity(format!(
                "revision {} belongs to a different note",
                revision.id
            )));
        }
        for attachment in &snapshot.attachments {
            validate_sha256(&attachment.sha256)?;
            if attachment.byte_size < 0 {
                return Err(StorageError::Integrity(format!(
                    "revision {} contains a negative attachment size",
                    revision.id
                )));
            }
        }
        validate_editor_state(snapshot.note.editor_state.as_str()).map_err(|error| {
            StorageError::Validation(format!(
                "revision {} contains an unsupported editor state: {error}",
                revision.id
            ))
        })?;
        Ok(snapshot)
    }

    /// Restore only the note-owned state represented by a version-history snapshot.
    ///
    /// Document-derived indexes are regenerated by `persist_note`. Independently stored
    /// relationship tables are deliberately outside this operation and must remain current.
    pub fn restore_note_revision(
        &self,
        revision_id: &str,
    ) -> StorageResult<NoteRevisionRestoreResult> {
        // Revision restore can retire current attachment derivatives. Resolve
        // the note scope first, then let the coordinator drain/freeze plaintext
        // producers before taking the storage lock. The revision itself is
        // re-read below while that boundary is held.
        let connection = self.read_only_connection()?;
        let note_id = connection
            .query_row(
                "SELECT note_id FROM note_revisions WHERE id = ?1",
                [revision_id],
                |row| row.get::<_, NoteId>(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::Validation("revision no longer exists".to_string()))?;
        drop(connection);
        let _cleanup_guard = self.begin_plaintext_derivative_cleanup(&note_id)?;

        // Read and verify the potentially large snapshot and attachment blobs before
        // opening the SQLite write transaction. The storage operation lock keeps
        // competing Rustnote mutations out while the preflight runs.
        let connection = self.connection()?;
        let revision = connection
            .query_row(
                "SELECT id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at
                 FROM note_revisions WHERE id = ?1",
                [revision_id],
                revision_from_row,
            )
            .optional()?
            .ok_or_else(|| StorageError::Validation("revision no longer exists".to_string()))?;
        let snapshot = self.revision_snapshot(&revision)?;
        let current = self
            .note_from_connection(&connection, &revision.note_id)?
            .ok_or_else(|| StorageError::Validation("note no longer exists".to_string()))?;
        if current.trashed {
            return Err(StorageError::Validation(
                "notes in Trash are read-only; recover the note before restoring a version"
                    .to_string(),
            ));
        }
        let trash_retention_days = self.trash_retention_days()?;
        let current_attachments =
            self.list_referenced_attachments_with_connection(&connection, &revision.note_id)?;
        let mut restored_note = materialize_revision_note(&snapshot)?;
        let mut restored_attachments = snapshot
            .attachments
            .iter()
            .map(|attachment| revision_attachment_to_attachment(&current.id, attachment))
            .collect::<StorageResult<Vec<_>>>()?;
        // Edited renditions reference their immutable root source through a foreign key.
        // Restore root attachments first so insertion remains valid regardless of snapshot order.
        restored_attachments.sort_by_key(|attachment| attachment.source_attachment_id.is_some());

        for attachment in &restored_attachments {
            let path = self.attachment_file_path(attachment)?;
            if !crate::security::managed_path_is_present(&path)? {
                return Err(StorageError::Integrity(format!(
                    "cannot restore version: attachment '{}' is missing",
                    attachment.file_name
                )));
            }
            let (checksum, byte_size) = sha256_file(&path)?;
            if checksum != attachment.sha256 || byte_size != attachment.byte_size as u64 {
                return Err(StorageError::Integrity(format!(
                    "cannot restore version: attachment '{}' failed verification",
                    attachment.file_name
                )));
            }
        }

        let expected_revision_checksum = revision.checksum.clone();
        let expected_note_updated_at = current.updated_at;
        let expected_attachment_state = current_attachments
            .iter()
            .map(|attachment| {
                (
                    attachment.id.clone(),
                    attachment.sha256.clone(),
                    attachment.byte_size,
                )
            })
            .collect::<HashSet<_>>();
        drop(connection);

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let confirmed_revision: Option<(NoteId, String)> = transaction
            .query_row(
                "SELECT note_id, checksum FROM note_revisions WHERE id = ?1",
                [revision_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if confirmed_revision != Some((revision.note_id.clone(), expected_revision_checksum)) {
            return Err(StorageError::Validation(
                "the selected revision changed while its attachments were being verified"
                    .to_string(),
            ));
        }
        let confirmed_updated_at: Option<i64> = transaction
            .query_row(
                "SELECT updated_at FROM notes WHERE id = ?1",
                [&current.id],
                |row| row.get(0),
            )
            .optional()?;
        if confirmed_updated_at != Some(expected_note_updated_at) {
            return Err(StorageError::Validation(
                "the note changed while the selected revision was being verified".to_string(),
            ));
        }
        let confirmed_attachments =
            self.list_referenced_attachments_with_connection(&transaction, &current.id)?;
        let confirmed_attachment_state = confirmed_attachments
            .iter()
            .map(|attachment| {
                (
                    attachment.id.clone(),
                    attachment.sha256.clone(),
                    attachment.byte_size,
                )
            })
            .collect::<HashSet<_>>();
        if confirmed_attachment_state != expected_attachment_state {
            return Err(StorageError::Validation(
                "the note attachments changed while the selected revision was being verified"
                    .to_string(),
            ));
        }

        self.record_revision(&transaction, &current, RevisionMode::Checkpoint)?;

        restored_note.id = current.id.clone();
        let restored_at = unix_timestamp().max(current.updated_at);
        restored_note.updated_at = restored_at;
        if restored_note.trashed {
            restored_note.purge_after =
                trash_retention_days.map(|days| trash_deadline_at(restored_at, days));
        } else {
            restored_note.purge_after = None;
        }
        let notebook_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM notebooks WHERE id = ?1)",
            [&restored_note.notebook_id],
            |row| row.get(0),
        )?;
        if !notebook_exists {
            restored_note.notebook_id = DEFAULT_NOTEBOOK_ID.to_string();
        }
        let restored_ids = restored_attachments
            .iter()
            .map(|attachment| attachment.id.clone())
            .collect::<HashSet<_>>();
        transaction.execute("DELETE FROM attachments WHERE note_id = ?1", [&current.id])?;
        for attachment in &restored_attachments {
            self.insert_attachment(&transaction, attachment)?;
            self.enqueue_sync_upsert(&transaction, "attachment", &attachment.id, attachment)?;
        }
        for attachment in &current_attachments {
            if !restored_ids.contains(&attachment.id) {
                self.enqueue_sync_delete(&transaction, "attachment", &attachment.id)?;
            }
        }

        // Revision restore intentionally stops at note-owned state. `persist_note` is used
        // here because it validates the restored document and rebuilds only indexes derived
        // from that document (currently attachment membership and internal note links). Do not add
        // parent/child hierarchy, sources, citations, or other independently stored
        // relationship mutations to this restore transaction. Those relationships remain
        // current when note text/history is restored.
        let (restored_note, _) = self
            .persist_note(&transaction, None, restored_note, None)
            .map_err(|failure| failure.error)?;
        self.record_revision(&transaction, &restored_note, RevisionMode::Checkpoint)?;
        self.enqueue_sync_upsert(
            &transaction,
            "note",
            restored_note.id.as_str(),
            &restored_note,
        )?;
        let current_blob_hashes = current_attachments
            .iter()
            .map(|attachment| attachment.sha256.clone())
            .collect::<HashSet<_>>();
        for sha256 in &current_blob_hashes {
            super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
                &transaction,
                sha256,
            )?;
        }
        transaction.commit()?;

        let mut warnings = Vec::new();
        for sha256 in current_blob_hashes {
            if let Err(error) = self.quarantine_blob_if_unreferenced(&sha256) {
                warnings.push(format!(
                    "Attachment cleanup will be retried during storage maintenance: {error}"
                ));
            }
        }
        Ok(NoteRevisionRestoreResult {
            note: restored_note,
            warnings,
        })
    }
    pub fn revision_retention(&self) -> StorageResult<RevisionRetention> {
        let connection = self.connection()?;
        revision_retention_from_connection(&connection)
    }
    pub fn revision_storage_usage(&self) -> StorageResult<RevisionStorageUsage> {
        let connection = self.connection()?;
        revision_storage_usage_from_connection(&connection)
    }
    pub fn set_revision_retention(
        &self,
        max_revisions_per_note: i64,
        max_age_days: i64,
    ) -> StorageResult<usize> {
        if !(10..=2_000).contains(&max_revisions_per_note) {
            return Err(StorageError::Validation(
                "revision count must be between 10 and 2000".to_string(),
            ));
        }
        if !(0..=3_650).contains(&max_age_days) {
            return Err(StorageError::Validation(
                "revision age must be between 0 and 3650 days".to_string(),
            ));
        }

        let removed = {
            let _operation_guard = self.lock_operations()?;
            let mut connection = self.connection()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![REVISION_MAX_KEY, max_revisions_per_note.to_string()],
            )?;
            transaction.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![REVISION_AGE_KEY, max_age_days.to_string()],
            )?;
            let removed = self.prune_revision_history_transaction(&transaction)?;
            transaction.commit()?;
            if removed > 0 {
                self.quarantine_orphan_blobs_unlocked()?;
            }
            removed
        };
        Ok(removed.saturating_add(self.prune_unlocked_protected_revision_history()?))
    }
    pub fn set_revision_storage_limit(&self, max_total_bytes: i64) -> StorageResult<usize> {
        if !(16_i64 * 1024 * 1024..=10_i64 * 1024 * 1024 * 1024).contains(&max_total_bytes) {
            return Err(StorageError::Validation(
                "revision storage limit must be between 16 MB and 10 GB".to_string(),
            ));
        }
        let removed = {
            let _operation_guard = self.lock_operations()?;
            let mut connection = self.connection()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![REVISION_BYTES_KEY, max_total_bytes.to_string()],
            )?;
            let removed = self.prune_revision_history_transaction(&transaction)?;
            transaction.commit()?;
            if removed > 0 {
                self.quarantine_orphan_blobs_unlocked()?;
            }
            removed
        };
        // Protected revision pruning is deliberately deferred/explicit; its
        // count/age retention and stale-object/blob GC run here without putting
        // an O(history) walk on protected autosave.
        Ok(removed.saturating_add(self.prune_unlocked_protected_revision_history()?))
    }
    pub fn prune_revision_history(&self) -> StorageResult<usize> {
        let ordinary_removed = {
            let _operation_guard = self.lock_operations()?;
            self.prune_revision_history_unlocked()?
        };
        Ok(ordinary_removed.saturating_add(self.prune_unlocked_protected_revision_history()?))
    }
    pub(super) fn prune_revision_history_unlocked(&self) -> StorageResult<usize> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = self.prune_revision_history_transaction(&transaction)?;
        transaction.commit()?;
        if removed > 0 {
            self.quarantine_orphan_blobs_unlocked()?;
        }
        Ok(removed)
    }
    pub(super) fn prune_revision_history_transaction(
        &self,
        transaction: &Transaction<'_>,
    ) -> StorageResult<usize> {
        let retention = revision_retention_from_connection(transaction)?;
        let retention_blob_hashes = all_revision_blob_hashes(transaction)?;
        let mut removed = prune_revisions_by_retention(transaction, &retention)?;
        if removed > 0 {
            ensure_unreferenced_revision_blob_ownership(transaction, &retention_blob_hashes)?;
        }
        removed += self.prune_revisions_to_storage_budget(transaction, &retention)?;
        Ok(removed)
    }
    pub(super) fn record_revision(
        &self,
        transaction: &Transaction<'_>,
        note: &Note,
        mode: RevisionMode,
    ) -> StorageResult<bool> {
        let latest: Option<(String, i64, String, i64)> = transaction
            .query_row(
                "SELECT id, revision_no, checksum, created_at
                 FROM note_revisions WHERE note_id = ?1
                 ORDER BY revision_no DESC LIMIT 1",
                [&note.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        let now = unix_timestamp();
        if mode == RevisionMode::Autosave
            && latest.as_ref().is_some_and(|(_, _, _, created_at)| {
                now.saturating_sub(*created_at) < AUTOSAVE_REVISION_INTERVAL_SECONDS
            })
        {
            return Ok(false);
        }

        let attachments =
            self.list_referenced_attachments_with_connection(transaction, &note.id)?;
        let snapshot = RevisionSnapshot::new(note, attachments);
        let snapshot_json = serde_json::to_string(&snapshot)?;
        let checksum = sha256_bytes(snapshot_json.as_bytes());
        let byte_size = i64::try_from(snapshot_json.len())
            .map_err(|_| StorageError::Validation("revision snapshot is too large".to_string()))?;
        if latest
            .as_ref()
            .is_some_and(|(_, _, latest_checksum, _)| latest_checksum == &checksum)
        {
            return Ok(false);
        }
        let revision_no = latest.map_or(1, |(_, revision_no, _, _)| revision_no + 1);
        let revision_id = new_id("rev");
        transaction.execute(
            "INSERT INTO note_revisions(
                id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                revision_id,
                note.id,
                revision_no,
                checksum,
                snapshot_json,
                byte_size,
                now
            ],
        )?;
        self.index_revision_attachment_refs(transaction, &revision_id, &snapshot.attachments)?;
        let retention = revision_retention_from_connection(transaction)?;
        self.prune_revisions_for_note(transaction, &note.id, &retention)?;
        self.prune_revisions_to_storage_budget(transaction, &retention)?;
        Ok(true)
    }
    pub(super) fn index_revision_attachment_refs(
        &self,
        transaction: &Transaction<'_>,
        revision_id: &str,
        attachments: &[RevisionAttachment],
    ) -> StorageResult<()> {
        transaction.execute(
            "DELETE FROM revision_attachment_refs WHERE revision_id = ?1",
            [revision_id],
        )?;
        for attachment in attachments {
            transaction.execute(
                "INSERT INTO revision_attachment_refs(
                    revision_id, attachment_id, sha256, byte_size
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    revision_id,
                    attachment.id,
                    attachment.sha256,
                    attachment.byte_size
                ],
            )?;
        }
        Ok(())
    }
    pub(super) fn prune_revisions_for_note(
        &self,
        transaction: &Transaction<'_>,
        note_id: &NoteId,
        retention: &RevisionRetention,
    ) -> StorageResult<usize> {
        // Any revision attachment reference removed below may be the final
        // durable database owner of an ordinary canonical plaintext blob.
        // Capture the candidate identities before the cascades, then bridge
        // any newly-unreferenced blob into the generic plaintext ledger before
        // the caller can commit this same transaction.
        let candidate_blob_hashes = revision_blob_hashes_for_note(transaction, note_id)?;
        let mut removed = 0_usize;

        if retention.max_age_days > 0 {
            let cutoff = unix_timestamp()
                .saturating_sub(retention.max_age_days.saturating_mul(24 * 60 * 60));
            removed += transaction.execute(
                "DELETE FROM note_revisions
                 WHERE note_id = ?1 AND created_at < ?2
                   AND id NOT IN (
                       SELECT id FROM note_revisions
                       WHERE note_id = ?1
                       ORDER BY revision_no DESC LIMIT 1
                   )",
                params![note_id, cutoff],
            )?;
        }

        removed += transaction.execute(
            "DELETE FROM note_revisions
             WHERE note_id = ?1 AND id NOT IN (
                 SELECT id FROM note_revisions
                 WHERE note_id = ?1
                 ORDER BY revision_no DESC
                 LIMIT ?2
             )",
            params![note_id, retention.max_revisions_per_note],
        )?;
        if removed > 0 {
            ensure_unreferenced_revision_blob_ownership(transaction, &candidate_blob_hashes)?;
        }
        Ok(removed)
    }
    pub(super) fn prune_revisions_to_storage_budget(
        &self,
        transaction: &Transaction<'_>,
        retention: &RevisionRetention,
    ) -> StorageResult<usize> {
        let usage = revision_storage_usage_from_connection(transaction)?;
        if usage.total_bytes <= retention.max_total_bytes {
            return Ok(0);
        }

        let mut candidates = revision_prune_candidates(transaction)?;
        if candidates.is_empty() {
            return Ok(0);
        }
        index_candidate_historical_blob_refs(transaction, &mut candidates)?;
        let mut historical_blobs = historical_blob_budgets(transaction)?;
        let (selected_revision_ids, projected_total_bytes) = select_revisions_for_budget(
            &candidates,
            &mut historical_blobs,
            usage.total_bytes,
            retention.max_total_bytes,
        )?;
        if selected_revision_ids.is_empty() {
            return Ok(0);
        }

        let candidate_blob_hashes =
            revision_blob_hashes_for_revision_ids(transaction, &selected_revision_ids)?;
        let removed = delete_revision_batches(transaction, &selected_revision_ids)?;
        ensure_unreferenced_revision_blob_ownership(transaction, &candidate_blob_hashes)?;
        let verified_usage = revision_storage_usage_from_connection(transaction)?;
        if verified_usage.total_bytes != projected_total_bytes {
            return Err(StorageError::Integrity(format!(
                "revision budget mismatch: projected {projected_total_bytes} bytes, measured {} bytes",
                verified_usage.total_bytes
            )));
        }
        Ok(removed)
    }
}

fn revision_blob_hashes_for_note(
    transaction: &Transaction<'_>,
    note_id: &NoteId,
) -> StorageResult<HashSet<String>> {
    let mut statement = transaction.prepare(
        "SELECT DISTINCT refs.sha256
         FROM revision_attachment_refs refs
         JOIN note_revisions revision ON revision.id = refs.revision_id
         WHERE revision.note_id = ?1",
    )?;
    let rows = statement.query_map([note_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<HashSet<_>, _>>()?)
}

fn revision_blob_hashes_for_revision_ids(
    transaction: &Transaction<'_>,
    revision_ids: &[String],
) -> StorageResult<HashSet<String>> {
    let mut hashes = HashSet::new();
    for batch in revision_ids.chunks(REVISION_DELETE_BATCH_SIZE) {
        let placeholders = vec!["?"; batch.len()].join(", ");
        let sql = format!(
            "SELECT DISTINCT sha256 FROM revision_attachment_refs
             WHERE revision_id IN ({placeholders})"
        );
        let mut statement = transaction.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(batch.iter()), |row| {
            row.get::<_, String>(0)
        })?;
        hashes.extend(rows.collect::<Result<HashSet<_>, _>>()?);
    }
    Ok(hashes)
}

fn all_revision_blob_hashes(transaction: &Transaction<'_>) -> StorageResult<HashSet<String>> {
    let mut statement =
        transaction.prepare("SELECT DISTINCT sha256 FROM revision_attachment_refs")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<HashSet<_>, _>>()?)
}

fn ensure_unreferenced_revision_blob_ownership(
    transaction: &Transaction<'_>,
    hashes: &HashSet<String>,
) -> StorageResult<()> {
    for sha256 in hashes {
        super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
            transaction,
            sha256,
        )?;
    }
    Ok(())
}

fn prune_revisions_by_retention(
    transaction: &Transaction<'_>,
    retention: &RevisionRetention,
) -> StorageResult<usize> {
    let mut removed = 0_usize;

    if retention.max_age_days > 0 {
        let cutoff =
            unix_timestamp().saturating_sub(retention.max_age_days.saturating_mul(24 * 60 * 60));
        removed += transaction.execute(
            "DELETE FROM note_revisions AS revision
             WHERE revision.created_at < ?1
               AND EXISTS (
                   SELECT 1 FROM note_revisions AS newer
                   WHERE newer.note_id = revision.note_id
                     AND newer.revision_no > revision.revision_no
               )",
            [cutoff],
        )?;
    }

    removed += transaction.execute(
        "DELETE FROM note_revisions
         WHERE id IN (
             SELECT id FROM (
                 SELECT id,
                        ROW_NUMBER() OVER (
                            PARTITION BY note_id
                            ORDER BY revision_no DESC
                        ) AS retention_rank
                 FROM note_revisions
             )
             WHERE retention_rank > ?1
         )",
        [retention.max_revisions_per_note],
    )?;
    Ok(removed)
}

fn revision_prune_candidates(
    connection: &Connection,
) -> StorageResult<Vec<RevisionPruneCandidate>> {
    let mut statement = connection.prepare(
        "SELECT r.id, r.byte_size
         FROM note_revisions r
         WHERE r.id NOT IN (
             SELECT newest.id
             FROM note_revisions newest
             WHERE newest.revision_no = (
                 SELECT MAX(candidate.revision_no)
                 FROM note_revisions candidate
                 WHERE candidate.note_id = newest.note_id
             )
         )
         ORDER BY r.created_at, r.revision_no, r.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RevisionPruneCandidate {
            id: row.get(0)?,
            snapshot_bytes: row.get(1)?,
            historical_blob_refs: Vec::new(),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn index_candidate_historical_blob_refs(
    connection: &Connection,
    candidates: &mut [RevisionPruneCandidate],
) -> StorageResult<()> {
    let candidate_indices = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut statement = connection.prepare(
        "SELECT refs.revision_id, refs.sha256, COUNT(*)
         FROM revision_attachment_refs refs
         WHERE NOT EXISTS (
             SELECT 1 FROM attachments live
             WHERE live.sha256 = refs.sha256
         )
         GROUP BY refs.revision_id, refs.sha256
         ORDER BY refs.revision_id, refs.sha256",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let revision_id = row.get::<_, String>(0)?;
        let Some(candidate_index) = candidate_indices.get(&revision_id).copied() else {
            continue;
        };
        let reference_count = row.get::<_, i64>(2)?;
        if reference_count <= 0 {
            return Err(StorageError::Integrity(format!(
                "revision {revision_id} has an invalid historical blob reference count"
            )));
        }
        candidates[candidate_index]
            .historical_blob_refs
            .push(RevisionBlobReference {
                sha256: row.get(1)?,
                reference_count,
            });
    }
    Ok(())
}

fn historical_blob_budgets(
    connection: &Connection,
) -> StorageResult<HashMap<String, HistoricalBlobBudget>> {
    let mut statement = connection.prepare(
        "SELECT refs.sha256, MAX(refs.byte_size), COUNT(*)
         FROM revision_attachment_refs refs
         WHERE NOT EXISTS (
             SELECT 1 FROM attachments live
             WHERE live.sha256 = refs.sha256
         )
         GROUP BY refs.sha256",
    )?;
    let mut rows = statement.query([])?;
    let mut budgets = HashMap::new();
    while let Some(row) = rows.next()? {
        let sha256 = row.get::<_, String>(0)?;
        let byte_size = row.get::<_, i64>(1)?;
        let remaining_references = row.get::<_, i64>(2)?;
        if byte_size < 0 || remaining_references <= 0 {
            return Err(StorageError::Integrity(format!(
                "historical blob {sha256} has invalid budget metadata"
            )));
        }
        budgets.insert(
            sha256,
            HistoricalBlobBudget {
                byte_size,
                remaining_references,
            },
        );
    }
    Ok(budgets)
}

fn select_revisions_for_budget(
    candidates: &[RevisionPruneCandidate],
    historical_blobs: &mut HashMap<String, HistoricalBlobBudget>,
    current_total_bytes: i64,
    max_total_bytes: i64,
) -> StorageResult<(Vec<String>, i64)> {
    let mut projected_total_bytes = current_total_bytes;
    let mut selected_revision_ids = Vec::new();

    for candidate in candidates {
        if projected_total_bytes <= max_total_bytes {
            break;
        }
        if candidate.snapshot_bytes < 0 {
            return Err(StorageError::Integrity(format!(
                "revision {} has a negative snapshot size",
                candidate.id
            )));
        }
        if candidate.snapshot_bytes > projected_total_bytes {
            return Err(StorageError::Integrity(format!(
                "revision budget underflow while selecting {}",
                candidate.id
            )));
        }
        projected_total_bytes -= candidate.snapshot_bytes;

        for reference in &candidate.historical_blob_refs {
            let blob = historical_blobs.get_mut(&reference.sha256).ok_or_else(|| {
                StorageError::Integrity(format!(
                    "revision {} references missing blob budget state for {}",
                    candidate.id, reference.sha256
                ))
            })?;
            if reference.reference_count > blob.remaining_references {
                return Err(StorageError::Integrity(format!(
                    "historical blob reference accounting underflow for {}",
                    reference.sha256
                )));
            }
            blob.remaining_references -= reference.reference_count;
            if blob.remaining_references == 0 {
                if blob.byte_size > projected_total_bytes {
                    return Err(StorageError::Integrity(format!(
                        "revision blob budget underflow for {}",
                        reference.sha256
                    )));
                }
                projected_total_bytes -= blob.byte_size;
            }
        }
        selected_revision_ids.push(candidate.id.clone());
    }

    Ok((selected_revision_ids, projected_total_bytes))
}

fn delete_revision_batches(
    transaction: &Transaction<'_>,
    revision_ids: &[String],
) -> StorageResult<usize> {
    let mut removed = 0_usize;
    for batch in revision_ids.chunks(REVISION_DELETE_BATCH_SIZE) {
        let placeholders = vec!["?"; batch.len()].join(", ");
        let sql = format!("DELETE FROM note_revisions WHERE id IN ({placeholders})");
        let batch_removed = transaction.execute(&sql, params_from_iter(batch.iter()))?;
        if batch_removed != batch.len() {
            return Err(StorageError::Integrity(format!(
                "revision budget pruning selected {} rows but removed {batch_removed}",
                batch.len()
            )));
        }
        removed = removed
            .checked_add(batch_removed)
            .ok_or_else(|| StorageError::Integrity("revision prune count overflow".to_string()))?;
    }
    Ok(removed)
}

pub(super) fn materialize_revision_note(snapshot: &RevisionSnapshot) -> StorageResult<Note> {
    let projections =
        derive_editor_projections(snapshot.note.editor_state.as_str()).map_err(|error| {
            StorageError::Validation(format!(
                "could not regenerate revision projections: {error}"
            ))
        })?;
    Ok(Note {
        id: snapshot.note.id.clone(),
        protection: crate::domain::NoteProtectionState::Unprotected,
        notebook_id: snapshot.note.notebook_id.clone(),
        title: snapshot.note.title.clone(),
        title_style: snapshot.note.title_style.clone().normalized(),
        document: std::sync::Arc::new(crate::domain::NoteDocument::new(
            projections.markdown,
            snapshot.note.editor_state.clone(),
            projections.plain_text,
        )),
        tags: snapshot.note.tags.clone(),
        attachment_names: snapshot
            .attachments
            .iter()
            .map(|attachment| attachment.file_name.clone())
            .collect(),
        pinned: snapshot.note.pinned,
        archived: snapshot.note.archived,
        trashed: snapshot.note.trashed,
        purge_after: snapshot.note.purge_after,
        created_at: snapshot.note.created_at,
        updated_at: 0,
    })
}

pub(super) fn revision_attachment_to_attachment(
    note_id: &NoteId,
    attachment: &RevisionAttachment,
) -> StorageResult<Attachment> {
    validate_sha256(&attachment.sha256)?;
    if attachment.byte_size < 0 {
        return Err(StorageError::Validation(
            "revision attachment has a negative size".to_string(),
        ));
    }
    Ok(Attachment {
        id: attachment.id.clone(),
        note_id: note_id.clone(),
        file_name: attachment.file_name.clone(),
        mime_type: attachment.mime_type.clone(),
        byte_size: attachment.byte_size,
        sha256: attachment.sha256.clone(),
        image_pixel_width: attachment.image_pixel_width,
        image_pixel_height: attachment.image_pixel_height,
        image_format: attachment.image_format.clone(),
        image_orientation: attachment.image_orientation,
        image_has_alpha: attachment.image_has_alpha,
        image_animated: attachment.image_animated,
        source_attachment_id: attachment.source_attachment_id.clone(),
        image_edit_recipe: attachment.image_edit_recipe.clone(),
        embedded: true,
        unreferenced_at: None,
        created_at: attachment.created_at,
        updated_at: unix_timestamp(),
    })
}

pub(super) fn revision_retention_from_connection(
    connection: &Connection,
) -> StorageResult<RevisionRetention> {
    let defaults = RevisionRetention::default();
    let (max_revisions, max_age, max_bytes): (Option<String>, Option<String>, Option<String>) =
        connection.query_row(
            "SELECT
                 MAX(CASE WHEN key = ?1 THEN value END),
                 MAX(CASE WHEN key = ?2 THEN value END),
                 MAX(CASE WHEN key = ?3 THEN value END)
             FROM settings
             WHERE key IN (?1, ?2, ?3)",
            params![REVISION_MAX_KEY, REVISION_AGE_KEY, REVISION_BYTES_KEY],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

    let max_revisions_per_note = max_revisions
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| (10..=2_000).contains(value))
        .unwrap_or(defaults.max_revisions_per_note);
    let max_age_days = max_age
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| (0..=3_650).contains(value))
        .unwrap_or(defaults.max_age_days);
    let max_total_bytes = max_bytes
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| (16_i64 * 1024 * 1024..=10_i64 * 1024 * 1024 * 1024).contains(value))
        .unwrap_or(defaults.max_total_bytes);
    Ok(RevisionRetention {
        max_revisions_per_note,
        max_age_days,
        max_total_bytes,
    })
}

pub(super) fn revision_storage_usage_from_connection(
    connection: &Connection,
) -> StorageResult<RevisionStorageUsage> {
    let revision_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM note_revisions", [], |row| row.get(0))?;
    let snapshot_bytes: i64 = connection.query_row(
        "SELECT COALESCE(SUM(byte_size), 0) FROM note_revisions",
        [],
        |row| row.get(0),
    )?;
    let historical_blob_bytes: i64 = connection.query_row(
        "SELECT COALESCE(SUM(byte_size), 0)
         FROM (
             SELECT sha256, MAX(byte_size) AS byte_size
             FROM revision_attachment_refs
             WHERE sha256 NOT IN (SELECT sha256 FROM attachments)
             GROUP BY sha256
         )",
        [],
        |row| row.get(0),
    )?;
    Ok(RevisionStorageUsage {
        revision_count,
        total_bytes: snapshot_bytes.saturating_add(historical_blob_bytes),
    })
}

pub(super) fn revision_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<NoteRevisionSummary> {
    Ok(NoteRevisionSummary {
        id: row.get(0)?,
        note_id: row.get(1)?,
        revision_no: row.get(2)?,
        byte_size: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub(super) fn revision_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteRevision> {
    Ok(NoteRevision {
        id: row.get(0)?,
        note_id: row.get(1)?,
        revision_no: row.get(2)?,
        checksum: row.get(3)?,
        snapshot_json: row.get(4)?,
        byte_size: row.get(5)?,
        created_at: row.get(6)?,
    })
}

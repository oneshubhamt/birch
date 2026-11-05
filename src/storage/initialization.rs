use std::fs;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::domain::{DEFAULT_NOTEBOOK_ID, DEFAULT_NOTEBOOK_NAME, Note, NoteTitleStyle};

use super::schema::{BASE_SCHEMA, CURRENT_SCHEMA_VERSION, canonicalize_schema_sql};
use super::{
    DeferredMaintenanceReport, RevisionMode, Storage, StorageError, StorageResult, unix_timestamp,
};

pub(super) const EXAMPLE_NOTE_ID: &str = "note_welcome";

const WELCOME_EDITOR_STATE: &str = include_str!("../../assets/welcome-note-editor-state.json");
const WELCOME_MARKDOWN: &str = include_str!("../../assets/welcome-note.md");
const WELCOME_PLAIN_TEXT: &str = include_str!("../../assets/welcome-note.txt");

const NOTE_RUNTIME_INDEXES: &[(&str, &str)] = &[
    (
        "idx_notes_list_global_updated_newest",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_global_updated_newest
             ON notes(trashed, archived, pinned DESC, updated_at DESC, id DESC);",
    ),
    (
        "idx_notes_list_global_updated_oldest",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_global_updated_oldest
             ON notes(trashed, archived, pinned DESC, updated_at ASC, id ASC);",
    ),
    (
        "idx_notes_list_global_created_newest",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_global_created_newest
             ON notes(trashed, archived, pinned DESC, created_at DESC, id DESC);",
    ),
    (
        "idx_notes_list_global_title_ascending",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_global_title_ascending
             ON notes(
                 trashed, archived, pinned DESC,
                 LOWER(CASE WHEN TRIM(title) = '' THEN 'Untitled note' ELSE TRIM(title) END) ASC,
                 id ASC
             );",
    ),
    (
        "idx_notes_list_global_title_descending",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_global_title_descending
             ON notes(
                 trashed, archived, pinned DESC,
                 LOWER(CASE WHEN TRIM(title) = '' THEN 'Untitled note' ELSE TRIM(title) END) DESC,
                 id DESC
             );",
    ),
    (
        "idx_notes_list_notebook_updated_newest",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_notebook_updated_newest
             ON notes(trashed, archived, notebook_id, pinned DESC, updated_at DESC, id DESC);",
    ),
    (
        "idx_notes_list_notebook_updated_oldest",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_notebook_updated_oldest
             ON notes(trashed, archived, notebook_id, pinned DESC, updated_at ASC, id ASC);",
    ),
    (
        "idx_notes_list_notebook_created_newest",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_notebook_created_newest
             ON notes(trashed, archived, notebook_id, pinned DESC, created_at DESC, id DESC);",
    ),
    (
        "idx_notes_list_notebook_title_ascending",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_notebook_title_ascending
             ON notes(
                 trashed, archived, notebook_id, pinned DESC,
                 LOWER(CASE WHEN TRIM(title) = '' THEN 'Untitled note' ELSE TRIM(title) END) ASC,
                 id ASC
             );",
    ),
    (
        "idx_notes_list_notebook_title_descending",
        "CREATE INDEX IF NOT EXISTS idx_notes_list_notebook_title_descending
             ON notes(
                 trashed, archived, notebook_id, pinned DESC,
                 LOWER(CASE WHEN TRIM(title) = '' THEN 'Untitled note' ELSE TRIM(title) END) DESC,
                 id DESC
             );",
    ),
    (
        "idx_notes_active_updated",
        "CREATE INDEX IF NOT EXISTS idx_notes_active_updated
             ON notes(trashed, updated_at DESC, id DESC);",
    ),
    (
        "idx_notes_trash_expiry",
        "CREATE INDEX IF NOT EXISTS idx_notes_trash_expiry
             ON notes(purge_after, id)
             WHERE trashed = 1 AND purge_after IS NOT NULL;",
    ),
    (
        "idx_notes_notebook_fk",
        "CREATE INDEX IF NOT EXISTS idx_notes_notebook_fk ON notes(notebook_id);",
    ),
];

pub(super) fn is_note_runtime_index(name: &str) -> bool {
    NOTE_RUNTIME_INDEXES
        .iter()
        .any(|(index_name, _)| *index_name == name)
}

fn canonical_runtime_index_sql(create_sql: &str) -> String {
    canonicalize_schema_sql(&create_sql.replacen(" IF NOT EXISTS", "", 1))
}

impl Storage {
    pub fn schema_version(&self) -> StorageResult<i64> {
        let connection = self.connection()?;
        Ok(connection.query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    pub(super) fn initialize(&self) -> StorageResult<()> {
        fs::create_dir_all(self.attachments_directory.join(".incoming"))?;
        fs::create_dir_all(self.attachments_directory.join(".orphaned"))?;

        // Durable cache namespace markers are established before the library is
        // opened to any producer or reader. Existing derived files therefore
        // acquire one authoritative cleanup namespace before they can be served,
        // and fresh libraries satisfy owner-before-plaintext from first use.
        crate::native::rendition::persistent_rendition_owner(
            &crate::library_paths::image_rendition_directory(&self.data_directory),
        )?;
        crate::media_compat::persistent_cache_owner(
            &crate::library_paths::media_compat_directory(&self.data_directory),
        )?;

        let mut connection = self.connection()?;
        // WAL mode is persistent database state. Establish it once at startup
        // rather than asking SQLite to renegotiate it on every short-lived connection.
        connection.execute_batch("PRAGMA journal_mode = WAL;")?;
        self.initialize_schema(&mut connection)?;
        drop(connection);
        // 2a.16: any library-resident plaintext staging that survived a crash
        // remains durably owned in SQLite. Remove it before normal recovery or
        // UI startup can proceed; deletion failure is a startup-blocking error.
        self.recover_managed_plaintext_staging_unlocked()?;
        // Current attachment staging must be claimed by either the durable
        // plaintext ledger or an attachment-operation row. Remove anything
        // outside those authorities before the library is allowed to open.
        self.cleanup_untracked_incoming_files(&self.list_attachment_operations()?)?;
        // Vault transitions span SQLite and the attachment filesystem. Recover
        // any operation interrupted by a hard crash before integrity checks can
        // observe an intentionally staged/missing blob.
        self.recover_vault_operations_unlocked()?;
        // After every durable recovery authority has run, prove
        // that no managed attachment plaintext exists without an owner. This is
        // intentionally verification-only; startup must not legitimize an
        // already-existing plaintext file by creating ownership after the fact.
        self.verify_managed_attachment_plaintext_ownership_unlocked()?;
        let mut connection = self.connection()?;
        self.verify_integrity_with_connection(&connection)?;
        super::settings::record_build_diagnostics(&connection)?;

        let notebook_count: i64 =
            connection.query_row("SELECT COUNT(*) FROM notebooks", [], |row| row.get(0))?;
        if notebook_count == 0 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            self.seed(&transaction)?;
            transaction.commit()?;
        }

        self.install_runtime_database_aids(&mut connection)?;
        drop(connection);

        Ok(())
    }

    pub(super) fn install_runtime_database_aids(
        &self,
        connection: &mut Connection,
    ) -> StorageResult<()> {
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (name, create_sql) in NOTE_RUNTIME_INDEXES {
            let existing = transaction
                .query_row(
                    "SELECT type, COALESCE(sql, '') FROM sqlite_schema WHERE name = ?1",
                    [*name],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;

            match existing {
                None => transaction.execute_batch(create_sql)?,
                Some((object_type, _)) if object_type != "index" => {
                    return Err(StorageError::Integrity(format!(
                        "runtime database aid {name} conflicts with a {object_type}"
                    )));
                }
                Some((_, existing_sql))
                    if canonicalize_schema_sql(&existing_sql)
                        == canonical_runtime_index_sql(create_sql) => {}
                Some(_) => {
                    // Runtime indexes are explicitly rebuildable. Repair only a
                    // mismatched definition so normal startup never churns the
                    // large note indexes unnecessarily.
                    transaction.execute_batch(&format!(
                        "DROP INDEX {name};\n{create_sql}"
                    ))?;
                }
            }
        }
        transaction.commit()?;

        // This connection may be new or may have just received a restored database.
        // Consider every table when deciding whether planner statistics need refresh;
        // SQLite bounds the ANALYZE work performed by PRAGMA optimize.
        connection.execute_batch("PRAGMA optimize=0x10002;")?;
        Ok(())
    }

    pub fn run_deferred_maintenance(&self) -> StorageResult<DeferredMaintenanceReport> {
        // Trash purge acquires its own note-scoped PrivacyCoordinator mutation
        // guard and therefore must begin outside an already-held storage lock.
        // Keep the other maintenance
        // phases serialized, release the lock for purge, then reacquire it for
        // the remaining database/filesystem maintenance.
        let (backup_recovery, scheduled_trash_deletions) = {
            let _operation_guard = self.lock_operations()?;
            (
                self.reconcile_backup_repository_unlocked()?,
                self.reconcile_trash_retention_unlocked()?,
            )
        };
        let purged_expired_notes = self.purge_expired_notes()?;
        let ordinary_pruned_revisions = {
            let _operation_guard = self.lock_operations()?;
            self.prune_revision_history_unlocked()?
        };
        // Protected retention is explicit/deferred rather than an autosave cost.
        // Acquire Vault readers before the storage lock inside the protected
        // maintenance path so Lock ordering remains canonical.
        let protected_pruned_revisions = self.prune_unlocked_protected_revision_history()?;
        let pruned_revisions = ordinary_pruned_revisions.saturating_add(protected_pruned_revisions);
        let attachment_recovery = self.reconcile_attachment_storage()?;
        Ok(DeferredMaintenanceReport {
            scheduled_trash_deletions,
            purged_expired_notes,
            pruned_revisions,
            attachment_recovery,
            backup_recovery,
        })
    }

    pub(super) fn initialize_schema(&self, connection: &mut Connection) -> StorageResult<()> {
        let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        match current {
            0 => {
                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                transaction.execute_batch(BASE_SCHEMA)?;
                transaction.commit()?;
                Ok(())
            }
            CURRENT_SCHEMA_VERSION => Ok(()),
            other => Err(StorageError::Validation(format!(
                "pre-release database schema {other} is unsupported by this Rustnote build \
                 ({CURRENT_SCHEMA_VERSION}); remove the development data directory and restart"
            ))),
        }
    }

    pub(super) fn seed(&self, transaction: &Transaction<'_>) -> StorageResult<()> {
        let now = unix_timestamp();
        transaction.execute(
            "INSERT INTO notebooks(id, name, parent_id, sort_order, created_at, updated_at)
             VALUES (?1, ?2, NULL, 0, ?3, ?3)",
            params![DEFAULT_NOTEBOOK_ID, DEFAULT_NOTEBOOK_NAME, now],
        )?;

        let example = Note {
            id: crate::domain::NoteId::parse(EXAMPLE_NOTE_ID)
                .expect("built-in example note identifier must be valid"),
            protection: crate::domain::NoteProtectionState::Unprotected,
            notebook_id: DEFAULT_NOTEBOOK_ID.to_string(),
            title: "Welcome to Rustnote".to_string(),
            title_style: NoteTitleStyle::default(),
            document: std::sync::Arc::new(crate::domain::NoteDocument::new(
                WELCOME_MARKDOWN.trim_end().to_string(),
                std::sync::Arc::new(WELCOME_EDITOR_STATE.trim().to_string()),
                WELCOME_PLAIN_TEXT.trim_end().to_string(),
            )),
            tags: vec!["example".to_string(), "getting-started".to_string()],
            attachment_names: Vec::new(),
            pinned: false,
            archived: false,
            trashed: false,
            purge_after: None,
            created_at: now,
            updated_at: now,
        };
        let (example, _) = self
            .persist_note(transaction, None, example, None)
            .map_err(|failure| failure.error)?;
        self.record_revision(transaction, &example, RevisionMode::Checkpoint)?;
        Ok(())
    }
}

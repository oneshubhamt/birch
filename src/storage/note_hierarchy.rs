use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::domain::{HierarchyNoteSummary, NoteHierarchy, NoteId};

use super::{Storage, StorageError, StorageResult};

impl Storage {
    pub fn note_hierarchy(&self, note_id: &NoteId) -> StorageResult<NoteHierarchy> {
        let connection = self.connection()?;
        let exists = connection
            .query_row("SELECT 1 FROM notes WHERE id = ?1", [note_id], |_| Ok(()))
            .optional()?
            .is_some();
        if !exists {
            return Err(StorageError::Validation(
                "note no longer exists".to_string(),
            ));
        }

        let parent = connection
            .query_row(
                "SELECT p.id, p.title, b.name, p.archived, p.trashed
                 FROM note_parent h
                 JOIN notes p ON p.id = h.parent_note_id
                 JOIN notebooks b ON b.id = p.notebook_id
                 WHERE h.child_note_id = ?1",
                [note_id],
                hierarchy_note_row,
            )
            .optional()?;

        let mut statement = connection.prepare(
            "SELECT c.id, c.title, b.name, c.archived, c.trashed
             FROM note_parent h
             JOIN notes c ON c.id = h.child_note_id
             JOIN notebooks b ON b.id = c.notebook_id
             WHERE h.parent_note_id = ?1
             ORDER BY c.trashed ASC, c.archived ASC,
                      LOWER(CASE WHEN TRIM(c.title) = '' THEN 'Untitled note' ELSE TRIM(c.title) END),
                      c.id",
        )?;
        let rows = statement.query_map([note_id], hierarchy_note_row)?;
        let children = rows.collect::<Result<Vec<_>, _>>()?;

        Ok(NoteHierarchy { parent, children })
    }

    /// Set or replace a note's single parent relationship.
    ///
    /// Notebook membership and note content are deliberately untouched. The
    /// transaction rejects self-parenting and any assignment that would make
    /// the parent chain cycle back to the child.
    pub fn set_note_parent(
        &self,
        child_note_id: &NoteId,
        parent_note_id: &NoteId,
    ) -> StorageResult<()> {
        if child_note_id == parent_note_id {
            return Err(StorageError::Validation(
                "a note cannot be its own parent".to_string(),
            ));
        }

        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_hierarchy_endpoint_is_editable(&transaction, child_note_id, "child")?;
        ensure_hierarchy_endpoint_is_editable(&transaction, parent_note_id, "parent")?;

        if hierarchy_assignment_would_cycle(&transaction, child_note_id, parent_note_id)? {
            return Err(StorageError::Validation(
                "that parent would create a circular note hierarchy".to_string(),
            ));
        }

        transaction.execute(
            "INSERT INTO note_parent(child_note_id, parent_note_id)
             VALUES (?1, ?2)
             ON CONFLICT(child_note_id) DO UPDATE SET parent_note_id = excluded.parent_note_id
             WHERE note_parent.parent_note_id <> excluded.parent_note_id",
            params![child_note_id, parent_note_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_note_parent(&self, child_note_id: &NoteId) -> StorageResult<()> {
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_hierarchy_endpoint_is_editable(&transaction, child_note_id, "note")?;
        transaction.execute(
            "DELETE FROM note_parent WHERE child_note_id = ?1",
            [child_note_id],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn hierarchy_note_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HierarchyNoteSummary> {
    Ok(HierarchyNoteSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        notebook_name: row.get(2)?,
        archived: row.get::<_, i64>(3)? != 0,
        trashed: row.get::<_, i64>(4)? != 0,
    })
}

fn ensure_hierarchy_endpoint_is_editable(
    transaction: &Transaction<'_>,
    note_id: &NoteId,
    role: &str,
) -> StorageResult<()> {
    let trashed = transaction
        .query_row(
            "SELECT trashed FROM notes WHERE id = ?1",
            [note_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    match trashed {
        None => Err(StorageError::Validation(format!(
            "the selected {role} note no longer exists"
        ))),
        Some(value) if value != 0 => Err(StorageError::Validation(format!(
            "the selected {role} note is in Trash"
        ))),
        Some(_) => Ok(()),
    }
}

fn hierarchy_assignment_would_cycle(
    transaction: &Transaction<'_>,
    child_note_id: &NoteId,
    parent_note_id: &NoteId,
) -> StorageResult<bool> {
    let would_cycle = transaction.query_row(
        "WITH RECURSIVE ancestors(note_id) AS (
             SELECT ?1
             UNION
             SELECT h.parent_note_id
             FROM note_parent h
             JOIN ancestors a ON h.child_note_id = a.note_id
         )
         SELECT EXISTS(SELECT 1 FROM ancestors WHERE note_id = ?2)",
        params![parent_note_id, child_note_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(would_cycle != 0)
}

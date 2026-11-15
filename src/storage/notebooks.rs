use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::domain::{DEFAULT_NOTEBOOK_ID, Notebook};

use super::backup_support::sanitize_component;
use super::{Storage, StorageError, StorageResult, new_id, unix_timestamp};

impl Storage {
    pub fn list_notebooks(&self) -> StorageResult<Vec<Notebook>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, parent_id, sort_order, created_at, updated_at
             FROM notebooks
             ORDER BY parent_id IS NOT NULL, sort_order, lower(name)",
        )?;
        let rows = statement.query_map([], notebook_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn create_notebook(&self, name: &str, parent_id: Option<&str>) -> StorageResult<Notebook> {
        let _operation_guard = self.lock_operations()?;
        let cleaned_name = name.trim();
        if cleaned_name.is_empty() {
            return Err(StorageError::Validation(
                "notebook name cannot be empty".to_string(),
            ));
        }
        let now = unix_timestamp();
        let notebook = Notebook {
            id: new_id("book"),
            name: cleaned_name.to_string(),
            parent_id: parent_id.map(str::to_string),
            sort_order: now,
            created_at: now,
            updated_at: now,
        };
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO notebooks(id, name, parent_id, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                notebook.id,
                notebook.name,
                notebook.parent_id,
                notebook.sort_order,
                notebook.created_at,
                notebook.updated_at
            ],
        )?;
        self.enqueue_sync_upsert(&transaction, "notebook", &notebook.id, &notebook)?;
        transaction.commit()?;
        Ok(notebook)
    }

    pub fn rename_notebook(&self, notebook_id: &str, name: &str) -> StorageResult<Notebook> {
        let _operation_guard = self.lock_operations()?;
        let cleaned_name = name.trim();
        if cleaned_name.is_empty() {
            return Err(StorageError::Validation(
                "notebook name cannot be empty".to_string(),
            ));
        }

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut notebook = transaction
            .query_row(
                "SELECT id, name, parent_id, sort_order, created_at, updated_at
                 FROM notebooks WHERE id = ?1",
                [notebook_id],
                notebook_from_row,
            )
            .optional()?
            .ok_or_else(|| StorageError::Validation("notebook no longer exists".to_string()))?;

        if notebook.name == cleaned_name {
            return Ok(notebook);
        }

        notebook.name = cleaned_name.to_string();
        notebook.updated_at = unix_timestamp();
        transaction.execute(
            "UPDATE notebooks SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![notebook.id, notebook.name, notebook.updated_at],
        )?;
        self.enqueue_sync_upsert(&transaction, "notebook", &notebook.id, &notebook)?;
        transaction.commit()?;
        Ok(notebook)
    }

    pub fn move_notebook(
        &self,
        notebook_id: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Notebook> {
        let _operation_guard = self.lock_operations()?;
        if notebook_id == DEFAULT_NOTEBOOK_ID {
            return Err(StorageError::Validation(
                "the default Notes notebook cannot be moved".to_string(),
            ));
        }
        if parent_id == Some(notebook_id) {
            return Err(StorageError::Validation(
                "a notebook cannot contain itself".to_string(),
            ));
        }

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut notebook = transaction
            .query_row(
                "SELECT id, name, parent_id, sort_order, created_at, updated_at
                 FROM notebooks WHERE id = ?1",
                [notebook_id],
                notebook_from_row,
            )
            .optional()?
            .ok_or_else(|| StorageError::Validation("notebook no longer exists".to_string()))?;

        if let Some(target_parent_id) = parent_id {
            let mut current = Some(target_parent_id.to_string());
            let mut visited = HashSet::new();
            while let Some(current_id) = current {
                if current_id == notebook_id {
                    return Err(StorageError::Validation(
                        "a notebook cannot be moved inside one of its descendants".to_string(),
                    ));
                }
                if !visited.insert(current_id.clone()) {
                    return Err(StorageError::Integrity(
                        "notebook hierarchy contains a cycle".to_string(),
                    ));
                }
                current = transaction
                    .query_row(
                        "SELECT parent_id FROM notebooks WHERE id = ?1",
                        [&current_id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()?
                    .ok_or_else(|| {
                        StorageError::Validation("target notebook no longer exists".to_string())
                    })?;
            }
        }

        let next_parent_id = parent_id.map(str::to_string);
        if notebook.parent_id == next_parent_id {
            return Ok(notebook);
        }

        notebook.parent_id = next_parent_id;
        notebook.updated_at = unix_timestamp();
        transaction.execute(
            "UPDATE notebooks SET parent_id = ?2, updated_at = ?3 WHERE id = ?1",
            params![notebook.id, notebook.parent_id, notebook.updated_at],
        )?;
        self.enqueue_sync_upsert(&transaction, "notebook", &notebook.id, &notebook)?;
        transaction.commit()?;
        Ok(notebook)
    }

    /// Delete a notebook without deleting user content.
    ///
    /// Direct notes are moved to the deleted notebook's parent, or to the
    /// built-in Notes notebook when deleting a top-level notebook. Direct child
    /// notebooks are reparented to the same destination before deletion.
    pub fn delete_notebook_preserving_contents(&self, notebook_id: &str) -> StorageResult<String> {
        let _operation_guard = self.lock_operations()?;
        if notebook_id == DEFAULT_NOTEBOOK_ID {
            return Err(StorageError::Validation(
                "the default Notes notebook cannot be deleted".to_string(),
            ));
        }

        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let notebook = transaction
            .query_row(
                "SELECT id, name, parent_id, sort_order, created_at, updated_at
                 FROM notebooks WHERE id = ?1",
                [notebook_id],
                notebook_from_row,
            )
            .optional()?
            .ok_or_else(|| StorageError::Validation("notebook no longer exists".to_string()))?;
        let replacement_notebook_id = notebook
            .parent_id
            .clone()
            .unwrap_or_else(|| DEFAULT_NOTEBOOK_ID.to_string());
        let now = unix_timestamp();

        #[cfg(feature = "remote-sync")]
        let moved_note_ids = {
            let mut statement =
                transaction.prepare("SELECT id FROM notes WHERE notebook_id = ?1")?;
            statement
                .query_map([notebook_id], |row| row.get::<_, crate::domain::NoteId>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };

        transaction.execute(
            "UPDATE notes SET notebook_id = ?2, updated_at = ?3 WHERE notebook_id = ?1",
            params![notebook_id, replacement_notebook_id, now],
        )?;

        #[cfg(feature = "remote-sync")]
        for note_id in moved_note_ids {
            let note = transaction.query_row(
                "SELECT
                    n.id, n.notebook_id, n.title, n.title_style, n.body, n.editor_state, n.plain_text,
                    n.pinned, n.archived, n.trashed, n.purge_after, n.created_at, n.updated_at,
                    COALESCE((
                        SELECT group_concat(t.name, char(31))
                        FROM note_tags nt
                        JOIN tags t ON t.id = nt.tag_id
                        WHERE nt.note_id = n.id
                    ), '') AS tag_names,
                    COALESCE((
                        SELECT group_concat(a.file_name, char(31))
                        FROM attachments a
                        WHERE a.note_id = n.id AND a.embedded = 1
                    ), '') AS attachment_names
                 FROM notes n
                 WHERE n.id = ?1",
                [&note_id],
                super::notes::note_from_row,
            )?;
            self.enqueue_sync_upsert(&transaction, "note", note.id.as_str(), &note)?;
        }

        let mut child_notebooks = {
            let mut statement = transaction.prepare(
                "SELECT id, name, parent_id, sort_order, created_at, updated_at
                 FROM notebooks WHERE parent_id = ?1",
            )?;
            statement
                .query_map([notebook_id], notebook_from_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        for child in &mut child_notebooks {
            child.parent_id = notebook.parent_id.clone();
            child.updated_at = now;
            transaction.execute(
                "UPDATE notebooks SET parent_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![child.id, child.parent_id, child.updated_at],
            )?;
            self.enqueue_sync_upsert(&transaction, "notebook", &child.id, child)?;
        }

        transaction.execute("DELETE FROM notebooks WHERE id = ?1", [notebook_id])?;
        self.enqueue_sync_delete(&transaction, "notebook", notebook_id)?;
        transaction.commit()?;
        Ok(replacement_notebook_id)
    }
}

pub(super) fn notebook_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Notebook> {
    Ok(Notebook {
        id: row.get(0)?,
        name: row.get(1)?,
        parent_id: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub(super) fn notebook_export_paths(
    notebooks: &[Notebook],
) -> StorageResult<HashMap<String, PathBuf>> {
    fn resolve(
        id: &str,
        notebooks: &[Notebook],
        cache: &mut HashMap<String, PathBuf>,
        visiting: &mut HashSet<String>,
    ) -> StorageResult<PathBuf> {
        if let Some(path) = cache.get(id) {
            return Ok(path.clone());
        }
        if !visiting.insert(id.to_string()) {
            return Err(StorageError::Integrity(
                "notebook cycle detected during export".to_string(),
            ));
        }
        let notebook = notebooks
            .iter()
            .find(|notebook| notebook.id == id)
            .ok_or_else(|| StorageError::Integrity("missing notebook during export".to_string()))?;
        let mut path = match notebook.parent_id.as_deref() {
            Some(parent) => resolve(parent, notebooks, cache, visiting)?,
            None => PathBuf::new(),
        };
        path.push(format!(
            "{}-{}",
            sanitize_component(&notebook.name),
            notebook
                .id
                .chars()
                .rev()
                .take(6)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
        visiting.remove(id);
        cache.insert(id.to_string(), path.clone());
        Ok(path)
    }

    let mut cache = HashMap::new();
    for notebook in notebooks {
        resolve(&notebook.id, notebooks, &mut cache, &mut HashSet::new())?;
    }
    Ok(cache)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::domain::DEFAULT_NOTEBOOK_ID;

    use super::*;

    fn test_storage(label: &str) -> (Storage, PathBuf) {
        let suffix = new_id(label);
        let root = std::env::temp_dir().join(format!("rustnote-{suffix}"));
        let data = root.join("data");
        let backups = root.join("backups");
        let storage =
            Storage::at_with_backup(data, backups).expect("test storage should initialize");
        (storage, root)
    }

    fn cleanup(storage: Storage, root: PathBuf) {
        drop(storage);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn notebook_mutations_preserve_hierarchy_and_note_contents() {
        let (storage, root) = test_storage("notebook-mutations");
        let parent = storage
            .create_notebook("Parent", None)
            .expect("parent notebook");
        let child = storage
            .create_notebook("Child", Some(&parent.id))
            .expect("child notebook");
        let note = storage
            .create_note(Some(&parent.id))
            .expect("note in parent notebook");

        let renamed = storage
            .rename_notebook(&parent.id, "Renamed")
            .expect("rename notebook");
        assert_eq!(renamed.name, "Renamed");

        let moved = storage
            .move_notebook(&child.id, None)
            .expect("move child to root");
        assert_eq!(moved.parent_id, None);
        storage
            .move_notebook(&child.id, Some(&parent.id))
            .expect("move child back");

        let replacement = storage
            .delete_notebook_preserving_contents(&parent.id)
            .expect("delete parent notebook safely");
        assert_eq!(replacement, DEFAULT_NOTEBOOK_ID);
        assert_eq!(
            storage
                .get_note(&note.id)
                .expect("note lookup")
                .expect("note should remain")
                .notebook_id,
            DEFAULT_NOTEBOOK_ID
        );
        assert!(
            storage
                .list_notebooks()
                .expect("notebook list")
                .iter()
                .all(|notebook| notebook.id != parent.id)
        );
        let child_after = storage
            .list_notebooks()
            .expect("notebook list")
            .into_iter()
            .find(|notebook| notebook.id == child.id)
            .expect("child should remain");
        assert_eq!(child_after.parent_id, None);

        cleanup(storage, root);
    }

    #[test]
    fn notebook_move_rejects_descendant_cycles() {
        let (storage, root) = test_storage("notebook-cycle");
        let parent = storage
            .create_notebook("Parent", None)
            .expect("parent notebook");
        let child = storage
            .create_notebook("Child", Some(&parent.id))
            .expect("child notebook");

        assert!(storage.move_notebook(&parent.id, Some(&child.id)).is_err());
        let parent_after = storage
            .list_notebooks()
            .expect("notebook list")
            .into_iter()
            .find(|notebook| notebook.id == parent.id)
            .expect("parent should remain");
        assert_eq!(parent_after.parent_id, None);

        cleanup(storage, root);
    }

    #[test]
    fn default_notebook_cannot_be_moved_or_deleted() {
        let (storage, root) = test_storage("default-notebook-protection");
        assert!(storage.move_notebook(DEFAULT_NOTEBOOK_ID, None).is_err());
        assert!(
            storage
                .delete_notebook_preserving_contents(DEFAULT_NOTEBOOK_ID)
                .is_err()
        );
        cleanup(storage, root);
    }
}

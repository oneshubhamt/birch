use std::{collections::HashSet, sync::MutexGuard};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    domain::{Note, NoteId},
    security::{ProtectedNotesMutationGuard, VaultNoteAccess},
};

use super::{Storage, StorageError, StorageResult};

impl Storage {
    pub fn list_tags(&self) -> StorageResult<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT DISTINCT t.name
             FROM tags t
             JOIN note_tags nt ON nt.tag_id = t.id
             ORDER BY lower(t.name)",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Rename a tag everywhere it is used. Protected note relationships stay
    /// exclusively inside encrypted MetadataState; the operation fails closed
    /// unless every protected note can be authorized for the same transaction.
    pub fn rename_tag(&self, current_name: &str, new_name: &str) -> StorageResult<usize> {
        let current_name = cleaned_tag_name(current_name)?;
        let new_name = cleaned_tag_name(new_name)?;
        let protected_guard = self.begin_all_protected_notes_tag_mutation()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let source_id = transaction
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                [&current_name],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let ordinary_note_ids = match source_id.as_deref() {
            Some(source_id) => note_ids_for_tag(&transaction, source_id)?,
            None => Vec::new(),
        };

        let mut protected_affected = 0_usize;
        for note_id in protected_note_ids(&transaction)? {
            let access = protected_guard.access_for(&note_id).ok_or_else(|| {
                StorageError::Integrity(
                    "protected tag mutation authorization set changed unexpectedly".to_string(),
                )
            })?;
            let changed = self.mutate_protected_metadata_with_access(
                &transaction,
                access,
                &note_id,
                |metadata| {
                    let mut changed = false;
                    for tag in &mut metadata.tags {
                        if tag.eq_ignore_ascii_case(&current_name) {
                            *tag = new_name.clone();
                            changed = true;
                        }
                    }
                    if changed {
                        metadata.tags = normalized_tags(&metadata.tags);
                    }
                    Ok(changed)
                },
            )?;
            protected_affected += usize::from(changed);
        }

        if current_name != new_name
            && let Some(source_id) = source_id.as_deref()
        {
            let existing_target_id = transaction
                .query_row(
                    "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                    [&new_name],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            match existing_target_id {
                Some(target_id) if target_id != source_id => {
                    transaction.execute(
                        "INSERT OR IGNORE INTO note_tags(note_id, tag_id)
                             SELECT note_id, ?2 FROM note_tags WHERE tag_id = ?1",
                        params![source_id, target_id],
                    )?;
                    transaction.execute("DELETE FROM tags WHERE id = ?1", [source_id])?;
                }
                _ => {
                    transaction.execute(
                        "UPDATE tags SET name = ?2 WHERE id = ?1",
                        params![source_id, new_name],
                    )?;
                }
            }
        }

        if source_id.is_none() && protected_affected == 0 {
            return Err(StorageError::Validation("tag no longer exists".to_string()));
        }
        self.refresh_notes_after_tag_mutation(&transaction, &ordinary_note_ids)?;
        transaction.commit()?;
        Ok(ordinary_note_ids.len() + protected_affected)
    }

    /// Remove one tag from one note. Protected notes mutate encrypted
    /// MetadataState directly and never recreate a plaintext note_tags row.
    pub fn remove_tag_from_note(&self, note_id: &NoteId, name: &str) -> StorageResult<bool> {
        let name = cleaned_tag_name(name)?;
        if let Some(removed) = self.mutate_protected_metadata_if_unlocked(note_id, |metadata| {
            let before = metadata.tags.len();
            metadata.tags.retain(|tag| !tag.eq_ignore_ascii_case(&name));
            Ok(metadata.tags.len() != before)
        })? {
            return Ok(removed);
        }

        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tag_id = transaction
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                [&name],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(tag_id) = tag_id else {
            transaction.commit()?;
            return Ok(false);
        };
        let removed = transaction.execute(
            "DELETE FROM note_tags WHERE note_id = ?1 AND tag_id = ?2",
            params![note_id, &tag_id],
        )? > 0;
        if removed {
            self.refresh_notes_after_tag_mutation(&transaction, std::slice::from_ref(note_id))?;
        }
        transaction.execute(
            "DELETE FROM tags WHERE id = ?1 AND NOT EXISTS(SELECT 1 FROM note_tags WHERE tag_id = ?1)",
            [&tag_id],
        )?;
        transaction.commit()?;
        Ok(removed)
    }

    /// Delete a tag globally. Because locked protected metadata cannot be
    /// inspected safely, the operation requires all protected notes to be
    /// unlocked or fails before mutating either ordinary or protected state.
    pub fn delete_tag(&self, name: &str) -> StorageResult<usize> {
        let name = cleaned_tag_name(name)?;
        let protected_guard = self.begin_all_protected_notes_tag_mutation()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let tag_id = transaction
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                [&name],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let ordinary_note_ids = match tag_id.as_deref() {
            Some(tag_id) => note_ids_for_tag(&transaction, tag_id)?,
            None => Vec::new(),
        };

        let mut protected_affected = 0_usize;
        for note_id in protected_note_ids(&transaction)? {
            let access = protected_guard.access_for(&note_id).ok_or_else(|| {
                StorageError::Integrity(
                    "protected tag mutation authorization set changed unexpectedly".to_string(),
                )
            })?;
            let changed = self.mutate_protected_metadata_with_access(
                &transaction,
                access,
                &note_id,
                |metadata| {
                    let before = metadata.tags.len();
                    metadata.tags.retain(|tag| !tag.eq_ignore_ascii_case(&name));
                    Ok(metadata.tags.len() != before)
                },
            )?;
            protected_affected += usize::from(changed);
        }

        if let Some(tag_id) = tag_id.as_deref() {
            transaction.execute("DELETE FROM tags WHERE id = ?1", [tag_id])?;
        }
        if tag_id.is_none() && protected_affected == 0 {
            return Err(StorageError::Validation("tag no longer exists".to_string()));
        }
        self.refresh_notes_after_tag_mutation(&transaction, &ordinary_note_ids)?;
        transaction.commit()?;
        Ok(ordinary_note_ids.len() + protected_affected)
    }

    fn begin_all_protected_notes_tag_mutation(&self) -> StorageResult<TagMutationGuard<'_>> {
        for _ in 0..4 {
            let before = {
                let connection = self.connection()?;
                protected_note_ids(&connection)?
            };

            let guard = if before.is_empty() {
                // Ordinary-only libraries still need the same storage serialization,
                // but an empty protected-note set is not a valid multi-note Vault
                // authorization request. Re-check after acquiring storage so a
                // concurrent Protect cannot slip between discovery and mutation.
                TagMutationGuard::Ordinary {
                    _storage: self.lock_operations()?,
                }
            } else {
                TagMutationGuard::Protected(
                    self.begin_protected_notes_mutation(&before)?
                        .ok_or_else(|| {
                            StorageError::Validation(
                                "unlock all encrypted notes before renaming or deleting tags"
                                    .to_string(),
                            )
                        })?,
                )
            };

            let after = {
                let connection = self.connection()?;
                protected_note_ids(&connection)?
            };
            if before == after {
                return Ok(guard);
            }
            drop(guard);
        }
        Err(StorageError::Validation(
            "protected-note state changed repeatedly; retry the tag operation".to_string(),
        ))
    }

    fn refresh_notes_after_tag_mutation(
        &self,
        transaction: &Transaction<'_>,
        note_ids: &[NoteId],
    ) -> StorageResult<()> {
        for note_id in note_ids {
            let note = note_for_tag_refresh(transaction, note_id)?;
            self.refresh_search_index(transaction, &note)?;
            self.enqueue_sync_upsert(transaction, "note", note.id.as_str(), &note)?;
        }
        Ok(())
    }
}

enum TagMutationGuard<'a> {
    Ordinary { _storage: MutexGuard<'a, ()> },
    Protected(ProtectedNotesMutationGuard<'a>),
}

impl TagMutationGuard<'_> {
    fn access_for(&self, note_id: &NoteId) -> Option<&VaultNoteAccess> {
        match self {
            Self::Ordinary { .. } => None,
            Self::Protected(guard) => guard.access_for(note_id),
        }
    }
}

fn cleaned_tag_name(name: &str) -> StorageResult<String> {
    let cleaned = name.trim();
    if cleaned.is_empty() {
        return Err(StorageError::Validation(
            "tag name cannot be empty".to_string(),
        ));
    }
    if cleaned.contains(',') {
        return Err(StorageError::Validation(
            "tag names cannot contain commas".to_string(),
        ));
    }
    Ok(cleaned.to_string())
}

fn protected_note_ids(connection: &Connection) -> StorageResult<Vec<NoteId>> {
    let mut statement =
        connection.prepare("SELECT note_id FROM vault_note_roots ORDER BY note_id")?;
    let rows = statement.query_map([], |row| row.get::<_, NoteId>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn note_ids_for_tag(transaction: &Transaction<'_>, tag_id: &str) -> StorageResult<Vec<NoteId>> {
    let mut statement =
        transaction.prepare("SELECT note_id FROM note_tags WHERE tag_id = ?1 ORDER BY note_id")?;
    let rows = statement.query_map([tag_id], |row| row.get::<_, NoteId>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn note_for_tag_refresh(transaction: &Transaction<'_>, note_id: &NoteId) -> StorageResult<Note> {
    Ok(transaction.query_row(
        "SELECT
            n.id, n.notebook_id, n.title, n.title_style, n.body, n.editor_state, n.plain_text,
            n.pinned, n.archived, n.trashed, n.purge_after, n.created_at, n.updated_at,
            COALESCE((
                SELECT group_concat(t.name, ', ')
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
        [note_id],
        super::notes::note_from_row,
    )?)
}

pub(super) fn split_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn parse_tags(tags: &str) -> Vec<String> {
    normalized_tags(&split_tags(tags))
}

pub(super) fn normalized_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for tag in tags {
        let cleaned = tag.trim();
        if cleaned.is_empty() {
            continue;
        }
        let key = cleaned.to_lowercase();
        if seen.insert(key) {
            result.push(cleaned.to_string());
        }
    }
    result.sort_by_key(|value| value.to_lowercase());
    result
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use crate::domain::{NoteFilters, NoteListQuery, NoteSort, Section};
    use crate::storage::{NoteSaveScope, RevisionMode, new_id};

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

    fn note_with_tags(storage: &Storage, tags: &[&str]) -> Note {
        let mut note = storage.create_note(None).expect("create note");
        note.tags = tags.iter().map(|tag| (*tag).to_string()).collect();
        storage
            .save_note_owned_with_revision_status_scoped(
                note,
                RevisionMode::Checkpoint,
                NoteSaveScope::TAGS,
            )
            .expect("save tags")
            .0
    }

    #[test]
    fn rename_tag_updates_every_note_and_search_index() {
        let (storage, root) = test_storage("rename-tag");
        let first = note_with_tags(&storage, &["alpha", "keep"]);
        let second = note_with_tags(&storage, &["alpha"]);

        assert_eq!(storage.rename_tag("alpha", "renamed").expect("rename"), 2);
        for note_id in [&first.id, &second.id] {
            let note = storage
                .get_note(note_id)
                .expect("lookup")
                .expect("note should remain");
            assert!(note.tags.iter().any(|tag| tag == "renamed"));
            assert!(!note.tags.iter().any(|tag| tag == "alpha"));
        }
        assert!(
            storage
                .list_tags()
                .expect("tags")
                .contains(&"renamed".to_string())
        );

        {
            let search = |term: &str| {
                storage
                    .list_notes_page(
                        &NoteListQuery {
                            section: Section::Notes,
                            notebook_id: None,
                            search: term.to_string(),
                            sort: NoteSort::UpdatedNewest,
                            filters: NoteFilters::default(),
                        },
                        None,
                    )
                    .expect("search notes")
                    .notes
                    .into_iter()
                    .map(|summary| summary.id)
                    .collect::<Vec<_>>()
            };
            let renamed_matches = search("renamed");
            assert!(renamed_matches.contains(&first.id));
            assert!(renamed_matches.contains(&second.id));
            assert!(search("alpha").is_empty());
        }
        cleanup(storage, root);
    }

    #[test]
    fn rename_tag_to_existing_name_merges_without_duplicates() {
        let (storage, root) = test_storage("merge-tag");
        let first = note_with_tags(&storage, &["alpha"]);
        let second = note_with_tags(&storage, &["beta"]);
        let third = note_with_tags(&storage, &["alpha", "beta"]);

        assert_eq!(storage.rename_tag("alpha", "beta").expect("merge"), 2);
        for note_id in [&first.id, &second.id, &third.id] {
            let note = storage
                .get_note(note_id)
                .expect("lookup")
                .expect("note should remain");
            assert_eq!(note.tags.iter().filter(|tag| *tag == "beta").count(), 1);
            assert!(!note.tags.iter().any(|tag| tag == "alpha"));
        }
        cleanup(storage, root);
    }

    #[test]
    fn delete_tag_removes_only_the_tag_relationships() {
        let (storage, root) = test_storage("delete-tag");
        let note = note_with_tags(&storage, &["remove", "keep"]);
        assert_eq!(storage.delete_tag("remove").expect("delete"), 1);
        let note = storage
            .get_note(&note.id)
            .expect("lookup")
            .expect("note should remain");
        assert_eq!(note.tags, vec!["keep".to_string()]);
        cleanup(storage, root);
    }
    #[test]
    fn protected_tag_commands_mutate_encrypted_metadata_without_plaintext_relationships() {
        let (storage, root) = test_storage("protected-tag-commands");
        let protected = note_with_tags(&storage, &["secret", "keep"]);
        storage
            .protect_note_with_password(&protected.id, b"correct horse battery staple")
            .expect("protect note");

        assert!(
            storage
                .remove_tag_from_note(&protected.id, "secret")
                .expect("remove encrypted tag")
        );
        let protected = storage
            .get_note(&protected.id)
            .expect("lookup protected note")
            .expect("protected note exists");
        assert_eq!(protected.tags, vec!["keep".to_string()]);

        let connection = storage.connection().expect("connection");
        let plaintext_relationships: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_tags WHERE note_id = ?1",
                [&protected.id],
                |row| row.get(0),
            )
            .expect("count plaintext tag relationships");
        assert_eq!(plaintext_relationships, 0);
        cleanup(storage, root);
    }

    #[test]
    fn global_tag_rename_and_delete_cover_unlocked_protected_metadata() {
        let (storage, root) = test_storage("protected-global-tag-commands");
        let ordinary = note_with_tags(&storage, &["shared"]);
        let protected = note_with_tags(&storage, &["shared", "protected-only"]);
        storage
            .protect_note_with_password(&protected.id, b"correct horse battery staple")
            .expect("protect note");

        assert_eq!(storage.rename_tag("shared", "renamed").expect("rename"), 2);
        let protected_after_rename = storage
            .get_note(&protected.id)
            .expect("lookup protected")
            .expect("protected exists");
        assert!(
            protected_after_rename
                .tags
                .iter()
                .any(|tag| tag == "renamed")
        );
        assert!(
            !protected_after_rename
                .tags
                .iter()
                .any(|tag| tag == "shared")
        );
        let ordinary_after_rename = storage
            .get_note(&ordinary.id)
            .expect("lookup ordinary")
            .expect("ordinary exists");
        assert_eq!(ordinary_after_rename.tags, vec!["renamed".to_string()]);

        assert_eq!(
            storage
                .delete_tag("protected-only")
                .expect("delete protected-only tag"),
            1
        );
        let protected_after_delete = storage
            .get_note(&protected.id)
            .expect("lookup protected")
            .expect("protected exists");
        assert!(
            !protected_after_delete
                .tags
                .iter()
                .any(|tag| tag == "protected-only")
        );

        let connection = storage.connection().expect("connection");
        let plaintext_relationships: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM note_tags WHERE note_id = ?1",
                [&protected.id],
                |row| row.get(0),
            )
            .expect("count plaintext tag relationships");
        assert_eq!(plaintext_relationships, 0);
        cleanup(storage, root);
    }

    #[test]
    fn global_tag_mutation_fails_closed_when_any_protected_note_is_locked() {
        let (storage, root) = test_storage("protected-global-tag-locked");
        let ordinary = note_with_tags(&storage, &["shared"]);
        let protected = note_with_tags(&storage, &["secret"]);
        storage
            .protect_note_with_password(&protected.id, b"correct horse battery staple")
            .expect("protect note");
        storage
            .lock_protected_note(&protected.id)
            .expect("lock protected note");

        let error = storage
            .rename_tag("shared", "renamed")
            .expect_err("global rename must fail closed while protected metadata is locked");
        assert!(error.to_string().contains("unlock all encrypted notes"));
        let ordinary_after = storage
            .get_note(&ordinary.id)
            .expect("lookup ordinary")
            .expect("ordinary exists");
        assert_eq!(ordinary_after.tags, vec!["shared".to_string()]);
        cleanup(storage, root);
    }
}

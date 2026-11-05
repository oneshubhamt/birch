use std::collections::{HashMap, HashSet};

use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, params, params_from_iter,
    types::Value,
};

use crate::editor_document::analyze_editor_state;
use crate::domain::{
    DEFAULT_NOTEBOOK_ID, NOTE_LIST_PAGE_SIZE, Note, NoteFilters, NoteId, NoteListCursor,
    NoteListPage, NoteListQuery, NoteSort, NoteSummary, NoteTitleStyle, Section,
};
use crate::search::ParsedSearch;

#[cfg(feature = "remote-sync")]
use super::attachments::sha256_bytes;
use super::tags::{normalized_tags, split_tags};
use super::{
    NoteSaveFailure, NoteSaveScope, OwnedNoteSaveResult, RevisionMode, Storage, StorageError,
    StorageResult, new_id, unix_timestamp,
};

const TRASH_PURGE_BATCH_SIZE: i64 = 100;

impl Storage {
    pub fn list_notes_page(
        &self,
        query: &NoteListQuery,
        cursor: Option<&NoteListCursor>,
    ) -> StorageResult<NoteListPage> {
        self.list_notes_page_internal(query, cursor, cursor.is_none())
    }

    pub fn list_notes_page_with_total(
        &self,
        query: &NoteListQuery,
        cursor: Option<&NoteListCursor>,
    ) -> StorageResult<NoteListPage> {
        self.list_notes_page_internal(query, cursor, true)
    }

    /// Refresh a logical page after a mutation without trusting cursors from the
    /// previous list generation. The page boundary and page contents are read
    /// from the same SQLite snapshot; ordinary scrolling continues to use
    /// keyset cursors through `list_notes_page`.
    pub fn reanchor_notes_page_with_total(
        &self,
        query: &NoteListQuery,
        page_index: usize,
    ) -> StorageResult<(Option<NoteListCursor>, NoteListPage)> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let parsed_search = ParsedSearch::parse(query.search.trim());
        let page_start_cursor =
            note_list_page_start_cursor(&transaction, query, &parsed_search, page_index)?;

        let page = if page_index == 0 || page_start_cursor.is_some() {
            note_list_page_in_transaction(
                &transaction,
                query,
                &parsed_search,
                page_start_cursor.as_ref(),
                true,
            )?
        } else {
            NoteListPage {
                notes: Vec::new(),
                next_cursor: None,
                total_count: Some(note_list_total_count(&transaction, query, &parsed_search)?),
            }
        };

        transaction.commit()?;
        Ok((page_start_cursor, self.hydrate_note_list_page(page)?))
    }

    fn list_notes_page_internal(
        &self,
        query: &NoteListQuery,
        cursor: Option<&NoteListCursor>,
        include_total: bool,
    ) -> StorageResult<NoteListPage> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let parsed_search = ParsedSearch::parse(query.search.trim());
        let page = note_list_page_in_transaction(
            &transaction,
            query,
            &parsed_search,
            cursor,
            include_total,
        )?;
        transaction.commit()?;
        self.hydrate_note_list_page(page)
    }

    fn hydrate_note_list_page(&self, mut page: NoteListPage) -> StorageResult<NoteListPage> {
        for note in &mut page.notes {
            if note.protection.is_protected() {
                *note = self.hydrate_protected_summary(note.clone())?;
            }
        }
        Ok(page)
    }

    pub fn note_matches_list_query(
        &self,
        note_id: &NoteId,
        query: &NoteListQuery,
    ) -> StorageResult<bool> {
        let connection = self.connection()?;
        let parsed_search = ParsedSearch::parse(query.search.trim());
        let (where_clause, mut parameters) = note_list_where(query, &parsed_search, 2);
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM notes n WHERE n.id = ?1 AND {where_clause})"
        );
        let mut values = Vec::with_capacity(parameters.len() + 1);
        values.push(Value::from(note_id.as_str().to_string()));
        values.append(&mut parameters);
        Ok(connection.query_row(&sql, params_from_iter(values.iter()), |row| row.get(0))?)
    }
    pub fn get_note(&self, note_id: &NoteId) -> StorageResult<Option<Note>> {
        let connection = self.connection()?;
        let mut note = connection
            .query_row(
                "SELECT
                    n.id, n.notebook_id, n.title, n.title_style, n.body, n.editor_state, n.plain_text, n.pinned, n.archived, n.trashed, n.purge_after,
                    n.created_at, n.updated_at,
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
                note_from_row,
            )
            .optional()?;
        if let Some(note) = note.as_mut()
            && super::protection::protected_note_exists(&connection, &note.id)?
        {
            note.protection = crate::domain::NoteProtectionState::Locked;
        }
        note.map(|note| self.hydrate_protected_note(note)).transpose()
    }
    pub fn create_note(&self, notebook_id: Option<&str>) -> StorageResult<Note> {
        let now = unix_timestamp();
        let notebook_id = notebook_id.unwrap_or(DEFAULT_NOTEBOOK_ID).to_string();
        let note = Note {
            id: super::new_note_id(),
            protection: crate::domain::NoteProtectionState::Unprotected,
            notebook_id,
            title: String::new(),
            title_style: NoteTitleStyle::default(),
            document: std::sync::Arc::new(crate::domain::NoteDocument::default()),
            tags: Vec::new(),
            attachment_names: Vec::new(),
            pinned: false,
            archived: false,
            trashed: false,
            purge_after: None,
            created_at: now,
            updated_at: now,
        };
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (note, _) = self
            .persist_note(&transaction, None, note, None)
            .map_err(|failure| failure.error)?;
        self.enqueue_sync_upsert(&transaction, "note", note.id.as_str(), &note)?;
        transaction.commit()?;
        Ok(note)
    }
    /// Create an independent duplicate of a note and every note-owned resource
    /// referenced by its current canonical document.
    ///
    /// Immutable blob bytes remain content-addressed and are therefore shared by
    /// hash, while attachment/database identities are always remapped. This keeps
    /// later edits/removals independent without copying large files unnecessarily.
    pub fn duplicate_note(&self, note_id: &NoteId) -> StorageResult<Note> {
        let _operation_guard = self.lock_operations()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if super::protection::protected_note_exists(&transaction, note_id)? {
            return Err(StorageError::Validation(
                "remove note encryption before duplicating this note".to_string(),
            ));
        }
        let source = transaction
            .query_row(
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
                 FROM notes n WHERE n.id = ?1",
                [note_id],
                note_from_row,
            )
            .optional()?
            .ok_or_else(|| StorageError::Validation("note no longer exists".to_string()))?;

        let analysis = analyze_editor_state(source.editor_state()).map_err(StorageError::from)?;
        if !analysis.pending_attachment_request_ids.is_empty() {
            return Err(StorageError::Validation(
                "finish pending attachment imports before duplicating this note".to_string(),
            ));
        }

        let all_attachments = self.list_attachments_with_connection(&transaction, note_id)?;
        let attachments_by_id = all_attachments
            .into_iter()
            .map(|attachment| (attachment.id.clone(), attachment))
            .collect::<HashMap<_, _>>();
        let mut required_ids = analysis
            .attachment_ids
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let mut frontier = analysis.attachment_ids.clone();
        while let Some(attachment_id) = frontier.pop() {
            let attachment = attachments_by_id.get(&attachment_id).ok_or_else(|| {
                StorageError::Validation(format!(
                    "document references missing or foreign attachment {attachment_id}"
                ))
            })?;
            if let Some(source_id) = attachment.source_attachment_id.as_ref()
                && required_ids.insert(source_id.clone())
            {
                frontier.push(source_id.clone());
            }
        }

        let duplicate_id = super::new_note_id();
        let attachment_id_map = required_ids
            .iter()
            .map(|attachment_id| (attachment_id.clone(), new_id("att")))
            .collect::<HashMap<_, _>>();
        let editor_state = remap_duplicated_editor_state(
            source.editor_state(),
            &attachment_id_map,
            &source.id,
            &duplicate_id,
        )?;
        let projections = crate::editor_projection::derive_editor_projections(&editor_state)
            .map_err(StorageError::from)?;
        let now = unix_timestamp();
        let title = duplicated_note_title(source.display_title());

        transaction.execute(
            "INSERT INTO notes(
                id, fts_rowid, notebook_id, title, title_style, body, editor_state, plain_text, pinned,
                archived, trashed, purge_after, created_at, updated_at
             ) VALUES (
                ?1, (SELECT COALESCE(MAX(fts_rowid), 0) + 1 FROM notes),
                ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, 0, NULL, ?8, ?8
             )",
            params![
                &duplicate_id,
                &source.notebook_id,
                &title,
                serde_json::to_string(&source.title_style)?,
                &projections.markdown,
                &editor_state,
                &projections.plain_text,
                now,
            ],
        )?;
        self.replace_note_tags(&transaction, &duplicate_id, &source.tags)?;

        let mut remaining = required_ids.into_iter().collect::<Vec<_>>();
        let mut inserted = HashSet::new();
        while !remaining.is_empty() {
            let Some(index) = remaining.iter().position(|attachment_id| {
                attachments_by_id
                    .get(attachment_id)
                    .and_then(|attachment| attachment.source_attachment_id.as_ref())
                    .is_none_or(|source_id| inserted.contains(source_id))
            }) else {
                return Err(StorageError::Integrity(
                    "attachment edit ancestry contains a cycle".to_string(),
                ));
            };
            let old_id = remaining.swap_remove(index);
            let source_attachment = attachments_by_id.get(&old_id).ok_or_else(|| {
                StorageError::Integrity(format!("attachment {old_id} disappeared during duplication"))
            })?;
            let mut duplicate_attachment = source_attachment.clone();
            duplicate_attachment.id = attachment_id_map
                .get(&old_id)
                .cloned()
                .ok_or_else(|| StorageError::Integrity("attachment remap is incomplete".to_string()))?;
            duplicate_attachment.note_id = duplicate_id.clone();
            duplicate_attachment.source_attachment_id = source_attachment
                .source_attachment_id
                .as_ref()
                .map(|source_id| {
                    attachment_id_map.get(source_id).cloned().ok_or_else(|| {
                        StorageError::Integrity("image source remap is incomplete".to_string())
                    })
                })
                .transpose()?;
            duplicate_attachment.created_at = now;
            duplicate_attachment.updated_at = now;
            self.insert_attachment(&transaction, &duplicate_attachment)?;
            self.enqueue_sync_upsert(
                &transaction,
                "attachment",
                &duplicate_attachment.id,
                &duplicate_attachment,
            )?;
            inserted.insert(old_id);
        }

        let remapped_document = analyze_editor_state(&editor_state).map_err(StorageError::from)?;
        let (attachment_names, _) = self.reconcile_document_attachments(
            &transaction,
            &duplicate_id,
            &remapped_document.attachment_ids,
            &[],
        )?;
        self.reconcile_document_note_links(
            &transaction,
            &duplicate_id,
            &remapped_document.note_links,
        )?;

        transaction.execute(
            "INSERT INTO note_parent(child_note_id, parent_note_id)
             SELECT ?1, parent_note_id FROM note_parent WHERE child_note_id = ?2",
            params![&duplicate_id, note_id],
        )?;
        transaction.execute(
            "INSERT INTO note_sources(note_id, source_id, created_at)
             SELECT ?1, source_id, ?3 FROM note_sources WHERE note_id = ?2",
            params![&duplicate_id, note_id, now],
        )?;
        {
            let mut statement = transaction.prepare(
                "SELECT source_id, locator FROM citations WHERE note_id = ?1 ORDER BY created_at, id",
            )?;
            let rows = statement.query_map([note_id], |row| {
                Ok((row.get::<_, crate::domain::SourceId>(0)?, row.get::<_, String>(1)?))
            })?;
            let citations = rows.collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            for (source_id, locator) in citations {
                transaction.execute(
                    "INSERT INTO citations(id, note_id, source_id, locator, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![new_id("cite"), &duplicate_id, source_id, locator, now],
                )?;
            }
        }
        for (old_id, new_id) in &attachment_id_map {
            transaction.execute(
                "INSERT INTO pdf_annotation_pages(attachment_id, page_index, annotations_json, updated_at)
                 SELECT ?1, page_index, annotations_json, ?3
                 FROM pdf_annotation_pages WHERE attachment_id = ?2",
                params![new_id, old_id, now],
            )?;
        }

        let mut duplicate = Note {
            id: duplicate_id,
            protection: crate::domain::NoteProtectionState::Unprotected,
            notebook_id: source.notebook_id,
            title,
            title_style: source.title_style,
            document: std::sync::Arc::new(crate::domain::NoteDocument::new(
                projections.markdown,
                std::sync::Arc::new(editor_state),
                projections.plain_text,
            )),
            tags: source.tags,
            attachment_names,
            pinned: false,
            archived: false,
            trashed: false,
            purge_after: None,
            created_at: now,
            updated_at: now,
        };
        self.refresh_search_index(&transaction, &duplicate)?;
        self.enqueue_sync_upsert(&transaction, "note", duplicate.id.as_str(), &duplicate)?;
        transaction.commit()?;
        duplicate.attachment_names.sort_by_key(|name| name.to_lowercase());
        Ok(duplicate)
    }

    pub fn save_note_owned(&self, note: Note) -> StorageResult<Note> {
        self.save_note_owned_with_revision_status(note, RevisionMode::Autosave)
            .map(|(note, _)| note)
            .map_err(|failure| failure.error)
    }

    pub fn save_note_checkpoint_owned(&self, note: Note) -> StorageResult<Note> {
        self.save_note_owned_with_revision_status(note, RevisionMode::Checkpoint)
            .map(|(note, _)| note)
            .map_err(|failure| failure.error)
    }

    #[cfg(test)]
    pub(super) fn save_note_owned_with_revision_status_for_test(
        &self,
        note: Note,
        revision_mode: RevisionMode,
    ) -> OwnedNoteSaveResult {
        self.save_note_owned_with_revision_status(note, revision_mode)
    }

    fn save_note_owned_with_revision_status(
        &self,
        note: Note,
        revision_mode: RevisionMode,
    ) -> OwnedNoteSaveResult {
        let note_id = note.id.clone();
        match self.protected_note_mutation_if_unlocked(&note_id) {
            Ok(Some(mutation)) => self.save_note_owned_with_revision_status_locked(
                note,
                revision_mode,
                Some(mutation.access()),
            ),
            Ok(None) => {
                let _operation_guard = match self.lock_operations() {
                    Ok(guard) => guard,
                    Err(error) => return Err(NoteSaveFailure::new(note, error)),
                };
                self.save_note_owned_with_revision_status_locked(note, revision_mode, None)
            }
            Err(error) => Err(NoteSaveFailure::new(note, error)),
        }
    }

    fn save_note_owned_with_revision_status_locked(
        &self,
        note: Note,
        revision_mode: RevisionMode,
        protected_access: Option<&crate::security::VaultNoteAccess>,
    ) -> OwnedNoteSaveResult {
        let mut connection = match self.connection() {
            Ok(connection) => connection,
            Err(error) => return Err(NoteSaveFailure::new(note, error)),
        };
        let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate)
        {
            Ok(transaction) => transaction,
            Err(error) => return Err(NoteSaveFailure::new(note, error.into())),
        };
        let (note, revision_recorded) = self.persist_note(
            &transaction,
            protected_access,
            note,
            Some(revision_mode),
        )?;
        if let Err(error) = transaction.commit() {
            return Err(NoteSaveFailure::new(note, error.into()));
        }
        Ok((note, revision_recorded))
    }

    pub(crate) fn save_note_owned_with_revision_status_scoped(
        &self,
        note: Note,
        revision_mode: RevisionMode,
        scope: NoteSaveScope,
    ) -> OwnedNoteSaveResult {
        if scope == NoteSaveScope::ALL {
            return self.save_note_owned_with_revision_status(note, revision_mode);
        }
        let sensitive_scope =
            scope.touches_document() || scope.touches_title() || scope.touches_tags();
        if sensitive_scope {
            let note_id = note.id.clone();
            match self.protected_note_mutation_if_unlocked(&note_id) {
                Ok(Some(mutation)) => {
                    return self.save_note_owned_with_revision_status_scoped_locked(
                        note,
                        revision_mode,
                        scope,
                        Some(mutation.access()),
                    );
                }
                Ok(None) => {}
                Err(error) => return Err(NoteSaveFailure::new(note, error)),
            }
        }
        let _operation_guard = match self.lock_operations() {
            Ok(guard) => guard,
            Err(error) => return Err(NoteSaveFailure::new(note, error)),
        };
        self.save_note_owned_with_revision_status_scoped_locked(
            note,
            revision_mode,
            scope,
            None,
        )
    }

    fn save_note_owned_with_revision_status_scoped_locked(
        &self,
        note: Note,
        revision_mode: RevisionMode,
        scope: NoteSaveScope,
        protected_access: Option<&crate::security::VaultNoteAccess>,
    ) -> OwnedNoteSaveResult {
        let mut connection = match self.connection() {
            Ok(connection) => connection,
            Err(error) => return Err(NoteSaveFailure::new(note, error)),
        };
        let transaction = match connection.transaction_with_behavior(TransactionBehavior::Immediate)
        {
            Ok(transaction) => transaction,
            Err(error) => return Err(NoteSaveFailure::new(note, error.into())),
        };
        let (note, revision_recorded) = self.persist_existing_note_scoped(
            &transaction,
            protected_access,
            note,
            Some(revision_mode),
            scope,
        )?;
        if let Err(error) = transaction.commit() {
            return Err(NoteSaveFailure::new(note, error.into()));
        }
        Ok((note, revision_recorded))
    }

    fn persist_existing_note_scoped(
        &self,
        transaction: &Transaction<'_>,
        protected_access: Option<&crate::security::VaultNoteAccess>,
        mut note: Note,
        revision_mode: Option<RevisionMode>,
        scope: NoteSaveScope,
    ) -> OwnedNoteSaveResult {
        let result = (|| -> StorageResult<bool> {
            if scope.touches_title() {
                note.title_style = std::mem::take(&mut note.title_style).normalized();
            }
            let existing_state = transaction
                .query_row(
                    "SELECT notebook_id, pinned, archived, trashed, purge_after
                     FROM notes WHERE id = ?1",
                    [&note.id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)? != 0,
                            row.get::<_, i64>(2)? != 0,
                            row.get::<_, i64>(3)? != 0,
                            row.get::<_, Option<i64>>(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                existing_notebook_id,
                existing_pinned,
                existing_archived,
                existing_trashed,
                existing_purge_after,
            )) = existing_state
            else {
                return Err(StorageError::Validation("note no longer exists".to_string()));
            };
            let protected = super::protection::protected_note_exists(transaction, &note.id)?;
            if existing_trashed
                && (scope.touches_document() || scope.touches_title() || scope.touches_tags())
            {
                return Err(StorageError::Validation(
                    "notes in Trash are read-only; recover the note before editing it".to_string(),
                ));
            }
            if existing_trashed && scope.touches_state() {
                let unchanged_while_trashed = note.trashed
                    && note.notebook_id == existing_notebook_id
                    && note.pinned == existing_pinned
                    && note.archived == existing_archived
                    && note.purge_after == existing_purge_after;
                let clean_recovery = !note.trashed
                    && note.notebook_id == existing_notebook_id
                    && note.pinned == existing_pinned
                    && note.archived == existing_archived
                    && note.purge_after.is_none();
                if !unchanged_while_trashed && !clean_recovery {
                    return Err(StorageError::Validation(
                        "notes in Trash are read-only; recover the note before changing its state"
                            .to_string(),
                    ));
                }
            }

            if protected {
                let sensitive_scope =
                    scope.touches_document() || scope.touches_title() || scope.touches_tags();
                let revision_recorded = if sensitive_scope {
                    let access = protected_access.ok_or_else(|| StorageError::Validation(
                        "note encryption changed while saving; retry after unlocking".to_string(),
                    ))?;
                    self.persist_protected_note_content(
                        transaction,
                        access,
                        &mut note,
                        revision_mode,
                        scope,
                    )?
                } else {
                    false
                };
                if scope.touches_state() {
                    if note.notebook_id.trim().is_empty() {
                        note.notebook_id = DEFAULT_NOTEBOOK_ID.to_string();
                    }
                    let notebook_exists: bool = transaction.query_row(
                        "SELECT EXISTS(SELECT 1 FROM notebooks WHERE id = ?1)",
                        [&note.notebook_id],
                        |row| row.get(0),
                    )?;
                    if !notebook_exists {
                        return Err(StorageError::Validation(format!(
                            "note references missing notebook {}",
                            note.notebook_id
                        )));
                    }
                    transaction.execute(
                        "UPDATE notes
                         SET notebook_id = ?1, pinned = ?2, archived = ?3, trashed = ?4,
                             purge_after = ?5, updated_at = ?6
                         WHERE id = ?7",
                        params![
                            &note.notebook_id,
                            note.pinned as i64,
                            note.archived as i64,
                            note.trashed as i64,
                            note.purge_after,
                            note.updated_at,
                            &note.id
                        ],
                    )?;
                }
                return Ok(revision_recorded);
            }

            if scope.touches_document() {
                let analysis =
                    analyze_editor_state(note.editor_state()).map_err(StorageError::from)?;
                let (attachment_names, attachment_membership_changed) =
                    self.reconcile_document_attachments(
                        transaction,
                        &note.id,
                        &analysis.attachment_ids,
                        &analysis.pending_attachment_request_ids,
                    )?;
                note.attachment_names = attachment_names;
                self.reconcile_document_note_links(
                    transaction,
                    &note.id,
                    &analysis.note_links,
                )?;
                transaction.execute(
                    "UPDATE notes
                     SET body = ?1, editor_state = ?2, plain_text = ?3, updated_at = ?4
                     WHERE id = ?5",
                    params![
                        note.body(),
                        note.editor_state(),
                        note.plain_text(),
                        note.updated_at,
                        &note.id
                    ],
                )?;
                if attachment_membership_changed {
                    transaction.execute(
                        "UPDATE notes_fts
                         SET body = ?1, attachment_names = ?2
                         WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = ?3)",
                        params![
                            note.plain_text(),
                            note.attachment_names.join("\u{1f}"),
                            &note.id
                        ],
                    )?;
                } else {
                    transaction.execute(
                        "UPDATE notes_fts
                         SET body = ?1
                         WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = ?2)",
                        params![note.plain_text(), &note.id],
                    )?;
                }
            }

            if scope.touches_title() {
                transaction.execute(
                    "UPDATE notes SET title = ?1, title_style = ?2, updated_at = ?3 WHERE id = ?4",
                    params![
                        &note.title,
                        serde_json::to_string(&note.title_style)?,
                        note.updated_at,
                        &note.id
                    ],
                )?;
                transaction.execute(
                    "UPDATE notes_fts
                     SET title = ?1
                     WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = ?2)",
                    params![&note.title, &note.id],
                )?;
            }

            if scope.touches_state() {
                if note.notebook_id.trim().is_empty() {
                    note.notebook_id = DEFAULT_NOTEBOOK_ID.to_string();
                }
                let notebook_exists: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM notebooks WHERE id = ?1)",
                    [&note.notebook_id],
                    |row| row.get(0),
                )?;
                if !notebook_exists {
                    return Err(StorageError::Validation(format!(
                        "note references missing notebook {}",
                        note.notebook_id
                    )));
                }
                transaction.execute(
                    "UPDATE notes
                     SET notebook_id = ?1, pinned = ?2, archived = ?3, trashed = ?4,
                         purge_after = ?5, updated_at = ?6
                     WHERE id = ?7",
                    params![
                        &note.notebook_id,
                        note.pinned as i64,
                        note.archived as i64,
                        note.trashed as i64,
                        note.purge_after,
                        note.updated_at,
                        &note.id
                    ],
                )?;
            }

            if scope.touches_tags() {
                note.tags = normalized_tags(&note.tags);
                self.replace_note_tags(transaction, &note.id, &note.tags)?;
                transaction.execute(
                    "UPDATE notes SET updated_at = ?1 WHERE id = ?2",
                    params![note.updated_at, &note.id],
                )?;
                transaction.execute(
                    "UPDATE notes_fts
                     SET tags = ?1
                     WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = ?2)",
                    params![note.tags_text(), &note.id],
                )?;
            }

            let revision_recorded = if let Some(revision_mode) = revision_mode {
                self.record_revision(transaction, &note, revision_mode)?
            } else {
                false
            };
            self.enqueue_sync_upsert(transaction, "note", note.id.as_str(), &note)?;
            Ok(revision_recorded)
        })();

        match result {
            Ok(revision_recorded) => Ok((note, revision_recorded)),
            Err(error) => Err(NoteSaveFailure::new(note, error)),
        }
    }

    pub(super) fn persist_note(
        &self,
        transaction: &Transaction<'_>,
        protected_access: Option<&crate::security::VaultNoteAccess>,
        mut note: Note,
        revision_mode: Option<RevisionMode>,
    ) -> OwnedNoteSaveResult {
        match super::protection::protected_note_exists(transaction, &note.id) {
            Ok(true) => {
                return self.persist_existing_note_scoped(
                    transaction,
                    protected_access,
                    note,
                    revision_mode,
                    NoteSaveScope::ALL,
                );
            }
            Ok(false) => {}
            Err(error) => return Err(NoteSaveFailure::new(note, error)),
        }
        let result = (|| -> StorageResult<bool> {
            note.tags = normalized_tags(&note.tags);
            note.title_style = std::mem::take(&mut note.title_style).normalized();
            let analysis =
                analyze_editor_state(note.editor_state()).map_err(StorageError::from)?;
            if note.notebook_id.trim().is_empty() {
                note.notebook_id = DEFAULT_NOTEBOOK_ID.to_string();
            }
            let notebook_exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM notebooks WHERE id = ?1)",
                [&note.notebook_id],
                |row| row.get(0),
            )?;
            if !notebook_exists {
                return Err(StorageError::Validation(format!(
                    "note references missing notebook {}",
                    note.notebook_id
                )));
            }

            transaction.execute(
                "INSERT INTO notes(
                    id, fts_rowid, notebook_id, title, title_style, body, editor_state, plain_text, pinned,
                    archived, trashed, purge_after, created_at, updated_at
                 ) VALUES (
                    ?1, COALESCE(
                        (SELECT fts_rowid FROM notes WHERE id = ?1),
                        (SELECT COALESCE(MAX(fts_rowid), 0) + 1 FROM notes)
                    ),
                    ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
                 )
                 ON CONFLICT(id) DO UPDATE SET
                    notebook_id = excluded.notebook_id,
                    title = excluded.title,
                    title_style = excluded.title_style,
                    body = excluded.body,
                    editor_state = excluded.editor_state,
                    plain_text = excluded.plain_text,
                    pinned = excluded.pinned,
                    archived = excluded.archived,
                    trashed = excluded.trashed,
                    purge_after = excluded.purge_after,
                    updated_at = excluded.updated_at",
                params![
                    &note.id,
                    &note.notebook_id,
                    &note.title,
                    serde_json::to_string(&note.title_style)?,
                    note.body(),
                    note.editor_state(),
                    note.plain_text(),
                    note.pinned as i64,
                    note.archived as i64,
                    note.trashed as i64,
                    note.purge_after,
                    note.created_at,
                    note.updated_at
                ],
            )?;
            self.replace_note_tags(transaction, &note.id, &note.tags)?;
            let (attachment_names, _) = self.reconcile_document_attachments(
                transaction,
                &note.id,
                &analysis.attachment_ids,
                &analysis.pending_attachment_request_ids,
            )?;
            note.attachment_names = attachment_names;
            self.reconcile_document_note_links(
                transaction,
                &note.id,
                &analysis.note_links,
            )?;
            self.refresh_search_index(transaction, &note)?;

            let revision_recorded = if let Some(revision_mode) = revision_mode {
                let revision_recorded = self.record_revision(transaction, &note, revision_mode)?;
                self.enqueue_sync_upsert(transaction, "note", note.id.as_str(), &note)?;
                revision_recorded
            } else {
                false
            };
            Ok(revision_recorded)
        })();

        match result {
            Ok(revision_recorded) => Ok((note, revision_recorded)),
            Err(error) => Err(NoteSaveFailure::new(note, error)),
        }
    }

    pub fn purge_expired_notes(&self) -> StorageResult<usize> {
        let now = unix_timestamp();
        let mut purged = 0_usize;

        loop {
            // Candidate discovery deliberately happens before coordinator
            // admission. The coordinator gates/drains plaintext producers before
            // taking storage mutation authority, so an existing producer can
            // finish without deadlocking. Eligibility is revalidated after
            // all selected note transitions have drained.
            let note_ids = {
                let connection = self.connection()?;
                let mut statement = connection.prepare(
                    "SELECT id FROM notes
                     WHERE trashed = 1 AND purge_after IS NOT NULL AND purge_after <= ?1
                     ORDER BY purge_after, id
                     LIMIT ?2",
                )?;
                let rows = statement.query_map(params![now, TRASH_PURGE_BATCH_SIZE], |row| {
                    row.get::<_, NoteId>(0)
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            if note_ids.is_empty() {
                break;
            }

            // Acquire multi-note transitions in canonical note-id order. Other
            // multi-note workflows use the same ordering, preventing a cyclic
            // wait when maintenance and rollback touch overlapping note sets.
            let mut transition_note_ids = note_ids.clone();
            transition_note_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            transition_note_ids.dedup_by(|left, right| left.as_str() == right.as_str());
            let _privacy_guard = self.begin_multi_note_privacy_transition(
                &transition_note_ids,
                crate::security::NotePrivacyTransition::Delete,
            )?;
            let mut connection = self.connection()?;
            let eligible_note_ids = {
                let mut eligible = Vec::with_capacity(note_ids.len());
                for note_id in &note_ids {
                    let still_expired = connection
                        .query_row(
                            "SELECT 1 FROM notes
                             WHERE id = ?1 AND trashed = 1
                               AND purge_after IS NOT NULL AND purge_after <= ?2",
                            params![note_id, now],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some();
                    if still_expired {
                        eligible.push(note_id.clone());
                    }
                }
                eligible
            };

            if eligible_note_ids.is_empty() {
                drop(connection);
                drop(_privacy_guard);
                continue;
            }

            let placeholders = vec!["?"; eligible_note_ids.len()].join(", ");
            let protected_revision_residue_sql = format!(
                "SELECT EXISTS(
                    SELECT 1
                    FROM revision_attachment_refs refs
                    JOIN note_revisions revision ON revision.id = refs.revision_id
                    JOIN vault_note_roots root ON root.note_id = revision.note_id
                    WHERE revision.note_id IN ({placeholders})
                )"
            );
            let protected_revision_residue: i64 = connection.query_row(
                &protected_revision_residue_sql,
                params_from_iter(eligible_note_ids.iter()),
                |row| row.get(0),
            )?;
            if protected_revision_residue != 0 {
                return Err(StorageError::Integrity(
                    "protected Trash note retained ordinary revision attachment references"
                        .to_string(),
                ));
            }

            // Filesystem plaintext is retired while all durable owner records
            // still exist and while the note-scoped transition blocks any new
            // producer. Only after cleanup succeeds may cascades remove owners.
            for note_id in &eligible_note_ids {
                self.retire_note_plaintext_before_delete_unlocked(note_id)?;
            }

            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Capture ordinary canonical blob identities while their attachment
            // rows still exist. Protected-note blobs have no ordinary plaintext
            // lifetime to transfer here. If the cascade removes the final
            // persistent reference, the same transaction establishes generic
            // durable ownership before it can commit.
            let ordinary_blob_hashes = {
                let mut hashes = HashSet::new();
                let attachment_sql = format!(
                    "SELECT DISTINCT a.sha256
                     FROM attachments a
                     WHERE a.note_id IN ({placeholders})
                       AND NOT EXISTS(
                           SELECT 1 FROM vault_note_roots root WHERE root.note_id = a.note_id
                       )"
                );
                let mut statement = transaction.prepare(&attachment_sql)?;
                let rows = statement.query_map(params_from_iter(eligible_note_ids.iter()), |row| {
                    row.get::<_, String>(0)
                })?;
                hashes.extend(rows.collect::<Result<HashSet<_>, _>>()?);
                drop(statement);

                let revision_sql = format!(
                    "SELECT DISTINCT refs.sha256
                     FROM revision_attachment_refs refs
                     JOIN note_revisions revision ON revision.id = refs.revision_id
                     WHERE revision.note_id IN ({placeholders})
                       AND NOT EXISTS(
                           SELECT 1 FROM vault_note_roots root
                           WHERE root.note_id = revision.note_id
                       )"
                );
                let mut statement = transaction.prepare(&revision_sql)?;
                let rows = statement.query_map(params_from_iter(eligible_note_ids.iter()), |row| {
                    row.get::<_, String>(0)
                })?;
                hashes.extend(rows.collect::<Result<HashSet<_>, _>>()?);
                hashes
            };
            let protected_ciphertext_hashes = {
                let sql = format!(
                    "SELECT DISTINCT refs.sha256
                     FROM vault_blob_refs refs
                     WHERE refs.note_id IN ({placeholders})"
                );
                let mut statement = transaction.prepare(&sql)?;
                let rows = statement.query_map(
                    params_from_iter(eligible_note_ids.iter()),
                    |row| row.get::<_, String>(0),
                )?;
                rows.collect::<Result<HashSet<_>, _>>()?
            };
            #[cfg(feature = "remote-sync")]
            {
                let attachment_ids = {
                    let sql = format!(
                        "SELECT id FROM attachments WHERE note_id IN ({placeholders}) ORDER BY id"
                    );
                    let mut statement = transaction.prepare(&sql)?;
                    let rows = statement.query_map(params_from_iter(eligible_note_ids.iter()), |row| {
                        row.get::<_, String>(0)
                    })?;
                    rows.collect::<Result<Vec<_>, _>>()?
                };
                for attachment_id in &attachment_ids {
                    self.enqueue_sync_delete(&transaction, "attachment", attachment_id)?;
                }
                for note_id in &eligible_note_ids {
                    self.enqueue_sync_delete(&transaction, "note", note_id.as_str())?;
                }
            }

            let fts_sql = format!(
                "DELETE FROM notes_fts
                 WHERE rowid IN (
                     SELECT fts_rowid FROM notes WHERE id IN ({placeholders})
                 )"
            );
            transaction.execute(&fts_sql, params_from_iter(eligible_note_ids.iter()))?;

            let operations_sql =
                format!("DELETE FROM attachment_operations WHERE note_id IN ({placeholders})");
            transaction.execute(
                &operations_sql,
                params_from_iter(eligible_note_ids.iter()),
            )?;

            let delete_sql = format!("DELETE FROM notes WHERE id IN ({placeholders})");
            let removed = transaction.execute(
                &delete_sql,
                params_from_iter(eligible_note_ids.iter()),
            )?;
            if removed != eligible_note_ids.len() {
                return Err(StorageError::Integrity(format!(
                    "Trash maintenance revalidated {} expired note(s) but deleted {removed}",
                    eligible_note_ids.len()
                )));
            }
            for sha256 in ordinary_blob_hashes
                .iter()
                .chain(protected_ciphertext_hashes.iter())
            {
                super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
                    &transaction,
                    sha256,
                )?;
            }
            transaction.commit()?;
            for sha256 in &protected_ciphertext_hashes {
                self.discard_canonical_blob_if_unreferenced_strict(sha256)?;
            }

            purged = purged.checked_add(removed).ok_or_else(|| {
                StorageError::Integrity("expired Trash purge count overflow".to_string())
            })?;

            // Explicit drops bound the lifetime of the canonical coordinator
            // privacy guard before discovering the next batch. This keeps both
            // admission and storage-mutation hold times batch-sized.
            drop(connection);
            drop(_privacy_guard);

            if note_ids.len() < TRASH_PURGE_BATCH_SIZE as usize {
                break;
            }
        }

        if purged > 0 {
            let _operation_guard = self.lock_operations()?;
            self.quarantine_orphan_blobs_unlocked()?;
        }
        Ok(purged)
    }

    pub fn permanently_delete_trashed_note(&self, note_id: &NoteId) -> StorageResult<()> {
        // PrivacyCoordinator admits Delete, blocks/drains producers and readers,
        // then takes storage mutation authority. No new protected/plaintext work
        // can appear between cleanup and cascading durable-owner removal.
        let _privacy_guard = self.begin_note_privacy_transition(
            note_id,
            crate::security::NotePrivacyTransition::Delete,
        )?;
        let connection = self.connection()?;
        let trashed = connection
            .query_row(
                "SELECT trashed FROM notes WHERE id = ?1",
                [note_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        match trashed {
            None => {
                return Err(StorageError::Validation(
                    "note no longer exists".to_string(),
                ));
            }
            Some(0) => {
                return Err(StorageError::Validation(
                    "only notes in Trash can be permanently deleted".to_string(),
                ));
            }
            Some(_) => {}
        }
        drop(connection);
        self.hard_delete_note_unlocked(note_id)
    }

    /// Complete plaintext retirement boundary for note deletion/rollback.
    /// Caller must already hold the appropriate `PrivacyCoordinator` mutation
    /// guard, which keeps the attachment set, protected readers, plaintext
    /// producers, and storage mutation scope stable until cleanup finishes.
    pub(super) fn retire_note_plaintext_before_delete_unlocked(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<()> {
        // Caller already owns Delete admission. Public attachment readers are
        // intentionally admission-aware and would therefore wait for this same
        // transition on protected notes. Cleanup only needs stable attachment
        // identities, which the protected public stubs retain in SQLite.
        let connection = self.connection()?;
        let attachment_ids = {
            let mut statement =
                connection.prepare("SELECT id FROM attachments WHERE note_id = ?1 ORDER BY id")?;
            let rows = statement.query_map([note_id], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        drop(connection);

        self.discard_managed_plaintext_staging_for_note(note_id)?;
        self.discard_attachment_ingests_for_note(note_id)?;
        self.discard_attachment_operation_staging_for_note(note_id)?;
        crate::native::staging::discard_staged_derivatives_for_note(note_id.as_str())
            .map_err(|error| StorageError::Integrity(error.to_string()))?;
        crate::native::staging::discard_staged_derivatives_for_attachments(&attachment_ids)
            .map_err(|error| StorageError::Integrity(error.to_string()))?;
        Ok(())
    }

    pub(super) fn hard_delete_note_unlocked(&self, note_id: &NoteId) -> StorageResult<()> {
        // Caller holds the canonical coordinator mutation guard. Filesystem
        // plaintext is retired while durable owner records still exist; only
        // successful cleanup permits note deletion to cascade those owners away.
        self.retire_note_plaintext_before_delete_unlocked(note_id)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let attachments = self.list_attachments_with_connection(&transaction, note_id)?;
        let was_protected = super::protection::protected_note_exists(&transaction, note_id)?;
        let historical_blob_hashes = {
            let mut statement = transaction.prepare(
                "SELECT DISTINCT refs.sha256
                 FROM revision_attachment_refs refs
                 JOIN note_revisions revision ON revision.id = refs.revision_id
                 WHERE revision.note_id = ?1
                 ORDER BY refs.sha256",
            )?;
            let rows = statement.query_map([note_id], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<HashSet<_>, _>>()?
        };
        if was_protected && !historical_blob_hashes.is_empty() {
            return Err(StorageError::Integrity(
                "protected note retained ordinary revision attachment references".to_string(),
            ));
        }
        let protected_blob_hashes = {
            let mut statement = transaction.prepare(
                "SELECT sha256 FROM vault_blob_refs WHERE note_id = ?1 ORDER BY sha256",
            )?;
            let rows = statement.query_map([note_id], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for attachment in &attachments {
            self.enqueue_sync_delete(&transaction, "attachment", &attachment.id)?;
        }
        self.enqueue_sync_delete(&transaction, "note", note_id.as_str())?;
        transaction.execute(
            "DELETE FROM notes_fts WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = ?1)",
            [note_id],
        )?;
        transaction.execute(
            "DELETE FROM attachment_operations WHERE note_id = ?1",
            [note_id],
        )?;
        transaction.execute("DELETE FROM notes WHERE id = ?1", [note_id])?;
        let ordinary_plaintext_hashes = if was_protected {
            HashSet::new()
        } else {
            let mut hashes = attachments
                .iter()
                .map(|attachment| attachment.sha256.clone())
                .collect::<HashSet<_>>();
            hashes.extend(historical_blob_hashes);
            hashes
        };
        for sha256 in ordinary_plaintext_hashes
            .iter()
            .chain(protected_blob_hashes.iter())
        {
            super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
                &transaction,
                sha256,
            )?;
        }
        transaction.commit()?;

        for sha256 in ordinary_plaintext_hashes {
            self.quarantine_blob_if_unreferenced(&sha256)?;
        }
        for sha256 in protected_blob_hashes {
            self.discard_canonical_blob_if_unreferenced_strict(&sha256)?;
        }
        Ok(())
    }
    #[cfg(feature = "remote-sync")]
    pub fn pending_sync_count(&self) -> StorageResult<i64> {
        let connection = self.connection()?;
        Ok(connection.query_row(
            "SELECT COUNT(*) FROM sync_queue WHERE state = 'pending'",
            [],
            |row| row.get(0),
        )?)
    }
    #[cfg(not(feature = "remote-sync"))]
    pub fn pending_sync_count(&self) -> StorageResult<i64> {
        Ok(0)
    }
    #[cfg(feature = "remote-sync")]
    pub(super) fn enqueue_sync_upsert<T: serde::Serialize>(
        &self,
        transaction: &Transaction<'_>,
        entity_type: &str,
        entity_id: &str,
        payload: &T,
    ) -> StorageResult<()> {
        if !self.sync_payload_allowed(transaction, entity_type, entity_id)? {
            self.clear_sync_payloads_for_entity(transaction, entity_type, entity_id)?;
            return Ok(());
        }
        let payload_json = serde_json::to_string(payload)?;
        self.enqueue_sync_payload(
            transaction,
            entity_type,
            entity_id,
            "upsert",
            &payload_json,
        )
    }
    #[cfg(not(feature = "remote-sync"))]
    pub(super) fn enqueue_sync_upsert<T>(
        &self,
        _transaction: &Transaction<'_>,
        _entity_type: &str,
        _entity_id: &str,
        _payload: &T,
    ) -> StorageResult<()> {
        Ok(())
    }
    #[cfg(feature = "remote-sync")]
    pub(super) fn enqueue_sync_delete(
        &self,
        transaction: &Transaction<'_>,
        entity_type: &str,
        entity_id: &str,
    ) -> StorageResult<()> {
        if !self.sync_payload_allowed(transaction, entity_type, entity_id)? {
            self.clear_sync_payloads_for_entity(transaction, entity_type, entity_id)?;
            return Ok(());
        }
        let payload_json = serde_json::to_string(&serde_json::json!({ "id": entity_id }))?;
        self.enqueue_sync_payload(
            transaction,
            entity_type,
            entity_id,
            "delete",
            &payload_json,
        )
    }
    #[cfg(not(feature = "remote-sync"))]
    pub(super) fn enqueue_sync_delete(
        &self,
        _transaction: &Transaction<'_>,
        _entity_type: &str,
        _entity_id: &str,
    ) -> StorageResult<()> {
        Ok(())
    }
    #[cfg(feature = "remote-sync")]
    fn sync_payload_allowed(
        &self,
        transaction: &Transaction<'_>,
        entity_type: &str,
        entity_id: &str,
    ) -> StorageResult<bool> {
        match entity_type {
            "note" => {
                let (exists, protected): (i64, i64) = transaction.query_row(
                    "SELECT
                        EXISTS(SELECT 1 FROM notes WHERE id = ?1),
                        EXISTS(SELECT 1 FROM vault_note_roots WHERE note_id = ?1)",
                    [entity_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                // Missing entities are also suppressed: a caller that has
                // already discarded the classification row cannot prove that
                // a note tombstone is safe to persist in plaintext.
                Ok(exists != 0 && protected == 0)
            }
            "attachment" => {
                let (classified, protected): (i64, i64) = transaction.query_row(
                    "SELECT
                        EXISTS(SELECT 1 FROM attachments WHERE id = ?1)
                            OR EXISTS(SELECT 1 FROM attachment_operations WHERE attachment_id = ?1),
                        EXISTS(
                            SELECT 1
                            FROM attachments attachment
                            JOIN vault_note_roots root ON root.note_id = attachment.note_id
                            WHERE attachment.id = ?1
                            UNION ALL
                            SELECT 1
                            FROM attachment_operations operation
                            JOIN vault_note_roots root ON root.note_id = operation.note_id
                            WHERE operation.attachment_id = ?1
                        )",
                    [entity_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                // A committed ordinary remove may be replayed after its
                // attachment row is gone; the durable attachment operation is
                // sufficient classification to preserve that sync tombstone.
                Ok(classified != 0 && protected == 0)
            }
            _ => Ok(true),
        }
    }

    #[cfg(feature = "remote-sync")]
    fn clear_sync_payloads_for_entity(
        &self,
        transaction: &Transaction<'_>,
        entity_type: &str,
        entity_id: &str,
    ) -> StorageResult<()> {
        transaction.execute(
            "DELETE FROM sync_queue
             WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
        )?;
        Ok(())
    }

    #[cfg(feature = "remote-sync")]
    fn enqueue_sync_payload(
        &self,
        transaction: &Transaction<'_>,
        entity_type: &str,
        entity_id: &str,
        operation: &str,
        payload_json: &str,
    ) -> StorageResult<()> {
        let now = unix_timestamp();
        let checksum = sha256_bytes(payload_json.as_bytes());
        transaction.execute(
            "DELETE FROM sync_queue
             WHERE entity_type = ?1 AND entity_id = ?2 AND state = 'pending'",
            params![entity_type, entity_id],
        )?;
        transaction.execute(
            "INSERT INTO sync_queue(
                id, entity_type, entity_id, operation, payload_json, checksum,
                state, attempt_count, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', 0, ?7, ?7)",
            params![
                new_id("sync"),
                entity_type,
                entity_id,
                operation,
                payload_json,
                checksum,
                now
            ],
        )?;
        Ok(())
    }
    pub(super) fn note_from_connection(
        &self,
        connection: &Connection,
        note_id: &NoteId,
    ) -> StorageResult<Option<Note>> {
        let mut note = connection
            .query_row(
                "SELECT
                    n.id, n.notebook_id, n.title, n.title_style, n.body, n.editor_state, n.plain_text, n.pinned, n.archived, n.trashed, n.purge_after,
                    n.created_at, n.updated_at,
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
                 FROM notes n WHERE n.id = ?1",
                [note_id],
                note_from_row,
            )
            .optional()?;
        if let Some(note) = note.as_mut()
            && super::protection::protected_note_exists(connection, &note.id)?
        {
            note.protection = crate::domain::NoteProtectionState::Locked;
        }
        Ok(note)
    }
    pub(super) fn replace_note_tags(
        &self,
        connection: &Connection,
        note_id: &NoteId,
        tags: &[String],
    ) -> StorageResult<()> {
        connection.execute("DELETE FROM note_tags WHERE note_id = ?1", [note_id])?;
        for name in normalized_tags(tags) {
            let tag_id = connection
                .query_row(
                    "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                    [&name],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .unwrap_or_else(|| new_id("tag"));
            connection.execute(
                "INSERT OR IGNORE INTO tags(id, name, created_at) VALUES (?1, ?2, ?3)",
                params![tag_id, name, unix_timestamp()],
            )?;
            connection.execute(
                "INSERT OR IGNORE INTO note_tags(note_id, tag_id) VALUES (?1, ?2)",
                params![note_id, tag_id],
            )?;
        }
        Ok(())
    }
    pub(super) fn refresh_search_index(
        &self,
        connection: &Connection,
        note: &Note,
    ) -> StorageResult<()> {
        let attachment_names = note.attachment_names.join("\u{1f}");
        let fts_rowid: i64 = connection.query_row(
            "SELECT fts_rowid FROM notes WHERE id = ?1",
            [&note.id],
            |row| row.get(0),
        )?;
        connection.execute("DELETE FROM notes_fts WHERE rowid = ?1", [fts_rowid])?;
        connection.execute(
            "INSERT INTO notes_fts(rowid, note_id, title, body, tags, attachment_names)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                fts_rowid,
                note.id,
                note.title,
                note.searchable_body(),
                note.tags_text(),
                attachment_names
            ],
        )?;
        Ok(())
    }
}

const NOTE_LIST_TITLE_KEY_SQL: &str =
    "LOWER(CASE WHEN TRIM(n.title) = '' THEN 'Untitled note' ELSE TRIM(n.title) END)";

fn note_list_page_in_transaction(
    transaction: &Transaction<'_>,
    query: &NoteListQuery,
    parsed_search: &ParsedSearch,
    cursor: Option<&NoteListCursor>,
    include_total: bool,
) -> StorageResult<NoteListPage> {
    let preview_fts_expression = super::search_query::preview_fts_expression(parsed_search);
    let (base_where, mut page_parameters) = note_list_where(query, parsed_search, 2);
    let mut cursor_clause = String::new();
    if let Some(cursor) = cursor {
        let first_parameter = page_parameters.len() + 2;
        let (clause, parameters) =
            note_list_cursor_predicate(query.sort, cursor, first_parameter);
        cursor_clause = format!(" AND {clause}");
        page_parameters.extend(parameters);
    }
    let limit_parameter = page_parameters.len() + 2;
    let order_by = note_list_order_by(query.sort);
    let query_sql = format!(
        "SELECT
            n.id, n.notebook_id, n.title,
            CASE
                WHEN ?1 = '' THEN substr(n.plain_text, 1, 600)
                ELSE COALESCE((
                    SELECT snippet(notes_fts, 2, '', '', ' … ', 32)
                    FROM notes_fts
                    WHERE rowid = n.fts_rowid AND notes_fts MATCH ?1
                    LIMIT 1
                ), substr(n.plain_text, 1, 600))
            END AS preview_text,
            n.pinned, n.archived, n.trashed,
            n.created_at, n.updated_at,
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
            ), '') AS attachment_names,
            EXISTS(SELECT 1 FROM vault_note_roots root WHERE root.note_id = n.id) AS protected,
            {NOTE_LIST_TITLE_KEY_SQL} AS title_key
         FROM notes n
         WHERE {base_where}{cursor_clause}
         ORDER BY {order_by}
         LIMIT ?{limit_parameter}"
    );

    let mut values = Vec::with_capacity(page_parameters.len() + 2);
    values.push(Value::from(preview_fts_expression));
    values.extend(page_parameters);
    values.push(Value::from((NOTE_LIST_PAGE_SIZE + 1) as i64));
    let mut statement = transaction.prepare(&query_sql)?;
    let rows = statement.query_map(params_from_iter(values.iter()), |row| {
        Ok((note_summary_from_row(row)?, row.get::<_, String>(12)?))
    })?;
    let mut loaded = rows.collect::<Result<Vec<_>, _>>()?;
    let has_more = loaded.len() > NOTE_LIST_PAGE_SIZE;
    if has_more {
        loaded.pop();
    }
    let next_cursor = if has_more {
        loaded.last().map(|(note, title_key)| NoteListCursor {
            pinned: note.pinned,
            updated_at: note.updated_at,
            created_at: note.created_at,
            title_key: title_key.clone(),
            id: note.id.clone(),
        })
    } else {
        None
    };
    let notes = loaded.into_iter().map(|(note, _)| note).collect();
    drop(statement);

    let total_count = if include_total {
        Some(note_list_total_count(transaction, query, parsed_search)?)
    } else {
        None
    };

    Ok(NoteListPage {
        notes,
        next_cursor,
        total_count,
    })
}

fn note_list_page_start_cursor(
    transaction: &Transaction<'_>,
    query: &NoteListQuery,
    parsed_search: &ParsedSearch,
    page_index: usize,
) -> StorageResult<Option<NoteListCursor>> {
    if page_index == 0 {
        return Ok(None);
    }

    let row_offset = page_index
        .saturating_mul(NOTE_LIST_PAGE_SIZE)
        .saturating_sub(1);
    let (base_where, mut parameters) = note_list_where(query, parsed_search, 1);
    let offset_parameter = parameters.len() + 1;
    let order_by = note_list_order_by(query.sort);
    let sql = format!(
        "SELECT
            n.pinned,
            n.updated_at,
            n.created_at,
            {NOTE_LIST_TITLE_KEY_SQL} AS title_key,
            n.id
         FROM notes n
         WHERE {base_where}
         ORDER BY {order_by}
         LIMIT 1 OFFSET ?{offset_parameter}"
    );
    parameters.push(Value::from(i64::try_from(row_offset).unwrap_or(i64::MAX)));

    Ok(transaction
        .query_row(&sql, params_from_iter(parameters.iter()), |row| {
            Ok(NoteListCursor {
                pinned: row.get::<_, i64>(0)? != 0,
                updated_at: row.get(1)?,
                created_at: row.get(2)?,
                title_key: row.get(3)?,
                id: row.get(4)?,
            })
        })
        .optional()?)
}

fn note_list_total_count(
    transaction: &Transaction<'_>,
    query: &NoteListQuery,
    parsed_search: &ParsedSearch,
) -> StorageResult<usize> {
    let (count_where, count_parameters) = note_list_where(query, parsed_search, 1);
    let count_sql = format!("SELECT COUNT(*) FROM notes n WHERE {count_where}");
    let count: i64 = transaction.query_row(
        &count_sql,
        params_from_iter(count_parameters.iter()),
        |row| row.get(0),
    )?;
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

fn note_list_where(
    query: &NoteListQuery,
    parsed_search: &ParsedSearch,
    first_parameter: usize,
) -> (String, Vec<Value>) {
    let mut clauses = vec![match query.section {
        Section::Notes => "n.trashed = 0 AND n.archived = 0".to_string(),
        Section::Pinned => {
            "n.trashed = 0 AND n.archived = 0 AND n.pinned = 1".to_string()
        }
        Section::Archive => "n.trashed = 0 AND n.archived = 1".to_string(),
        Section::Trash => "n.trashed = 1".to_string(),
    }];
    let mut parameters = Vec::new();
    let mut next_parameter = first_parameter;

    if let Some(notebook_id) = query.notebook_id.as_deref() {
        clauses.push(format!("n.notebook_id = ?{next_parameter}"));
        parameters.push(Value::from(notebook_id.to_string()));
        next_parameter += 1;
    }

    let (search_predicate, search_parameters) =
        super::search_query::candidate_sql_predicate(parsed_search, "n", next_parameter);
    clauses.push(search_predicate);
    next_parameter += search_parameters.len();
    parameters.extend(search_parameters.into_iter().map(Value::from));

    append_note_filter_predicates(
        &query.filters,
        &mut clauses,
        &mut parameters,
        &mut next_parameter,
    );

    (clauses.join(" AND "), parameters)
}

fn append_note_filter_predicates(
    filters: &NoteFilters,
    clauses: &mut Vec<String>,
    parameters: &mut Vec<Value>,
    next_parameter: &mut usize,
) {
    if filters.pinned {
        clauses.push("n.pinned = 1".to_string());
    }
    if filters.with_attachments {
        clauses.push(
            "EXISTS(SELECT 1 FROM attachments filter_attachment WHERE filter_attachment.note_id = n.id AND filter_attachment.embedded = 1)"
                .to_string(),
        );
    }
    if filters.without_tags {
        clauses.push(
            "NOT EXISTS(SELECT 1 FROM note_tags filter_note_tag WHERE filter_note_tag.note_id = n.id)"
                .to_string(),
        );
    }
    for tag in &filters.tags {
        clauses.push(format!(
            "EXISTS(
                SELECT 1
                FROM note_tags filter_note_tag
                JOIN tags filter_tag ON filter_tag.id = filter_note_tag.tag_id
                WHERE filter_note_tag.note_id = n.id
                  AND filter_tag.name = ?{} COLLATE NOCASE
            )",
            *next_parameter
        ));
        parameters.push(Value::from(tag.clone()));
        *next_parameter += 1;
    }
}

fn note_list_order_by(sort: NoteSort) -> &'static str {
    match sort {
        NoteSort::UpdatedNewest => "n.pinned DESC, n.updated_at DESC, n.id DESC",
        NoteSort::UpdatedOldest => "n.pinned DESC, n.updated_at ASC, n.id ASC",
        NoteSort::CreatedNewest => "n.pinned DESC, n.created_at DESC, n.id DESC",
        NoteSort::TitleAscending => {
            "n.pinned DESC, LOWER(CASE WHEN TRIM(n.title) = '' THEN 'Untitled note' ELSE TRIM(n.title) END) ASC, n.id ASC"
        }
        NoteSort::TitleDescending => {
            "n.pinned DESC, LOWER(CASE WHEN TRIM(n.title) = '' THEN 'Untitled note' ELSE TRIM(n.title) END) DESC, n.id DESC"
        }
    }
}

fn note_list_cursor_predicate(
    sort: NoteSort,
    cursor: &NoteListCursor,
    first_parameter: usize,
) -> (String, Vec<Value>) {
    let pinned_parameter = first_parameter;
    let key_parameter = first_parameter + 1;
    let id_parameter = first_parameter + 2;
    let (key_sql, comparison, key_value) = match sort {
        NoteSort::UpdatedNewest => ("n.updated_at", "<", Value::from(cursor.updated_at)),
        NoteSort::UpdatedOldest => ("n.updated_at", ">", Value::from(cursor.updated_at)),
        NoteSort::CreatedNewest => ("n.created_at", "<", Value::from(cursor.created_at)),
        NoteSort::TitleAscending => (
            NOTE_LIST_TITLE_KEY_SQL,
            ">",
            Value::from(cursor.title_key.clone()),
        ),
        NoteSort::TitleDescending => (
            NOTE_LIST_TITLE_KEY_SQL,
            "<",
            Value::from(cursor.title_key.clone()),
        ),
    };
    let id_comparison = if matches!(sort, NoteSort::UpdatedOldest | NoteSort::TitleAscending) {
        ">"
    } else {
        "<"
    };
    let clause = format!(
        "(
            n.pinned < ?{pinned_parameter}
            OR (
                n.pinned = ?{pinned_parameter}
                AND (
                    {key_sql} {comparison} ?{key_parameter}
                    OR ({key_sql} = ?{key_parameter} AND n.id {id_comparison} ?{id_parameter})
                )
            )
        )"
    );
    (
        clause,
        vec![
            Value::from(if cursor.pinned { 1_i64 } else { 0_i64 }),
            key_value,
            Value::from(cursor.id.as_str().to_string()),
        ],
    )
}

fn duplicated_note_title(display_title: &str) -> String {
    let title = display_title.trim();
    if title.is_empty() {
        "Untitled note copy".to_string()
    } else {
        format!("{title} copy")
    }
}

fn remap_duplicated_editor_state(
    serialized: &str,
    attachment_ids: &HashMap<String, String>,
    source_note_id: &NoteId,
    duplicate_note_id: &NoteId,
) -> StorageResult<String> {
    let mut envelope: serde_json::Value = serde_json::from_str(serialized)?;
    remap_duplicated_document_value(
        &mut envelope,
        attachment_ids,
        source_note_id.as_str(),
        duplicate_note_id.as_str(),
    );
    let remapped = serde_json::to_string(&envelope)?;
    analyze_editor_state(&remapped).map_err(StorageError::from)?;
    Ok(remapped)
}

fn remap_duplicated_document_value(
    value: &mut serde_json::Value,
    attachment_ids: &HashMap<String, String>,
    source_note_id: &str,
    duplicate_note_id: &str,
) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                remap_duplicated_document_value(
                    value,
                    attachment_ids,
                    source_note_id,
                    duplicate_note_id,
                );
            }
        }
        serde_json::Value::Object(object) => {
            let value_type = object
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            if let Some(attrs) = object
                .get_mut("attrs")
                .and_then(serde_json::Value::as_object_mut)
            {
                match value_type.as_deref() {
                    Some("attachment") => {
                        if let Some(serde_json::Value::String(attachment_id)) =
                            attrs.get_mut("attachmentId")
                            && let Some(remapped) = attachment_ids.get(attachment_id)
                        {
                            *attachment_id = remapped.clone();
                        }
                    }
                    Some("note_link") => {
                        if let Some(serde_json::Value::String(note_id)) = attrs.get_mut("noteId")
                            && note_id == source_note_id
                        {
                            *note_id = duplicate_note_id.to_string();
                        }
                    }
                    Some("media_link") => {
                        let self_media_attachment = attrs
                            .get("noteId")
                            .and_then(serde_json::Value::as_str)
                            .filter(|note_id| *note_id == source_note_id)
                            .and_then(|_| attrs.get("attachmentId"))
                            .and_then(serde_json::Value::as_str)
                            .and_then(|attachment_id| attachment_ids.get(attachment_id))
                            .cloned();
                        if let Some(remapped_attachment_id) = self_media_attachment {
                            if let Some(serde_json::Value::String(note_id)) = attrs.get_mut("noteId") {
                                *note_id = duplicate_note_id.to_string();
                            }
                            if let Some(serde_json::Value::String(attachment_id)) =
                                attrs.get_mut("attachmentId")
                            {
                                *attachment_id = remapped_attachment_id;
                            }
                        }
                    }
                    _ => {}
                }
            }
            for child in object.values_mut() {
                remap_duplicated_document_value(
                    child,
                    attachment_ids,
                    source_note_id,
                    duplicate_note_id,
                );
            }
        }
        _ => {}
    }
}

pub(super) fn note_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<NoteSummary> {
    let tag_names: String = row.get(9)?;
    let attachment_names: String = row.get(10)?;
    Ok(NoteSummary {
        id: row.get(0)?,
        protection: if row.get::<_, i64>(11)? != 0 {
            crate::domain::NoteProtectionState::Locked
        } else {
            crate::domain::NoteProtectionState::Unprotected
        },
        notebook_id: row.get(1)?,
        title: row.get(2)?,
        preview_text: row.get(3)?,
        pinned: row.get::<_, i64>(4)? != 0,
        archived: row.get::<_, i64>(5)? != 0,
        trashed: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        tags: split_tags(&tag_names),
        attachment_names: split_attachment_names(&attachment_names),
    })
}

pub(super) fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    let title_style_json: String = row.get(3)?;
    let title_style = serde_json::from_str::<NoteTitleStyle>(&title_style_json)
        .unwrap_or_default()
        .normalized();
    let tag_names: String = row.get(13)?;
    let attachment_names: String = row.get(14)?;
    Ok(Note {
        id: row.get(0)?,
        protection: crate::domain::NoteProtectionState::Unprotected,
        notebook_id: row.get(1)?,
        title: row.get(2)?,
        title_style,
        document: std::sync::Arc::new(crate::domain::NoteDocument::new(
            row.get(4)?,
            std::sync::Arc::new(row.get(5)?),
            row.get(6)?,
        )),
        pinned: row.get::<_, i64>(7)? != 0,
        archived: row.get::<_, i64>(8)? != 0,
        trashed: row.get::<_, i64>(9)? != 0,
        purge_after: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        tags: split_tags(&tag_names),
        attachment_names: split_attachment_names(&attachment_names),
    })
}

pub(super) fn split_attachment_names(names: &str) -> Vec<String> {
    names
        .split(char::from(31))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, thread, time::Duration};

    use rusqlite::{TransactionBehavior, params};
    use serde_json::{Value, json};

    use crate::domain::{Attachment, SourceKind};
    use crate::storage::{NoteSaveScope, RevisionMode, new_id};

    use super::*;
    use super::super::attachments::blob_relative_path;

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

    fn test_attachment(
        note_id: &NoteId,
        id: &str,
        file_name: &str,
        sha256: &str,
        source_attachment_id: Option<&str>,
        embedded: bool,
        now: i64,
    ) -> Attachment {
        Attachment {
            id: id.to_string(),
            note_id: note_id.clone(),
            file_name: file_name.to_string(),
            mime_type: Some(if file_name.ends_with(".pdf") {
                "application/pdf"
            } else {
                "image/png"
            }
            .to_string()),
            byte_size: 16,
            sha256: sha256.to_string(),
            image_pixel_width: file_name.ends_with(".png").then_some(64),
            image_pixel_height: file_name.ends_with(".png").then_some(64),
            image_format: file_name.ends_with(".png").then(|| "png".to_string()),
            image_orientation: file_name.ends_with(".png").then_some(1),
            image_has_alpha: file_name.ends_with(".png").then_some(true),
            image_animated: file_name.ends_with(".png").then_some(false),
            source_attachment_id: source_attachment_id.map(str::to_string),
            image_edit_recipe: source_attachment_id.map(|_| "{\"rotation\":90}".to_string()),
            embedded,
            unreferenced_at: (!embedded).then_some(now),
            created_at: now,
            updated_at: now,
        }
    }

    fn attachment_node(id: &str, kind: &str, display_name: &str, media_type: &str) -> Value {
        json!({
            "type": "attachment",
            "attrs": {
                "attachmentId": id,
                "kind": kind,
                "displayName": display_name,
                "mediaType": media_type,
                "byteSize": 16,
                "caption": "",
                "altText": "",
                "alignment": "center",
                "widthPercent": 100,
                "sizeMode": "auto",
                "mediaAnnotations": null,
                "mediaTrimStartMs": null,
                "mediaTrimEndMs": null,
                "nodeSchemaVersion": 2
            }
        })
    }

    #[test]
    fn duplicate_editor_remap_keeps_external_media_links_and_remaps_self_media_atomically() {
        let source_note_id = crate::storage::new_note_id();
        let duplicate_note_id = crate::storage::new_note_id();
        let external_note_id = crate::storage::new_note_id();
        let source_attachment_id = new_id("att");
        let duplicate_attachment_id = new_id("att");
        let external_attachment_id = new_id("att");
        let attachment_ids = HashMap::from([(
            source_attachment_id.clone(),
            duplicate_attachment_id.clone(),
        )]);
        let editor_state = serde_json::to_string(&json!({
            "schema_version": 2,
            "editor_engine": "prosemirror",
            "document_schema": "rustnote",
            "document_schema_version": 6,
            "document": {
                "type": "doc",
                "content": [
                    attachment_node(&source_attachment_id, "audio", "clip.mp3", "audio/mpeg"),
                    {
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "self media",
                            "marks": [{
                                "type": "media_link",
                                "attrs": {
                                    "noteId": source_note_id.as_str(),
                                    "attachmentId": source_attachment_id.as_str(),
                                    "startMs": 1000,
                                    "endMs": null
                                }
                            }]
                        }]
                    },
                    {
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "external media",
                            "marks": [{
                                "type": "media_link",
                                "attrs": {
                                    "noteId": external_note_id.as_str(),
                                    "attachmentId": external_attachment_id.as_str(),
                                    "startMs": 2000,
                                    "endMs": 3000
                                }
                            }]
                        }]
                    }
                ]
            }
        }))
        .expect("serialize media-link editor state");

        let remapped = remap_duplicated_editor_state(
            &editor_state,
            &attachment_ids,
            &source_note_id,
            &duplicate_note_id,
        )
        .expect("remap duplicated editor state");
        let remapped: Value = serde_json::from_str(&remapped).expect("parse remapped editor state");
        let content = remapped["document"]["content"]
            .as_array()
            .expect("document content");
        assert_eq!(
            content[0]["attrs"]["attachmentId"].as_str(),
            Some(duplicate_attachment_id.as_str())
        );
        let self_media = &content[1]["content"][0]["marks"][0]["attrs"];
        assert_eq!(self_media["noteId"].as_str(), Some(duplicate_note_id.as_str()));
        assert_eq!(
            self_media["attachmentId"].as_str(),
            Some(duplicate_attachment_id.as_str())
        );
        let external_media = &content[2]["content"][0]["marks"][0]["attrs"];
        assert_eq!(external_media["noteId"].as_str(), Some(external_note_id.as_str()));
        assert_eq!(
            external_media["attachmentId"].as_str(),
            Some(external_attachment_id.as_str())
        );
    }

    #[test]
    fn duplicate_note_remaps_owned_resources_without_aliasing_attachment_history() {
        let (storage, root) = test_storage("duplicate-note");
        let parent = storage.create_note(None).expect("create parent note");
        let mut source_note = storage.create_note(None).expect("create source note");
        source_note.title = "Research note".to_string();
        source_note.tags = vec!["alpha".to_string(), "beta".to_string()];
        storage
            .set_note_parent(&source_note.id, &parent.id)
            .expect("set source parent");
        let source = storage
            .create_source_and_attach(
                &source_note.id,
                SourceKind::Journal,
                "Reference",
                Some("doi:10.1000/example"),
            )
            .expect("attach source");
        storage
            .add_citation(&source_note.id, &source.id, "p. 42")
            .expect("add citation");

        let now = unix_timestamp();
        let original_image_id = new_id("att");
        let edited_image_id = new_id("att");
        let pdf_id = new_id("att");
        let stale_id = new_id("att");
        let original_image_hash = "1".repeat(64);
        let edited_image_hash = "2".repeat(64);
        let pdf_hash = "3".repeat(64);
        let stale_hash = "4".repeat(64);

        {
            let mut connection = storage.connection().expect("open test connection");
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("start attachment transaction");
            for attachment in [
                test_attachment(
                    &source_note.id,
                    &original_image_id,
                    "original.png",
                    &original_image_hash,
                    None,
                    true,
                    now,
                ),
                test_attachment(
                    &source_note.id,
                    &edited_image_id,
                    "edited.png",
                    &edited_image_hash,
                    Some(&original_image_id),
                    true,
                    now + 1,
                ),
                test_attachment(
                    &source_note.id,
                    &pdf_id,
                    "paper.pdf",
                    &pdf_hash,
                    None,
                    true,
                    now + 2,
                ),
                test_attachment(
                    &source_note.id,
                    &stale_id,
                    "stale.png",
                    &stale_hash,
                    None,
                    false,
                    now + 3,
                ),
            ] {
                storage
                    .insert_attachment(&transaction, &attachment)
                    .expect("insert test attachment");
            }
            transaction
                .execute(
                    "INSERT INTO pdf_annotation_pages(attachment_id, page_index, annotations_json, updated_at)\n                     VALUES (?1, 2, ?2, ?3)",
                    params![&pdf_id, r#"[{"id":"a1","kind":"highlight"}]"#, now],
                )
                .expect("insert pdf annotation page");
            transaction.commit().expect("commit attachment setup");
        }

        let editor_state = serde_json::to_string(&json!({
            "schema_version": 2,
            "editor_engine": "prosemirror",
            "document_schema": "rustnote",
            "document_schema_version": 6,
            "document": {
                "type": "doc",
                "content": [
                    {
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "self",
                            "marks": [{
                                "type": "note_link",
                                "attrs": {"noteId": source_note.id.as_str()}
                            }]
                        }]
                    },
                    attachment_node(&edited_image_id, "image", "edited.png", "image/png"),
                    attachment_node(&pdf_id, "pdf", "paper.pdf", "application/pdf")
                ]
            }
        }))
        .expect("serialize source editor state");
        let projections = crate::editor_projection::derive_editor_projections(&editor_state)
            .expect("project source editor state");
        source_note.replace_document(
            projections.markdown,
            editor_state,
            projections.plain_text,
        );
        source_note.updated_at = now + 4;
        source_note = storage
            .save_note_owned_with_revision_status_scoped(
                source_note,
                RevisionMode::Checkpoint,
                NoteSaveScope::ALL,
            )
            .expect("save source document")
            .0;

        let duplicate = storage
            .duplicate_note(&source_note.id)
            .expect("duplicate note with owned resources");

        assert_ne!(duplicate.id, source_note.id);
        assert_eq!(duplicate.notebook_id, source_note.notebook_id);
        let mut source_tags = source_note.tags.clone();
        let mut duplicate_tags = duplicate.tags.clone();
        source_tags.sort();
        duplicate_tags.sort();
        assert_eq!(duplicate_tags, source_tags);
        assert!(!duplicate.pinned);
        assert!(!duplicate.archived);
        assert!(!duplicate.trashed);
        assert_eq!(duplicate.purge_after, None);

        let duplicate_attachments = storage
            .list_attachments(&duplicate.id)
            .expect("list duplicate attachments");
        assert_eq!(duplicate_attachments.len(), 3, "stale attachment history must not be copied");
        assert!(duplicate_attachments.iter().all(|attachment| attachment.id != stale_id));
        let duplicate_original = duplicate_attachments
            .iter()
            .find(|attachment| attachment.file_name == "original.png")
            .expect("duplicated original image");
        let duplicate_edited = duplicate_attachments
            .iter()
            .find(|attachment| attachment.file_name == "edited.png")
            .expect("duplicated edited image");
        let duplicate_pdf = duplicate_attachments
            .iter()
            .find(|attachment| attachment.file_name == "paper.pdf")
            .expect("duplicated pdf");
        assert_ne!(duplicate_original.id, original_image_id);
        assert_ne!(duplicate_edited.id, edited_image_id);
        assert_ne!(duplicate_pdf.id, pdf_id);
        assert_eq!(duplicate_original.sha256, original_image_hash);
        assert_eq!(duplicate_edited.sha256, edited_image_hash);
        assert_eq!(duplicate_pdf.sha256, pdf_hash);
        assert_eq!(
            duplicate_edited.source_attachment_id.as_deref(),
            Some(duplicate_original.id.as_str()),
            "image edit ancestry must point at the duplicate-owned source"
        );

        let duplicate_analysis = analyze_editor_state(duplicate.editor_state())
            .expect("analyze duplicated editor state");
        assert!(duplicate_analysis.attachment_ids.contains(&duplicate_edited.id));
        assert!(duplicate_analysis.attachment_ids.contains(&duplicate_pdf.id));
        assert!(!duplicate_analysis.attachment_ids.contains(&edited_image_id));
        assert!(!duplicate_analysis.attachment_ids.contains(&pdf_id));
        assert_eq!(duplicate_analysis.note_links, vec![duplicate.id.clone()]);

        let hierarchy = storage
            .note_hierarchy(&duplicate.id)
            .expect("load duplicate hierarchy");
        assert_eq!(hierarchy.parent.map(|summary| summary.id), Some(parent.id));

        let duplicate_sources = storage
            .list_note_sources(&duplicate.id)
            .expect("load duplicate sources");
        assert_eq!(duplicate_sources.len(), 1);
        assert_eq!(duplicate_sources[0].source.id, source.id);
        assert_eq!(duplicate_sources[0].citations.len(), 1);
        assert_eq!(duplicate_sources[0].citations[0].locator, "p. 42");

        let connection = storage.connection().expect("open verification connection");
        let annotation: (String, i64) = connection
            .query_row(
                "SELECT annotations_json, page_index\n                 FROM pdf_annotation_pages WHERE attachment_id = ?1",
                [&duplicate_pdf.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("duplicated pdf annotation page");
        assert_eq!(annotation.0, r#"[{"id":"a1","kind":"highlight"}]"#);
        assert_eq!(annotation.1, 2);

        cleanup(storage, root);
    }

    #[test]
    fn expired_trash_purge_transfers_ordinary_blob_ownership_before_cascade_commit() {
        let (storage, root) = test_storage("purge-owned-orphan");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("purge-owned-orphan.txt");
        fs::write(&source, b"purge ownership continuity").expect("write attachment");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        storage
            .connection()
            .expect("trash connection")
            .execute(
                "UPDATE notes SET trashed = 1, purge_after = ?2 WHERE id = ?1",
                params![&note.id, unix_timestamp() - 1],
            )
            .expect("expire note");

        assert_eq!(storage.purge_expired_notes().expect("purge expired note"), 1);
        assert!(storage.get_note(&note.id).expect("note lookup").is_none());
        let connection = storage.connection().expect("owner connection");
        let orphan_owner_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging
                 WHERE kind = 'portability_blob' AND relative_path LIKE '.orphaned/%'",
                [],
                |row| row.get(0),
            )
            .expect("orphan owner count");
        assert_eq!(
            orphan_owner_count, 1,
            "batch purge must leave the quarantined plaintext under durable ownership"
        );
        let orphan_path: String = connection
            .query_row(
                "SELECT relative_path FROM managed_plaintext_staging
                 WHERE kind = 'portability_blob' AND relative_path LIKE '.orphaned/%'",
                [],
                |row| row.get(0),
            )
            .expect("orphan owner path");
        drop(connection);
        assert!(
            orphan_path.ends_with(&attachment.sha256),
            "quarantine ownership must identify the purged attachment blob"
        );
        assert!(storage.attachments_directory.join(&orphan_path).exists());
        cleanup(storage, root);
    }

    #[test]
    fn permanent_delete_transfers_revision_only_blob_ownership_before_cascade_commit() {
        let (storage, root) = test_storage("delete-revision-only-owned-orphan");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("historical-only.txt");
        fs::write(&source, b"revision-only attachment lifetime")
            .expect("write historical attachment");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        let canonical = storage
            .attachment_file_path(&attachment)
            .expect("canonical attachment path");

        let revision_id = new_id("rev-delete-owner");
        let connection = storage.connection().expect("revision fixture connection");
        connection
            .execute(
                "INSERT INTO note_revisions(
                    id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at
                 ) VALUES (?1, ?2, 1, ?3, '{}', 2, ?4)",
                params![&revision_id, &note.id, new_id("checksum"), unix_timestamp()],
            )
            .expect("insert revision fixture");
        connection
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
            .expect("insert revision attachment ref");
        drop(connection);

        storage
            .remove_attachment(&attachment.id)
            .expect("remove current attachment reference");
        assert!(
            canonical.exists(),
            "revision-only reference must retain canonical plaintext before hard delete"
        );
        storage
            .connection()
            .expect("trash connection")
            .execute(
                "UPDATE notes SET trashed = 1, purge_after = ?2 WHERE id = ?1",
                params![&note.id, unix_timestamp() - 1],
            )
            .expect("mark note trashed");

        storage
            .permanently_delete_trashed_note(&note.id)
            .expect("hard delete revision-only blob owner");
        assert!(
            !canonical.exists(),
            "revision-only plaintext must leave canonical path after hard delete"
        );
        let connection = storage.connection().expect("owner verification");
        let orphan_path: String = connection
            .query_row(
                "SELECT relative_path FROM managed_plaintext_staging
                 WHERE kind = 'portability_blob'
                   AND relative_path LIKE '.orphaned/%'
                   AND relative_path LIKE '%' || ?1
                 LIMIT 1",
                [&attachment.sha256],
                |row| row.get(0),
            )
            .expect("durable orphan owner");
        drop(connection);
        assert!(storage.attachments_directory.join(orphan_path).exists());
        cleanup(storage, root);
    }

    #[test]
    fn expired_trash_purge_fails_closed_on_protected_ordinary_revision_residue() {
        let (storage, root) = test_storage("protected-purge-revision-residue");
        let note = storage.create_note(None).expect("create note");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        let revision_id = new_id("rev-protected-residue");
        let connection = storage.connection().expect("residue fixture connection");
        connection
            .execute(
                "INSERT INTO note_revisions(
                    id, note_id, revision_no, checksum, snapshot_json, byte_size, created_at
                 ) VALUES (?1, ?2, 1, ?3, '{}', 2, ?4)",
                params![&revision_id, &note.id, new_id("checksum"), unix_timestamp()],
            )
            .expect("insert impossible protected revision residue");
        connection
            .execute(
                "INSERT INTO revision_attachment_refs(
                    revision_id, attachment_id, sha256, byte_size
                 ) VALUES (?1, 'historical-only', ?2, 1)",
                params![&revision_id, "0".repeat(64)],
            )
            .expect("insert impossible protected revision attachment residue");
        connection
            .execute(
                "UPDATE notes SET trashed = 1, purge_after = ?2 WHERE id = ?1",
                params![&note.id, unix_timestamp() - 1],
            )
            .expect("expire protected note");
        drop(connection);

        assert!(
            storage.purge_expired_notes().is_err(),
            "batch purge must fail closed rather than cascade impossible protected revision residue"
        );
        assert!(
            storage.get_note(&note.id).expect("note lookup").is_some(),
            "failed purge must preserve the protected note"
        );
        let residue_count: i64 = storage
            .connection()
            .expect("residue verification")
            .query_row(
                "SELECT COUNT(*) FROM revision_attachment_refs WHERE revision_id = ?1",
                [&revision_id],
                |row| row.get(0),
            )
            .expect("residue count");
        assert_eq!(residue_count, 1, "fail-closed purge must preserve ownership evidence");
        cleanup(storage, root);
    }

    #[cfg(feature = "remote-sync")]
    #[test]
    fn ordinary_note_and_attachment_sync_payloads_remain_enabled() {
        let (storage, root) = test_storage("ordinary-sync-queue-boundary");
        let note = storage.create_note(None).expect("create ordinary note");
        let source = root.join("ordinary-sync.txt");
        fs::write(&source, b"ordinary sync payload").expect("write attachment source");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add ordinary attachment");

        let mut connection = storage.connection().expect("ordinary sync connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("ordinary sync transaction");
        transaction.execute("DELETE FROM sync_queue", []).expect("clear setup sync rows");
        storage
            .enqueue_sync_upsert(&transaction, "note", note.id.as_str(), &note)
            .expect("ordinary note upsert must remain enabled");
        storage
            .enqueue_sync_delete(&transaction, "note", note.id.as_str())
            .expect("ordinary note delete must remain enabled");
        storage
            .enqueue_sync_upsert(&transaction, "attachment", &attachment.id, &attachment)
            .expect("ordinary attachment upsert must remain enabled");
        storage
            .enqueue_sync_delete(&transaction, "attachment", &attachment.id)
            .expect("ordinary attachment delete must remain enabled");
        transaction.commit().expect("commit ordinary sync attempts");

        let connection = storage.connection().expect("ordinary sync verification");
        let note_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_queue WHERE entity_type = 'note' AND entity_id = ?1",
                [note.id.as_str()],
                |row| row.get(0),
            )
            .expect("ordinary note sync count");
        let attachment_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sync_queue WHERE entity_type = 'attachment' AND entity_id = ?1",
                [&attachment.id],
                |row| row.get(0),
            )
            .expect("ordinary attachment sync count");
        assert_eq!(note_rows, 1, "ordinary note sync tombstone must remain queued");
        assert_eq!(attachment_rows, 1, "ordinary attachment sync tombstone must remain queued");
        cleanup(storage, root);
    }

    #[cfg(feature = "remote-sync")]
    #[test]
    fn protected_note_and_attachment_payloads_never_enter_sync_queue() {
        let (storage, root) = test_storage("protected-sync-queue-boundary");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("protected-sync.txt");
        fs::write(&source, b"protected sync payload").expect("write attachment source");
        let attachment = storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");

        let mut connection = storage.connection().expect("sync boundary connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("sync boundary transaction");
        storage
            .enqueue_sync_upsert(
                &transaction,
                "note",
                note.id.as_str(),
                &json!({"id": note.id.as_str(), "title": "must-not-persist"}),
            )
            .expect("protected note upsert must be suppressed");
        storage
            .enqueue_sync_delete(&transaction, "note", note.id.as_str())
            .expect("protected note delete must be suppressed");
        storage
            .enqueue_sync_upsert(
                &transaction,
                "attachment",
                &attachment.id,
                &json!({"id": attachment.id.as_str(), "file_name": "must-not-persist.txt"}),
            )
            .expect("protected attachment upsert must be suppressed");
        storage
            .enqueue_sync_delete(&transaction, "attachment", &attachment.id)
            .expect("protected attachment delete must be suppressed");
        transaction.commit().expect("commit suppressed sync attempts");

        let forbidden: i64 = storage
            .connection()
            .expect("sync verification")
            .query_row(
                "SELECT COUNT(*) FROM sync_queue
                 WHERE (entity_type = 'note' AND entity_id = ?1)
                    OR (entity_type = 'attachment' AND entity_id = ?2)",
                params![note.id.as_str(), &attachment.id],
                |row| row.get(0),
            )
            .expect("forbidden sync count");
        assert_eq!(forbidden, 0, "protected entities must have no plaintext sync rows");
        cleanup(storage, root);
    }

    #[test]
    fn protected_permanent_delete_does_not_reenter_note_access() {
        let (storage, root) = test_storage("protected-delete-non-reentrant");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("protected-hard-delete.txt");
        fs::write(&source, b"protected ciphertext cleanup").expect("write attachment");
        storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        let ciphertext_sha256: String = storage
            .connection()
            .expect("ciphertext connection")
            .query_row(
                "SELECT sha256 FROM vault_blob_refs WHERE note_id = ?1 LIMIT 1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("protected ciphertext reference");
        let ciphertext_path = storage
            .attachments_directory
            .join(blob_relative_path(&ciphertext_sha256).expect("ciphertext path"));
        assert!(ciphertext_path.exists());

        storage
            .connection()
            .expect("trash connection")
            .execute(
                "UPDATE notes SET trashed = 1, purge_after = ?2 WHERE id = ?1",
                params![&note.id, unix_timestamp() - 1],
            )
            .expect("mark protected note trashed");
        storage
            .permanently_delete_trashed_note(&note.id)
            .expect("protected hard delete completes without self-wait");
        assert!(storage.get_note(&note.id).expect("note lookup").is_none());
        assert!(
            !ciphertext_path.exists(),
            "protected hard delete should directly retire unreferenced ciphertext"
        );
        let canonical_owner: i64 = storage
            .connection()
            .expect("cleanup owner connection")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging WHERE relative_path = ?1",
                [blob_relative_path(&ciphertext_sha256).expect("ciphertext relative path")],
                |row| row.get(0),
            )
            .expect("cleanup owner count");
        assert_eq!(canonical_owner, 0, "ciphertext cleanup owner must retire after deletion");
        cleanup(storage, root);
    }

    #[test]
    fn protected_ciphertext_cleanup_owner_recovers_after_database_commit_crash_boundary() {
        let (storage, root) = test_storage("protected-delete-ciphertext-recovery");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("protected-delete-crash-boundary.txt");
        fs::write(&source, b"protected ciphertext crash recovery").expect("write attachment");
        storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        let ciphertext_sha256: String = storage
            .connection()
            .expect("ciphertext connection")
            .query_row(
                "SELECT sha256 FROM vault_blob_refs WHERE note_id = ?1 LIMIT 1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("protected ciphertext reference");
        let relative_path =
            blob_relative_path(&ciphertext_sha256).expect("ciphertext relative path");
        let ciphertext_path = storage.attachments_directory.join(&relative_path);
        assert!(ciphertext_path.exists());

        // Simulate process death at the hard-delete crash boundary: the durable
        // database deletion and generic canonical cleanup owner commit, but the
        // post-commit ciphertext unlink never runs. Startup must consume that
        // pre-existing cleanup obligation rather than invent ownership later.
        let mut connection = storage.connection().expect("delete transaction connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("delete transaction");
        transaction
            .execute(
                "DELETE FROM notes_fts WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = ?1)",
                [&note.id],
            )
            .expect("delete protected note FTS stub");
        transaction
            .execute(
                "DELETE FROM attachment_operations WHERE note_id = ?1",
                [&note.id],
            )
            .expect("delete protected attachment operations");
        transaction
            .execute("DELETE FROM notes WHERE id = ?1", [&note.id])
            .expect("delete protected note");
        super::super::plaintext_staging::ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(
            &transaction,
            &ciphertext_sha256,
        )
        .expect("reserve post-delete ciphertext cleanup owner");
        transaction.commit().expect("commit simulated hard delete");
        drop(connection);

        assert!(
            ciphertext_path.exists(),
            "simulated crash boundary must leave ciphertext present before restart recovery"
        );
        let owner_count: i64 = storage
            .connection()
            .expect("cleanup owner verification")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging WHERE relative_path = ?1",
                [&relative_path],
                |row| row.get(0),
            )
            .expect("cleanup owner count");
        assert_eq!(owner_count, 1, "database commit must retain one durable cleanup owner");

        drop(storage);
        let reopened = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("startup must recover pending ciphertext cleanup");
        assert!(
            !ciphertext_path.exists(),
            "startup recovery must delete ciphertext left after the committed hard-delete boundary"
        );
        let owner_count: i64 = reopened
            .connection()
            .expect("reopened owner verification")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging WHERE relative_path = ?1",
                [&relative_path],
                |row| row.get(0),
            )
            .expect("reopened cleanup owner count");
        assert_eq!(owner_count, 0, "startup recovery must retire the consumed cleanup owner");
        cleanup(reopened, root);
    }

    #[test]
    fn expired_trash_purge_directly_retires_protected_ciphertext() {
        let (storage, root) = test_storage("protected-purge-ciphertext");
        let note = storage.create_note(None).expect("create note");
        let source = root.join("protected-purge.txt");
        fs::write(&source, b"protected purge ciphertext cleanup").expect("write attachment");
        storage
            .add_attachment(&note.id, &source)
            .expect("add attachment");
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        let ciphertext_sha256: String = storage
            .connection()
            .expect("ciphertext connection")
            .query_row(
                "SELECT sha256 FROM vault_blob_refs WHERE note_id = ?1 LIMIT 1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("protected ciphertext reference");
        let ciphertext_path = storage
            .attachments_directory
            .join(blob_relative_path(&ciphertext_sha256).expect("ciphertext path"));
        assert!(ciphertext_path.exists());
        storage
            .connection()
            .expect("trash connection")
            .execute(
                "UPDATE notes SET trashed = 1, purge_after = ?2 WHERE id = ?1",
                params![&note.id, unix_timestamp() - 1],
            )
            .expect("expire protected note");

        assert_eq!(storage.purge_expired_notes().expect("purge protected note"), 1);
        assert!(storage.get_note(&note.id).expect("note lookup").is_none());
        assert!(
            !ciphertext_path.exists(),
            "batch purge should directly retire unreferenced protected ciphertext"
        );
        let canonical_owner: i64 = storage
            .connection()
            .expect("cleanup owner connection")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging WHERE relative_path = ?1",
                [blob_relative_path(&ciphertext_sha256).expect("ciphertext relative path")],
                |row| row.get(0),
            )
            .expect("cleanup owner count");
        assert_eq!(canonical_owner, 0);
        cleanup(storage, root);
    }

    #[test]
    fn permanent_delete_waits_for_plaintext_producer_then_scrubs_late_staging() {
        let (storage, root) = test_storage("delete-plaintext-transition");
        let note = storage.create_note(None).expect("test note");
        storage
            .connection()
            .expect("trash connection")
            .execute(
                "UPDATE notes SET trashed = 1, purge_after = ?2 WHERE id = ?1",
                params![&note.id, unix_timestamp() - 1],
            )
            .expect("mark note trashed");

        // Model an attachment producer that was authorized before delete began.
        // Permanent delete must wait for this permit instead of taking the
        // operation lock first and racing the producer's staging lifetime.
        let producer = storage
            .privacy_coordinator
            .acquire_plaintext_derivative(&note.id)
            .expect("producer permit");
        let delete_storage = storage.clone();
        let delete_note_id = note.id.clone();
        let delete_thread = thread::spawn(move || {
            delete_storage
                .permanently_delete_trashed_note(&delete_note_id)
                .expect("permanent delete");
        });
        thread::sleep(Duration::from_millis(30));

        let relative = format!(".incoming/{}", new_id("delete-race"));
        let mut staged_plaintext = storage
            .reserve_attachment_owned_plaintext(&note.id, &relative)
            .expect("reserve late staging owner");
        let staged = staged_plaintext.path().to_path_buf();
        fs::create_dir_all(staged.parent().expect("staging parent"))
            .expect("create staging parent");
        let mut staged_file = staged_plaintext.create_file().expect("create staged plaintext");
        std::io::Write::write_all(&mut staged_file, b"late producer plaintext")
            .expect("write staged plaintext");
        drop(staged_file);
        let image_stage_token = crate::native::staging::stage_image_import(
            note.id.as_str(),
            b"late browser image plaintext",
        )
        .expect("stage browser image under producer permit");

        drop(producer);
        delete_thread.join().expect("delete thread");

        assert!(!staged.exists(), "delete must scrub staging created by the drained producer");
        assert!(
            crate::native::staging::claim_staged_image_import(
                &image_stage_token,
                note.id.as_str(),
            )
            .is_err(),
            "delete must retire registered browser/native staging for the deleted note"
        );
        assert!(storage.get_note(&note.id).expect("note lookup").is_none());
        let owner_count: i64 = storage
            .connection()
            .expect("owner connection")
            .query_row(
                "SELECT COUNT(*) FROM managed_plaintext_staging WHERE note_id = ?1",
                [&note.id],
                |row| row.get(0),
            )
            .expect("owner count");
        assert_eq!(owner_count, 0, "delete must not leave a stale staging owner");

        cleanup(storage, root);
    }

}

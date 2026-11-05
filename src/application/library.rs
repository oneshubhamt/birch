use crate::domain::{
    Note, NoteCandidate, NoteId, NoteListCursor, NoteListPage, NoteListQuery, Notebook,
};
use crate::storage::LibraryStore;

use super::errors::{DataAccessError, DataResult};

/// Library-navigation workflows used by note lists, notebook navigation, and pickers.
///
/// The UI receives this capability instead of unrestricted `Storage`. It deliberately
/// exposes library browsing/catalog operations, not persistence implementation details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryService {
    storage: LibraryStore,
}

impl LibraryService {
    pub(crate) fn new(storage: LibraryStore) -> Self {
        Self { storage }
    }

    pub(crate) fn get_note(&self, note_id: &NoteId) -> DataResult<Option<Note>> {
        self.storage.get_note(note_id).map_err(DataAccessError::from)
    }

    pub(crate) fn list_notes_page(
        &self,
        query: &NoteListQuery,
        cursor: Option<&NoteListCursor>,
    ) -> DataResult<NoteListPage> {
        self.storage
            .list_notes_page(query, cursor)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn list_notes_page_with_total(
        &self,
        query: &NoteListQuery,
        cursor: Option<&NoteListCursor>,
    ) -> DataResult<NoteListPage> {
        self.storage
            .list_notes_page_with_total(query, cursor)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn reanchor_notes_page_with_total(
        &self,
        query: &NoteListQuery,
        page_index: usize,
    ) -> DataResult<(Option<NoteListCursor>, NoteListPage)> {
        self.storage
            .reanchor_notes_page_with_total(query, page_index)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn note_matches_list_query(
        &self,
        note_id: &NoteId,
        query: &NoteListQuery,
    ) -> DataResult<bool> {
        self.storage
            .note_matches_list_query(note_id, query)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn search_note_candidates(&self, query: &str) -> DataResult<Vec<NoteCandidate>> {
        self.storage
            .search_note_candidates(query)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn create_note(&self, notebook_id: Option<&str>) -> DataResult<Note> {
        self.storage
            .create_note(notebook_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn list_notebooks(&self) -> DataResult<Vec<Notebook>> {
        self.storage.list_notebooks().map_err(DataAccessError::from)
    }

    pub(crate) fn create_notebook(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> DataResult<Notebook> {
        self.storage
            .create_notebook(name, parent_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn rename_notebook(&self, notebook_id: &str, name: &str) -> DataResult<Notebook> {
        self.storage
            .rename_notebook(notebook_id, name)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn move_notebook(
        &self,
        notebook_id: &str,
        parent_id: Option<&str>,
    ) -> DataResult<Notebook> {
        self.storage
            .move_notebook(notebook_id, parent_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn delete_notebook_preserving_contents(
        &self,
        notebook_id: &str,
    ) -> DataResult<String> {
        self.storage
            .delete_notebook_preserving_contents(notebook_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn list_tags(&self) -> DataResult<Vec<String>> {
        self.storage.list_tags().map_err(DataAccessError::from)
    }

    pub(crate) fn note_list_preferences(&self) -> DataResult<(String, String)> {
        self.storage
            .note_list_preferences()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_note_list_preferences(&self, sort: &str, filter: &str) -> DataResult<()> {
        self.storage
            .set_note_list_preferences(sort, filter)
            .map_err(DataAccessError::from)
    }
}

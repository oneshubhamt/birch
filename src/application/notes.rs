use crate::domain::{
    BacklinkSummary, Citation, Note, NoteHierarchy, NoteId, NotePropertiesContext,
    NoteRevisionSummary, NoteSourceSummary, RevisionRetention, RevisionSnapshot,
    RevisionStorageUsage, Source, SourceId, SourceKind,
};
use crate::storage::{
    NoteStore, RevisionMode, RevisionSnapshotResource as StorageRevisionSnapshotResource,
};

use super::{
    errors::{DataAccessError, DataResult},
    persistence::NoteChangeScope,
};

/// Application-owned handle for a revision snapshot whose protected plaintext
/// authorization must survive through all UI-side derivation and publication.
///
/// The storage permit remains deliberately opaque above the application layer.
#[derive(Debug)]
pub(crate) struct RevisionSnapshotResource(StorageRevisionSnapshotResource);

impl RevisionSnapshotResource {
    pub(crate) fn snapshot(&self) -> &RevisionSnapshot {
        self.0.snapshot()
    }

    pub(crate) fn is_publishable(&self) -> bool {
        self.0.is_publishable()
    }
}

/// Note-level application capability for metadata, hierarchy, history, tags, and sources.
///
/// Editor document synchronization remains in `EditorService`; this service owns the
/// surrounding authoritative note mutations used by Rust-owned UI surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoteService {
    storage: NoteStore,
}

impl NoteService {
    pub(crate) fn parse_tags(raw: &str) -> Vec<String> {
        crate::storage::parse_tags(raw)
    }

    pub(crate) fn new(storage: NoteStore) -> Self {
        Self { storage }
    }

    pub(crate) fn get(&self, note_id: &NoteId) -> DataResult<Option<Note>> {
        self.storage
            .get_note(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn checkpoint(
        &self,
        note: Note,
        scope: NoteChangeScope,
    ) -> DataResult<(Note, bool)> {
        self.storage
            .save_note_owned_with_revision_status_scoped(
                note,
                RevisionMode::Checkpoint,
                scope.storage_scope(),
            )
            .map_err(|failure| DataAccessError::from(failure.error))
    }

    pub(crate) fn duplicate(&self, note_id: &NoteId) -> DataResult<Note> {
        self.storage
            .duplicate_note(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn permanently_delete_trashed(&self, note_id: &NoteId) -> DataResult<()> {
        self.storage
            .permanently_delete_trashed_note(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn trash_deletion_deadline(&self) -> DataResult<Option<i64>> {
        self.storage
            .trash_deletion_deadline()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn rename_tag(&self, current_name: &str, new_name: &str) -> DataResult<usize> {
        self.storage
            .rename_tag(current_name, new_name)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn remove_tag_from_note(&self, note_id: &NoteId, name: &str) -> DataResult<bool> {
        self.storage
            .remove_tag_from_note(note_id, name)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn delete_tag(&self, name: &str) -> DataResult<usize> {
        self.storage.delete_tag(name).map_err(DataAccessError::from)
    }

    pub(crate) fn hierarchy(&self, note_id: &NoteId) -> DataResult<NoteHierarchy> {
        self.storage
            .note_hierarchy(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn remove_parent(&self, child_note_id: &NoteId) -> DataResult<()> {
        self.storage
            .remove_note_parent(child_note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_parent(
        &self,
        child_note_id: &NoteId,
        parent_note_id: &NoteId,
    ) -> DataResult<()> {
        self.storage
            .set_note_parent(child_note_id, parent_note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn backlinks(&self, note_id: &NoteId) -> DataResult<Vec<BacklinkSummary>> {
        self.storage
            .list_backlinks(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn properties(&self, note_id: &NoteId) -> DataResult<NotePropertiesContext> {
        self.storage
            .note_properties_context(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn revision_summaries(
        &self,
        note_id: &NoteId,
    ) -> DataResult<Vec<NoteRevisionSummary>> {
        self.storage
            .list_revision_summaries(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn revision_retention(&self) -> DataResult<RevisionRetention> {
        self.storage
            .revision_retention()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn revision_storage_usage(&self) -> DataResult<RevisionStorageUsage> {
        self.storage
            .revision_storage_usage()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn revision_snapshot_resource(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> DataResult<RevisionSnapshotResource> {
        self.storage
            .revision_snapshot_resource_for_note(note_id, revision_id)
            .map(RevisionSnapshotResource)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_revision_retention(
        &self,
        max_revisions_per_note: i64,
        max_age_days: i64,
    ) -> DataResult<usize> {
        self.storage
            .set_revision_retention(max_revisions_per_note, max_age_days)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn set_revision_storage_limit(&self, max_total_bytes: i64) -> DataResult<usize> {
        self.storage
            .set_revision_storage_limit(max_total_bytes)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn list_sources(&self, note_id: &NoteId) -> DataResult<Vec<NoteSourceSummary>> {
        self.storage
            .list_note_sources(note_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn search_source_candidates(
        &self,
        note_id: &NoteId,
        query: &str,
    ) -> DataResult<Vec<Source>> {
        self.storage
            .search_source_candidates(note_id, query)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn create_source_and_attach(
        &self,
        note_id: &NoteId,
        kind: SourceKind,
        title: &str,
        reference: Option<&str>,
    ) -> DataResult<Source> {
        self.storage
            .create_source_and_attach(note_id, kind, title, reference)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn attach_source(&self, note_id: &NoteId, source_id: &SourceId) -> DataResult<()> {
        self.storage
            .attach_source(note_id, source_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn detach_source(&self, note_id: &NoteId, source_id: &SourceId) -> DataResult<()> {
        self.storage
            .detach_source(note_id, source_id)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn add_citation(
        &self,
        note_id: &NoteId,
        source_id: &SourceId,
        locator: &str,
    ) -> DataResult<Citation> {
        self.storage
            .add_citation(note_id, source_id, locator)
            .map_err(DataAccessError::from)
    }

    pub(crate) fn remove_citation(&self, note_id: &NoteId, citation_id: &str) -> DataResult<()> {
        self.storage
            .remove_citation(note_id, citation_id)
            .map_err(DataAccessError::from)
    }
}

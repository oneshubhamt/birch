//! Narrow concrete storage capabilities used by application services.
//!
//! These are zero-policy facades over the same `Storage` handle. They restrict
//! authority at the application/storage boundary without introducing repository
//! traits, extra connections, or runtime indirection.

use std::path::{Path, PathBuf};

use crate::{
    portability::{ImportPlan, ImportSummary, ImportUndoToken, PortabilityFormat, PortabilityScope},
    domain::{
        Attachment, BacklinkSummary, BackupRepositoryStatus, Citation, CompleteArchiveSummary,
        Note, NoteCandidate, NoteHierarchy, NoteId, NoteLinkTarget, NoteListCursor, NoteListPage,
        NoteListQuery, NotePropertiesContext, NoteRevisionRestoreResult, NoteRevisionSummary,
        NoteSourceSummary, Notebook, RevisionRetention, RevisionStorageUsage,
        Source, SourceId, SourceKind,
    },
    library_paths::{attachment_handoff_directory, media_compat_directory},
};

use super::{
    AttachmentIngestIntent, AuthorizedDerivedImageAttachment, DeferredMaintenanceReport,
    NoteSaveScope, OwnedNoteSaveResult,
    PendingAttachmentIngestResource, PreparedAttachmentImport, ProfiledAttachmentImport,
    RevisionMode, RevisionRestoreAuthorization, RevisionSnapshotResource, Storage, StorageError,
    StorageLocations, StorageResult,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LibraryStore {
    storage: Storage,
}

/// Narrow capability for publication authorization. It exposes no storage data;
/// callers can only capture/validate the coordinator-owned privacy snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrivacyPublicationStore {
    storage: Storage,
}

/// Narrow capability for whole-library privacy barriers. It exposes no note or
/// library data; callers can only begin a coordinator-owned read/durability
/// lease that excludes note/library privacy transitions for its lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrivacyOperationStore {
    storage: Storage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoteStore {
    storage: Storage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingsStore {
    storage: Storage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeStore {
    storage: Storage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditorStore {
    storage: Storage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentStore {
    storage: Storage,
}

#[derive(Debug)]
pub(crate) struct AttachmentFileResource {
    path: PathBuf,
    _vault_permit: Option<crate::security::VaultReadPermit>,
}

impl AttachmentFileResource {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn is_publishable(&self) -> bool {
        self._vault_permit
            .as_ref()
            .map(|permit| permit.is_publishable().unwrap_or(false))
            .unwrap_or(true)
    }
}

#[derive(Debug)]
pub(crate) struct AttachmentFileCommandResource {
    attachment: Attachment,
    file: AttachmentFileResource,
    handoff_directory: PathBuf,
    _authorization: crate::security::PlaintextOperationPermit,
}

impl AttachmentFileCommandResource {
    pub(crate) fn attachment(&self) -> &Attachment {
        &self.attachment
    }

    pub(crate) fn path(&self) -> &Path {
        self.file.path()
    }

    pub(crate) fn handoff_directory(&self) -> &Path {
        &self.handoff_directory
    }

    pub(crate) fn is_publishable(&self) -> bool {
        self.file.is_publishable()
            && self
                ._authorization
                .vault_permit()
                .map(|permit| permit.is_publishable().unwrap_or(false))
                .unwrap_or(true)
    }
}

#[derive(Debug)]
pub(crate) struct PdfExportCommandResource {
    attachment: Attachment,
    _authorization: crate::security::PlaintextOperationPermit,
}

impl PdfExportCommandResource {
    pub(crate) fn attachment(&self) -> &Attachment {
        &self.attachment
    }

    pub(crate) fn is_publishable(&self) -> bool {
        self._authorization
            .vault_permit()
            .map(|permit| permit.is_publishable().unwrap_or(false))
            .unwrap_or(true)
    }
}

#[derive(Debug)]
pub(crate) struct NoteExportResource {
    _authorization: crate::security::PlaintextOperationPermit,
}

impl NoteExportResource {
    pub(crate) fn is_publishable(&self) -> bool {
        self._authorization
            .vault_permit()
            .map(|permit| permit.is_publishable().unwrap_or(false))
            .unwrap_or(true)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportStore {
    storage: Storage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VaultStore {
    storage: Storage,
}

impl Storage {
    pub(crate) fn library_store(&self) -> LibraryStore {
        LibraryStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn privacy_publication_store(&self) -> PrivacyPublicationStore {
        PrivacyPublicationStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn privacy_operation_store(&self) -> PrivacyOperationStore {
        PrivacyOperationStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn note_store(&self) -> NoteStore {
        NoteStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn settings_store(&self) -> SettingsStore {
        SettingsStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn runtime_store(&self) -> RuntimeStore {
        RuntimeStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn editor_store(&self) -> EditorStore {
        EditorStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn attachment_store(&self) -> AttachmentStore {
        AttachmentStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn export_store(&self) -> ExportStore {
        ExportStore {
            storage: self.clone(),
        }
    }

    pub(crate) fn vault_store(&self) -> VaultStore {
        VaultStore {
            storage: self.clone(),
        }
    }
}

impl PrivacyOperationStore {
    pub(crate) fn begin_whole_library_operation(
        &self,
    ) -> StorageResult<Option<crate::security::WholeLibraryOperationLease>> {
        self.storage
            .privacy_coordinator
            .begin_whole_library_operation()
            .map_err(|message| super::StorageError::Integrity(message.to_string()))
    }
}

impl PrivacyPublicationStore {
    pub(crate) fn capture_note(
        &self,
        note_id: &crate::domain::NoteId,
    ) -> StorageResult<Option<crate::security::PrivacyPublicationAuthorization>> {
        self.storage
            .privacy_coordinator
            .capture_note_publication_authorization(note_id)
            .map_err(|message| super::StorageError::Integrity(message.to_string()))
    }

    pub(crate) fn capture_multi_note(
        &self,
    ) -> StorageResult<Option<crate::security::PrivacyPublicationAuthorization>> {
        self.storage
            .privacy_coordinator
            .capture_multi_note_publication_authorization()
            .map_err(|message| super::StorageError::Integrity(message.to_string()))
    }
}

impl LibraryStore {
    pub(crate) fn prepare_portability_import(&self, source: &Path) -> StorageResult<ImportPlan> {
        self.storage.prepare_portability_import(source)
    }

    pub(crate) fn import_portability_plan(&self, plan: &ImportPlan) -> StorageResult<ImportSummary> {
        self.storage.import_portability_plan(plan)
    }

    pub(crate) fn export_portable_note_scope(
        &self,
        note_id: &NoteId,
        format: PortabilityFormat,
        destination: &Path,
    ) -> StorageResult<PathBuf> {
        self.storage.export_portable_scope(
            &PortabilityScope::Note(note_id.clone()),
            format,
            destination,
        )
    }

    pub(crate) fn export_portable_notebook_scope(
        &self,
        notebook_id: &str,
        format: PortabilityFormat,
        destination: &Path,
    ) -> StorageResult<PathBuf> {
        self.storage
            .export_portable_notebook_scope_authorized(notebook_id, format, destination)
    }

    pub(crate) fn export_portable_library_scope(
        &self,
        format: PortabilityFormat,
        destination: &Path,
    ) -> StorageResult<PathBuf> {
        self.storage
            .export_portable_scope(&PortabilityScope::Library, format, destination)
    }

    pub(crate) fn undo_portability_import(&self, token: &ImportUndoToken) -> StorageResult<()> {
        self.storage.undo_portability_import(token)
    }

    pub(crate) fn get_note(&self, note_id: &NoteId) -> StorageResult<Option<Note>> {
        self.storage.get_note(note_id)
    }

    pub(crate) fn list_notes_page(
        &self,
        query: &NoteListQuery,
        cursor: Option<&NoteListCursor>,
    ) -> StorageResult<NoteListPage> {
        self.storage.list_notes_page(query, cursor)
    }

    pub(crate) fn list_notes_page_with_total(
        &self,
        query: &NoteListQuery,
        cursor: Option<&NoteListCursor>,
    ) -> StorageResult<NoteListPage> {
        self.storage.list_notes_page_with_total(query, cursor)
    }

    pub(crate) fn reanchor_notes_page_with_total(
        &self,
        query: &NoteListQuery,
        page_index: usize,
    ) -> StorageResult<(Option<NoteListCursor>, NoteListPage)> {
        self.storage.reanchor_notes_page_with_total(query, page_index)
    }

    pub(crate) fn note_matches_list_query(
        &self,
        note_id: &NoteId,
        query: &NoteListQuery,
    ) -> StorageResult<bool> {
        self.storage.note_matches_list_query(note_id, query)
    }

    pub(crate) fn search_note_candidates(&self, query: &str) -> StorageResult<Vec<NoteCandidate>> {
        self.storage.search_note_candidates(query)
    }

    pub(crate) fn create_note(&self, notebook_id: Option<&str>) -> StorageResult<Note> {
        self.storage.create_note(notebook_id)
    }

    pub(crate) fn list_notebooks(&self) -> StorageResult<Vec<Notebook>> {
        self.storage.list_notebooks()
    }

    pub(crate) fn create_notebook(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Notebook> {
        self.storage.create_notebook(name, parent_id)
    }

    pub(crate) fn rename_notebook(&self, notebook_id: &str, name: &str) -> StorageResult<Notebook> {
        self.storage.rename_notebook(notebook_id, name)
    }

    pub(crate) fn move_notebook(
        &self,
        notebook_id: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Notebook> {
        self.storage.move_notebook(notebook_id, parent_id)
    }

    pub(crate) fn delete_notebook_preserving_contents(
        &self,
        notebook_id: &str,
    ) -> StorageResult<String> {
        self.storage.delete_notebook_preserving_contents(notebook_id)
    }

    pub(crate) fn list_tags(&self) -> StorageResult<Vec<String>> {
        self.storage.list_tags()
    }

    pub(crate) fn note_list_preferences(&self) -> StorageResult<(String, String)> {
        self.storage.note_list_preferences()
    }

    pub(crate) fn set_note_list_preferences(&self, sort: &str, filter: &str) -> StorageResult<()> {
        self.storage.set_note_list_preferences(sort, filter)
    }
}

impl NoteStore {
    pub(crate) fn get_note(&self, note_id: &NoteId) -> StorageResult<Option<Note>> {
        self.storage.get_note(note_id)
    }

    pub(crate) fn save_note_owned_with_revision_status_scoped(
        &self,
        note: Note,
        revision_mode: RevisionMode,
        scope: NoteSaveScope,
    ) -> OwnedNoteSaveResult {
        self.storage
            .save_note_owned_with_revision_status_scoped(note, revision_mode, scope)
    }

    pub(crate) fn duplicate_note(&self, note_id: &NoteId) -> StorageResult<Note> {
        self.storage.duplicate_note(note_id)
    }

    pub(crate) fn permanently_delete_trashed_note(&self, note_id: &NoteId) -> StorageResult<()> {
        self.storage.permanently_delete_trashed_note(note_id)
    }

    pub(crate) fn trash_deletion_deadline(&self) -> StorageResult<Option<i64>> {
        self.storage.trash_deletion_deadline()
    }

    pub(crate) fn rename_tag(&self, current_name: &str, new_name: &str) -> StorageResult<usize> {
        self.storage.rename_tag(current_name, new_name)
    }

    pub(crate) fn remove_tag_from_note(&self, note_id: &NoteId, name: &str) -> StorageResult<bool> {
        self.storage.remove_tag_from_note(note_id, name)
    }

    pub(crate) fn delete_tag(&self, name: &str) -> StorageResult<usize> {
        self.storage.delete_tag(name)
    }

    pub(crate) fn note_hierarchy(&self, note_id: &NoteId) -> StorageResult<NoteHierarchy> {
        self.storage.note_hierarchy(note_id)
    }

    pub(crate) fn remove_note_parent(&self, child_note_id: &NoteId) -> StorageResult<()> {
        self.storage.remove_note_parent(child_note_id)
    }

    pub(crate) fn set_note_parent(
        &self,
        child_note_id: &NoteId,
        parent_note_id: &NoteId,
    ) -> StorageResult<()> {
        self.storage.set_note_parent(child_note_id, parent_note_id)
    }

    pub(crate) fn list_backlinks(&self, note_id: &NoteId) -> StorageResult<Vec<BacklinkSummary>> {
        self.storage.list_backlinks(note_id)
    }

    pub(crate) fn note_properties_context(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<NotePropertiesContext> {
        self.storage.note_properties_context(note_id)
    }

    pub(crate) fn list_revision_summaries(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Vec<NoteRevisionSummary>> {
        self.storage.list_revision_summaries(note_id)
    }

    pub(crate) fn revision_retention(&self) -> StorageResult<RevisionRetention> {
        self.storage.revision_retention()
    }

    pub(crate) fn revision_storage_usage(&self) -> StorageResult<RevisionStorageUsage> {
        self.storage.revision_storage_usage()
    }

    pub(crate) fn revision_snapshot_resource_for_note(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> StorageResult<RevisionSnapshotResource> {
        self.storage
            .revision_snapshot_resource_for_note(note_id, revision_id)
    }

    pub(crate) fn set_revision_retention(
        &self,
        max_revisions_per_note: i64,
        max_age_days: i64,
    ) -> StorageResult<usize> {
        self.storage
            .set_revision_retention(max_revisions_per_note, max_age_days)
    }

    pub(crate) fn set_revision_storage_limit(&self, max_total_bytes: i64) -> StorageResult<usize> {
        self.storage.set_revision_storage_limit(max_total_bytes)
    }

    pub(crate) fn list_note_sources(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Vec<NoteSourceSummary>> {
        self.storage.list_note_sources(note_id)
    }

    pub(crate) fn search_source_candidates(
        &self,
        note_id: &NoteId,
        query: &str,
    ) -> StorageResult<Vec<Source>> {
        self.storage.search_source_candidates(note_id, query)
    }

    pub(crate) fn create_source_and_attach(
        &self,
        note_id: &NoteId,
        kind: SourceKind,
        title: &str,
        reference: Option<&str>,
    ) -> StorageResult<Source> {
        self.storage
            .create_source_and_attach(note_id, kind, title, reference)
    }

    pub(crate) fn attach_source(
        &self,
        note_id: &NoteId,
        source_id: &SourceId,
    ) -> StorageResult<()> {
        self.storage.attach_source(note_id, source_id)
    }

    pub(crate) fn detach_source(
        &self,
        note_id: &NoteId,
        source_id: &SourceId,
    ) -> StorageResult<()> {
        self.storage.detach_source(note_id, source_id)
    }

    pub(crate) fn add_citation(
        &self,
        note_id: &NoteId,
        source_id: &SourceId,
        locator: &str,
    ) -> StorageResult<Citation> {
        self.storage.add_citation(note_id, source_id, locator)
    }

    pub(crate) fn remove_citation(&self, note_id: &NoteId, citation_id: &str) -> StorageResult<()> {
        self.storage.remove_citation(note_id, citation_id)
    }
}

impl SettingsStore {
    pub(crate) fn expected_data_directory() -> PathBuf {
        Storage::expected_data_directory()
    }

    pub(crate) fn configured_locations() -> StorageResult<StorageLocations> {
        Storage::configured_locations()
    }

    pub(crate) fn default_locations() -> StorageLocations {
        Storage::default_locations()
    }

    pub(crate) fn save_configured_locations(
        data_directory: PathBuf,
        backup_directory: PathBuf,
    ) -> StorageResult<StorageLocations> {
        Storage::save_configured_locations(data_directory, backup_directory)
    }

    pub(crate) fn reset_configured_locations() -> StorageResult<()> {
        Storage::reset_configured_locations()
    }

    pub(crate) fn data_directory(&self) -> &Path {
        self.storage.data_directory()
    }

    pub(crate) fn attachments_directory(&self) -> &Path {
        self.storage.attachments_directory()
    }

    pub(crate) fn automatic_backup_directory(&self) -> &Path {
        self.storage.automatic_backup_directory()
    }

    pub(crate) fn database_path(&self) -> &Path {
        self.storage.database_path()
    }

    pub(crate) fn default_archive_path(&self) -> PathBuf {
        self.storage.default_archive_path()
    }

    pub(crate) fn editor_preferences(&self) -> StorageResult<(bool, Option<u32>, Option<String>, bool)> {
        self.storage.editor_preferences()
    }

    pub(crate) fn set_editor_preferences(
        &self,
        auto_indent: bool,
        auto_first_line_indent: u32,
        default_line_spacing: Option<&str>,
        spellcheck: bool,
    ) -> StorageResult<()> {
        self.storage
            .set_editor_preferences(auto_indent, auto_first_line_indent, default_line_spacing, spellcheck)
    }

    pub(crate) fn dark_mode(&self) -> StorageResult<bool> {
        self.storage.dark_mode()
    }

    pub(crate) fn set_dark_mode(&self, enabled: bool) -> StorageResult<()> {
        self.storage.set_dark_mode(enabled)
    }

    pub(crate) fn layout_widths(&self) -> StorageResult<(i32, i32)> {
        self.storage.layout_widths()
    }

    pub(crate) fn set_layout_widths(&self, sidebar: i32, note_list: i32) -> StorageResult<()> {
        self.storage.set_layout_widths(sidebar, note_list)
    }

    pub(crate) fn right_panel_width(&self) -> StorageResult<i32> {
        self.storage.right_panel_width()
    }

    pub(crate) fn set_right_panel_width(&self, width: i32) -> StorageResult<()> {
        self.storage.set_right_panel_width(width)
    }

    pub(crate) fn schema_version(&self) -> StorageResult<i64> {
        self.storage.schema_version()
    }

    pub(crate) fn build_diagnostics(&self) -> StorageResult<(Option<String>, Option<String>)> {
        self.storage.build_diagnostics()
    }

    pub(crate) fn revision_retention(&self) -> StorageResult<RevisionRetention> {
        self.storage.revision_retention()
    }

    pub(crate) fn revision_storage_usage(&self) -> StorageResult<RevisionStorageUsage> {
        self.storage.revision_storage_usage()
    }

    pub(crate) fn set_revision_retention(
        &self,
        max_revisions_per_note: i64,
        max_age_days: i64,
    ) -> StorageResult<usize> {
        self.storage
            .set_revision_retention(max_revisions_per_note, max_age_days)
    }

    pub(crate) fn set_revision_storage_limit(&self, max_total_bytes: i64) -> StorageResult<usize> {
        self.storage.set_revision_storage_limit(max_total_bytes)
    }

    pub(crate) fn prune_revision_history(&self) -> StorageResult<usize> {
        self.storage.prune_revision_history()
    }

    pub(crate) fn trash_retention_days(&self) -> StorageResult<Option<u32>> {
        self.storage.trash_retention_days()
    }

    pub(crate) fn set_trash_retention_days(&self, days: Option<u32>) -> StorageResult<usize> {
        self.storage.set_trash_retention_days(days)
    }

    pub(crate) fn backup_repository_status(&self) -> StorageResult<BackupRepositoryStatus> {
        self.storage.backup_repository_status()
    }

    pub(crate) fn set_backup_repository_budget_bytes(
        &self,
        bytes: u64,
    ) -> StorageResult<BackupRepositoryStatus> {
        self.storage.set_backup_repository_budget_bytes(bytes)
    }

    pub(crate) fn inspect_backup_repository_snapshot(
        &self,
        snapshot_id: &str,
    ) -> StorageResult<CompleteArchiveSummary> {
        self.storage.inspect_backup_repository_snapshot(snapshot_id)
    }

    pub(crate) fn inspect_complete_archive(
        &self,
        path: &Path,
    ) -> StorageResult<CompleteArchiveSummary> {
        self.storage.inspect_complete_archive(path)
    }

    pub(crate) fn restore_complete_archive(
        &self,
        path: &Path,
    ) -> StorageResult<CompleteArchiveSummary> {
        self.storage.restore_complete_archive(path)
    }

    pub(crate) fn restore_backup_repository_snapshot(
        &self,
        snapshot_id: &str,
    ) -> StorageResult<CompleteArchiveSummary> {
        self.storage.restore_backup_repository_snapshot(snapshot_id)
    }
}

impl RuntimeStore {
    pub(crate) fn run_deferred_maintenance(&self) -> StorageResult<DeferredMaintenanceReport> {
        self.storage.run_deferred_maintenance()
    }

    pub(crate) fn reconcile_pending_attachment_ingests(&self) -> StorageResult<(usize, usize)> {
        self.storage.reconcile_pending_attachment_ingests()
    }

    pub(crate) fn purge_expired_notes(&self) -> StorageResult<usize> {
        self.storage.purge_expired_notes()
    }

    pub(crate) fn automatic_backup_due(&self) -> StorageResult<bool> {
        self.storage.automatic_backup_due()
    }

    pub(crate) fn maybe_create_automatic_backup(&self) -> StorageResult<Option<PathBuf>> {
        self.storage.maybe_create_automatic_backup()
    }

    pub(crate) fn durability_checkpoint(&self) -> StorageResult<()> {
        self.storage.durability_checkpoint()
    }
}

impl EditorStore {
    pub(crate) fn get_note(&self, note_id: &NoteId) -> StorageResult<Option<Note>> {
        self.storage.get_note(note_id)
    }

    pub(crate) fn list_note_link_targets(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Vec<NoteLinkTarget>> {
        self.storage.list_note_link_targets(note_id)
    }

    pub(crate) fn list_attachments(&self, note_id: &NoteId) -> StorageResult<Vec<Attachment>> {
        self.storage.list_attachments(note_id)
    }

    pub(crate) fn restore_note_revision(
        &self,
        revision_id: &str,
    ) -> StorageResult<NoteRevisionRestoreResult> {
        self.storage.restore_note_revision(revision_id)
    }

    pub(crate) fn restore_protected_note_revision(
        &self,
        note_id: &NoteId,
        revision_id: &str,
    ) -> StorageResult<(NoteRevisionRestoreResult, RevisionRestoreAuthorization)> {
        self.storage
            .restore_protected_revision_if_unlocked(note_id, revision_id)?
            .ok_or_else(|| StorageError::Validation("note is not encrypted".to_string()))
    }

    pub(crate) fn persist_protected_runtime_normalization(
        &self,
        note: &Note,
    ) -> StorageResult<bool> {
        self.storage
            .persist_protected_runtime_normalization_if_unlocked(note)
    }

    pub(crate) fn persist_protected_runtime_normalization_after_restore(
        &self,
        note: &Note,
        authorization: &RevisionRestoreAuthorization,
    ) -> StorageResult<()> {
        self.storage
            .persist_protected_runtime_normalization_after_restore(note, authorization)
    }

    pub(crate) fn save_note_owned_with_revision_status_scoped(
        &self,
        note: Note,
        revision_mode: RevisionMode,
        scope: NoteSaveScope,
    ) -> OwnedNoteSaveResult {
        self.storage
            .save_note_owned_with_revision_status_scoped(note, revision_mode, scope)
    }
}

impl AttachmentStore {
    pub(crate) fn list_attachments(&self, note_id: &NoteId) -> StorageResult<Vec<Attachment>> {
        self.storage.list_attachments(note_id)
    }

    pub(crate) fn list_attachments_with_authorization(
        &self,
        note_id: &NoteId,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Vec<Attachment>> {
        self.storage
            .list_attachments_with_authorization(note_id, authorization)
    }

    /// Enter the transition boundary for a plaintext attachment mutation and
    /// re-authorize the note after any protection transition that we waited for.
    pub(crate) fn plaintext_attachment_ingress_permit(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<crate::security::PlaintextOperationPermit> {
        let permit = self.storage.acquire_plaintext_operation_for_note(note_id)?;
        self.storage
            .validate_note_accepts_attachment_with_authorization(note_id, &permit)?;
        Ok(permit)
    }

    pub(crate) fn prepare_attachment_import_for_request_profiled(
        &self,
        note_id: &NoteId,
        request_id: Option<&str>,
        source: &Path,
    ) -> StorageResult<PreparedAttachmentImport> {
        self.storage
            .prepare_attachment_import_for_request_profiled(note_id, request_id, source)
    }

    pub(crate) fn finalize_prepared_attachment_import_profiled(
        &self,
        prepared: PreparedAttachmentImport,
    ) -> StorageResult<ProfiledAttachmentImport> {
        self.storage
            .finalize_prepared_attachment_import_profiled(prepared)
    }

    pub(crate) fn attachment_ingest_intents(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<Vec<AttachmentIngestIntent>> {
        self.storage.attachment_ingest_intents(note_id)
    }

    pub(crate) fn reconcile_pending_attachment_ingests_for_note(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<(usize, usize)> {
        self.storage
            .reconcile_pending_attachment_ingests_for_note(note_id)
    }

    pub(crate) fn pending_attachment_ingest_resource(
        &self,
        ingest_id: &str,
    ) -> StorageResult<Option<PendingAttachmentIngestResource>> {
        self.storage.pending_attachment_ingest_resource(ingest_id)
    }

    pub(crate) fn pending_attachment_ingest_resource_with_authorization(
        &self,
        ingest_id: &str,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Option<PendingAttachmentIngestResource>> {
        self.storage
            .pending_attachment_ingest_resource_with_authorization(ingest_id, authorization)
    }


    pub(crate) fn attachment_file_command_resource(
        &self,
        note_id: &NoteId,
        attachment_id: &str,
    ) -> StorageResult<AttachmentFileCommandResource> {
        // Compound authorization is linearized once in PrivacyCoordinator. A
        // Lock/Protect can therefore occur only before or after both the
        // derivative lifetime and any Vault reader are captured.
        let authorization = self.storage.acquire_plaintext_operation_for_note(note_id)?;
        let connection = self.storage.connection()?;
        let protected = super::protection::protected_note_exists(&connection, note_id)?;
        drop(connection);

        if protected {
            let access = authorization.access().ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note before opening its attachments".to_string(),
                )
            })?;
            let resource = self
                .storage
                .protected_attachment_with_access(attachment_id, access)?
                .ok_or_else(|| StorageError::Validation("attachment no longer exists".to_string()))?;
            if resource.note_id != *note_id {
                return Err(StorageError::Validation(
                    "attachment belongs to a different note".to_string(),
                ));
            }
            return Ok(AttachmentFileCommandResource {
                attachment: resource.attachment,
                file: AttachmentFileResource {
                    path: resource.path,
                    _vault_permit: None,
                },
                handoff_directory: attachment_handoff_directory(self.storage.data_directory()),
                _authorization: authorization,
            });
        }

        let attachment = self
            .storage
            .list_attachments(note_id)?
            .into_iter()
            .find(|attachment| attachment.id == attachment_id)
            .ok_or_else(|| StorageError::Validation("attachment no longer exists".to_string()))?;
        let path = self.storage.attachment_file_path(&attachment)?;
        Ok(AttachmentFileCommandResource {
            attachment,
            file: AttachmentFileResource {
                path,
                _vault_permit: None,
            },
            handoff_directory: attachment_handoff_directory(self.storage.data_directory()),
            _authorization: authorization,
        })
    }

    pub(crate) fn attachment_file_resource(
        &self,
        attachment: &Attachment,
    ) -> StorageResult<AttachmentFileResource> {
        if let Some(resource) = self.storage.protected_attachment_resource(&attachment.id)? {
            return Ok(AttachmentFileResource {
                path: resource.path,
                _vault_permit: Some(resource.permit),
            });
        }
        Ok(AttachmentFileResource {
            path: self.storage.attachment_file_path(attachment)?,
            _vault_permit: None,
        })
    }

    pub(crate) fn add_attachment_file_as_authorized(
        &self,
        note_id: &NoteId,
        file_name: &str,
        mime_type: Option<&str>,
        source: &Path,
        authorization: &crate::security::PlaintextOperationPermit,
    ) -> StorageResult<Attachment> {
        self.storage.add_attachment_file_as_authorized(
            note_id, file_name, mime_type, source, authorization,
        )
    }

    pub(crate) fn add_derived_image_attachment_file_authorized(
        &self,
        request: AuthorizedDerivedImageAttachment<'_>,
    ) -> StorageResult<Attachment> {
        self.storage
            .add_derived_image_attachment_file_authorized(request)
    }

    pub(crate) fn transport_store(&self) -> super::AttachmentTransportStore {
        self.storage.transport_store()
    }

    pub(crate) fn media_compat_directory(&self) -> PathBuf {
        media_compat_directory(self.storage.data_directory())
    }

    pub(crate) fn attachment_handoff_directory(&self) -> PathBuf {
        attachment_handoff_directory(self.storage.data_directory())
    }
}

impl ExportStore {
    pub(crate) fn export_complete_archive(&self, destination: &Path) -> StorageResult<()> {
        self.storage.export_complete_archive(destination)
    }

    /// Retain authorization for the complete native export/print lifetime.
    ///
    /// The derivative permit is acquired before protection-state inspection, so
    /// Protect/Unprotect/Delete cannot cross the interaction. If the note is
    /// protected, the exact unlocked Vault-session permit is retained as well,
    /// which makes Lock wait until the native chooser/print lifecycle ends.
    pub(crate) fn note_export_resource(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<NoteExportResource> {
        let authorization = self.storage.acquire_plaintext_operation_for_note(note_id)?;
        let connection = self.storage.connection()?;
        let protected = super::protection::protected_note_exists(&connection, note_id)?;
        drop(connection);
        if protected && authorization.access().is_none() {
            return Err(StorageError::Validation(
                "unlock this encrypted note before exporting it".to_string(),
            ));
        }
        Ok(NoteExportResource {
            _authorization: authorization,
        })
    }

    pub(crate) fn pdf_export_command_resource(
        &self,
        note_id: &NoteId,
        attachment_id: &str,
    ) -> StorageResult<PdfExportCommandResource> {
        let authorization = self.storage.acquire_plaintext_operation_for_note(note_id)?;
        let connection = self.storage.connection()?;
        let protected = super::protection::protected_note_exists(&connection, note_id)?;
        drop(connection);

        let attachment = if protected {
            let access = authorization.access().ok_or_else(|| {
                StorageError::Validation(
                    "unlock this encrypted note before using its attachment metadata".to_string(),
                )
            })?;
            self.storage
                .protected_attachment_metadata_with_access(note_id, attachment_id, access)?
                .ok_or_else(|| StorageError::Validation("attachment no longer exists".to_string()))?
        } else {
            self.storage
                .list_attachments(note_id)?
                .into_iter()
                .find(|attachment| attachment.id == attachment_id)
                .ok_or_else(|| StorageError::Validation("attachment no longer exists".to_string()))?
        };

        Ok(PdfExportCommandResource {
            attachment,
            _authorization: authorization,
        })
    }

}

impl VaultStore {
    pub(crate) fn requires_autosave_transition_barrier(
        &self,
        note_id: &NoteId,
    ) -> StorageResult<bool> {
        self.storage.note_has_pending_vault_operation(note_id)
    }

    pub(crate) fn protect_note(
        &self,
        note_id: &NoteId,
        password: &[u8],
        public_label: &str,
    ) -> StorageResult<Note> {
        self.storage
            .protect_note_with_password_and_public_label(note_id, password, public_label)
    }

    pub(crate) fn pending_backup_privacy_reset_count(&self) -> StorageResult<usize> {
        self.storage.pending_backup_privacy_reset_count()
    }

    pub(crate) fn unlock_note(&self, note_id: &NoteId, password: &[u8]) -> StorageResult<Note> {
        self.storage.unlock_protected_note(note_id, password)
    }

    pub(crate) fn lock_note(&self, note_id: &NoteId) -> StorageResult<Note> {
        self.storage.lock_protected_note(note_id)
    }

    pub(crate) fn remove_note_protection(
        &self,
        note_id: &NoteId,
        password: &[u8],
    ) -> StorageResult<Note> {
        self.storage.remove_note_protection_with_password(note_id, password)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::new_id;
    use std::{fs, path::PathBuf};

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

    #[test]
    fn protected_tag_note_store_commands_use_encrypted_metadata() {
        let (storage, root) = test_storage("protected-tag-note-store-command");
        let mut note = storage.create_note(None).expect("create note");
        note.tags = vec!["secret".to_string(), "shared".to_string()];
        let note = storage
            .save_note_owned_with_revision_status_scoped(
                note,
                RevisionMode::Checkpoint,
                NoteSaveScope::TAGS,
            )
            .expect("save tags")
            .0;
        storage
            .protect_note_with_password(&note.id, b"correct horse battery staple")
            .expect("protect note");
        let notes = storage.note_store();

        assert!(
            notes
                .remove_tag_from_note(&note.id, "secret")
                .expect("remove protected tag")
        );
        assert_eq!(
            notes.get_note(&note.id).expect("lookup").expect("note").tags,
            vec!["shared".to_string()]
        );

        assert_eq!(notes.rename_tag("shared", "renamed").expect("rename"), 1);
        assert_eq!(
            notes.get_note(&note.id).expect("lookup").expect("note").tags,
            vec!["renamed".to_string()]
        );

        assert_eq!(notes.delete_tag("renamed").expect("delete"), 1);
        assert!(
            notes
                .get_note(&note.id)
                .expect("lookup")
                .expect("note")
                .tags
                .is_empty()
        );

        cleanup(storage, root);
    }
}


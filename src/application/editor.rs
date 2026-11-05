use std::sync::Arc;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::autosave::{AutosaveController, AutosaveEvent, TargetFlushTicket};
use crate::editor_projection::derive_editor_projections;
use crate::application::errors::EditorError;
use crate::domain::{Attachment, Note, NoteId, NoteLinkTarget};
use crate::storage::{
    EditorStore, NoteSaveScope, RevisionMode,
    RevisionRestoreAuthorization as StorageRevisionRestoreAuthorization,
};

use super::persistence::NoteChangeScope;

/// Application-level editor workflows that do not depend on Dioxus or the browser bridge.
#[derive(Clone, PartialEq)]
pub(crate) struct EditorService {
    storage: EditorStore,
    autosave: AutosaveController,
}

#[derive(Debug)]
pub(crate) struct EditorFlushTicket(TargetFlushTicket);

#[derive(Debug)]
pub(crate) struct EditorFlushResult {
    pub(crate) pending_warning: Option<String>,
}

impl EditorFlushTicket {
    pub(crate) fn wait(self) -> Result<EditorFlushResult, EditorError> {
        let (_, pending_warning) = self
            .0
            .wait()
            .map_err(EditorError::Autosave)?
            .into_parts();
        Ok(EditorFlushResult { pending_warning })
    }
}

pub(crate) type EditorPersistenceEvent = AutosaveEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorPersistenceMode {
    Persist,
    Checkpoint,
    CheckpointSilent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectedEditorDocument {
    pub(crate) editor_state: String,
    pub(crate) markdown: String,
    pub(crate) plain_text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct RevisionRestoreAuthorization(StorageRevisionRestoreAuthorization);

#[derive(Debug)]
pub(crate) struct RevisionRestorePersistence {
    pub(crate) note: Note,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) note_link_targets: Vec<NoteLinkTarget>,
    pub(crate) warnings: Vec<String>,
    pub(crate) authorization: Option<RevisionRestoreAuthorization>,
}



impl EditorService {
    pub(crate) fn new(storage: EditorStore, autosave: AutosaveController) -> Self {
        Self { storage, autosave }
    }

    pub(crate) fn load_note(&self, note_id: &NoteId) -> Result<Option<Note>, EditorError> {
        self.storage.get_note(note_id).map_err(EditorError::from)
    }

    pub(crate) fn note_link_targets(&self, note_id: &NoteId) -> Result<Vec<NoteLinkTarget>, EditorError> {
        self.storage
            .list_note_link_targets(note_id)
            .map_err(EditorError::from)
    }

    /// Merge an optional live editor snapshot into the latest Rust draft and cross the
    /// requested durability boundary. Browser/session verification stays in the UI adapter.
    pub(crate) fn synchronize_document(
        &self,
        selected: Note,
        editor_state: Option<String>,
        mode: EditorPersistenceMode,
    ) -> Result<Arc<Note>, EditorError> {
        self.flush_pending(mode)?;
        let note_id = selected.id.clone();
        if selected.protection.is_locked() {
            // A locked note has no live plaintext editor to checkpoint. Treat the
            // durability boundary as already satisfied and return canonical public
            // state instead of asking the autosave worker to persist a locked stub.
            return Ok(Arc::new(self.storage.get_note(&note_id)?.unwrap_or(selected)));
        }
        let mut current = self
            .storage
            .get_note(&note_id)?
            .unwrap_or(selected);
        let document_changed = match editor_state {
            Some(editor_state) if current.editor_state() == editor_state => false,
            Some(editor_state) => {
                let projected = project_editor_document(editor_state)?;
                apply_projected_document(&mut current, projected)
            }
            None => false,
        };
        if document_changed {
            current.updated_at = crate::storage::unix_timestamp();
        }
        let scope = if document_changed {
            NoteSaveScope::DOCUMENT
        } else {
            NoteSaveScope::NONE
        };
        self.flush_note(current, mode, scope)
    }

    pub(crate) fn flush_pending(
        &self,
        mode: EditorPersistenceMode,
    ) -> Result<Option<Arc<Note>>, EditorError> {
        match mode {
            EditorPersistenceMode::Persist => self.autosave.flush_pending_live(),
            EditorPersistenceMode::Checkpoint => self.autosave.flush_pending(),
            EditorPersistenceMode::CheckpointSilent => self.autosave.flush_pending_silent(),
        }
        .map_err(EditorError::Autosave)
    }

    pub(crate) fn checkpoint_note_silent(&self, note_id: &NoteId) -> Result<(), EditorError> {
        if self
            .storage
            .get_note(note_id)?
            .is_some_and(|note| note.protection.is_locked())
        {
            // Locked notes expose only their public stub and have no editable
            // plaintext draft to checkpoint. Treat this durability request as
            // already satisfied instead of sending an inadmissible autosave.
            return Ok(());
        }
        self.autosave
            .checkpoint_note_silent(note_id)
            .map(|_| ())
            .map_err(EditorError::Autosave)
    }

    pub(crate) fn checkpoint_all(&self) -> Result<(), EditorError> {
        self.autosave.checkpoint_all().map_err(EditorError::Autosave)
    }

    pub(crate) fn schedule_note(
        &self,
        mut note: Note,
        scope: NoteChangeScope,
    ) -> Result<(), EditorError> {
        note.updated_at = crate::storage::unix_timestamp();
        self.autosave
            .schedule_scoped(note, scope.storage_scope())
            .map_err(EditorError::Autosave)
    }

    pub(crate) fn begin_note_flush(
        &self,
        mut note: Note,
        mode: EditorPersistenceMode,
        scope: NoteChangeScope,
    ) -> Result<EditorFlushTicket, EditorError> {
        note.updated_at = crate::storage::unix_timestamp();
        let (checkpoint, emit_saved_event) = match mode {
            EditorPersistenceMode::Persist => (false, true),
            EditorPersistenceMode::Checkpoint => (true, true),
            EditorPersistenceMode::CheckpointSilent => (true, false),
        };
        self.autosave
            .begin_note_target_flush_scoped(
                note,
                scope.storage_scope(),
                checkpoint,
                emit_saved_event,
            )
            .map(EditorFlushTicket)
            .map_err(EditorError::Autosave)
    }

    pub(crate) fn take_persistence_event_receiver(
        &self,
    ) -> Result<UnboundedReceiver<EditorPersistenceEvent>, EditorError> {
        self.autosave
            .take_event_receiver()
            .map_err(EditorError::Autosave)
    }

    pub(crate) fn is_current_note_version(&self, note_id: &NoteId, version: u64) -> bool {
        self.autosave.is_current_note_version(note_id, version)
    }

    pub(crate) fn reset_library(&self) -> Result<(), EditorError> {
        self.autosave.reset_library().map_err(EditorError::Autosave)
    }

    pub(crate) fn discard_pending_note(&self, note_id: &NoteId) -> Result<(), EditorError> {
        self.autosave
            .discard_note(note_id)
            .map_err(EditorError::Autosave)
    }

    fn flush_note(
        &self,
        note: Note,
        mode: EditorPersistenceMode,
        scope: NoteSaveScope,
    ) -> Result<Arc<Note>, EditorError> {
        match mode {
            EditorPersistenceMode::Persist => self.autosave.flush_live_note_scoped(note, scope),
            EditorPersistenceMode::Checkpoint => self.autosave.flush_note_scoped(note, scope),
            EditorPersistenceMode::CheckpointSilent => {
                self.autosave.flush_note_silent_scoped(note, scope)
            }
        }
        .map_err(EditorError::Autosave)
    }

    /// Commit the pre-validated revision restore and refresh the editor-owned registries.
    /// Browser replacement/remount remains a UI/bridge responsibility.
    pub(crate) fn restore_revision(
        &self,
        current: Note,
        revision_id: &str,
    ) -> Result<RevisionRestorePersistence, EditorError> {
        let note_id = current.id.clone();
        let protected_restore = current.protection.is_unlocked();

        let (mut restored, authorization) = if protected_restore {
            // Protected notes are intentionally read-only through the ordinary
            // autosave path. Revision restore is a specialized authenticated
            // Vault mutation and must never flush the decrypted note into SQLite.
            let (restored, authorization) = self
                .storage
                .restore_protected_note_revision(&note_id, revision_id)
                .map_err(EditorError::Storage)?;
            (restored, Some(RevisionRestoreAuthorization(authorization)))
        } else {
            self.autosave
                .flush_note_silent_scoped(current, NoteSaveScope::DOCUMENT)
                .map_err(|error| {
                    EditorError::Autosave(
                        error.context("could not save the current note before restoring")
                    )
                })?;
            self.autosave
                .discard_note(&note_id)
                .map_err(EditorError::Autosave)?;
            (
                self.storage
                    .restore_note_revision(revision_id)
                    .map_err(EditorError::Storage)?,
                None,
            )
        };

        if !protected_restore
            && let Err(error) = self.autosave.discard_note(&note_id)
        {
            restored.warnings.push(format!(
                "The restored version was committed, but stale autosave cleanup will be retried: {error}"
            ));
        }
        let attachments = match self.storage.list_attachments(&note_id) {
            Ok(attachments) => attachments,
            Err(error) => {
                restored.warnings.push(format!(
                    "The restored version was committed, but attachments could not refresh: {error}"
                ));
                Vec::new()
            }
        };
        let note_link_targets = match self.storage.list_note_link_targets(&note_id) {
            Ok(targets) => targets,
            Err(error) => {
                restored.warnings.push(format!(
                    "The restored version was committed, but note-link status could not refresh: {error}"
                ));
                Vec::new()
            }
        };

        Ok(RevisionRestorePersistence {
            note: restored.note,
            attachments,
            note_link_targets,
            warnings: restored.warnings,
            authorization,
        })
    }

    /// Persist a browser-runtime-normalized document after a revision restore.
    /// A committed restore remains successful even when this maintenance step warns.
    pub(crate) fn persist_runtime_normalization(&self, note: Note) -> Vec<String> {
        let mut warnings = Vec::new();
        let note_id = note.id.clone();
        let protected_normalization = note.protection.is_unlocked();
        let save_result = if protected_normalization {
            self.storage
                .persist_protected_runtime_normalization(&note)
                .map(|_| ())
        } else {
            self.storage
                .save_note_owned_with_revision_status_scoped(
                    note,
                    RevisionMode::Autosave,
                    NoteSaveScope::DOCUMENT,
                )
                .map(|_| ())
                .map_err(|failure| failure.error)
        };
        if let Err(error) = save_result {
            warnings.push(format!(
                "The restored version was committed, but the runtime-normalized state could not be persisted: {error}"
            ));
        }
        if !protected_normalization
            && let Err(error) = self.autosave.discard_note(&note_id)
        {
            warnings.push(format!(
                "The restored version was committed, but normalized autosave cleanup will be retried: {error}"
            ));
        }
        warnings
    }

    /// Persist runtime normalization while retaining the exact protected
    /// revision-restore authorization that already spans the post-commit UI
    /// lifetime. This avoids reacquiring Vault access behind a concurrent Lock.
    pub(crate) fn persist_runtime_normalization_after_restore(
        &self,
        note: Note,
        authorization: RevisionRestoreAuthorization,
    ) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Err(error) = self
            .storage
            .persist_protected_runtime_normalization_after_restore(&note, &authorization.0)
        {
            warnings.push(format!(
                "The restored version was committed, but the runtime-normalized state could not be persisted: {error}"
            ));
        }
        warnings
    }
}

pub(crate) fn project_editor_document(
    editor_state: String,
) -> Result<ProjectedEditorDocument, EditorError> {
    let projections = derive_editor_projections(&editor_state)
        .map_err(EditorError::InvalidDocument)?;
    Ok(ProjectedEditorDocument {
        editor_state,
        markdown: projections.markdown,
        plain_text: projections.plain_text,
    })
}

pub(crate) fn apply_projected_document(
    note: &mut Note,
    change: ProjectedEditorDocument,
) -> bool {
    if note.editor_state() == change.editor_state
        && note.body() == change.markdown
        && note.plain_text() == change.plain_text
    {
        return false;
    }
    note.replace_document(change.markdown, change.editor_state, change.plain_text);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_document_round_trip_preserves_canonical_state() {
        let state = include_str!("../../assets/blank-editor-state.json").trim().to_string();
        let projected = project_editor_document(state.clone()).expect("project blank document");
        assert_eq!(projected.editor_state, state);
        assert!(!projected.markdown.contains("undefined"));
    }

    #[test]
    fn note_switch_checkpoints_current_note_before_loading_target() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-a14-note-switch-{}-{}",
            std::process::id(),
            crate::storage::new_id("switch")
        ));
        let storage = crate::storage::Storage::at_with_backup(
            root.join("data"),
            root.join("backups"),
        )
        .expect("test storage should initialize");
        let mut current = storage.create_note(None).expect("create current note");
        let target = storage.create_note(None).expect("create target note");
        current.title = "Checkpointed before switch".to_string();

        let autosave = AutosaveController::try_new(storage.clone()).expect("start autosave");
        autosave.schedule(current.clone()).expect("schedule current note");
        let service = EditorService::new(storage.editor_store(), autosave.clone());
        let checkpointed = service
            .synchronize_document(
                current.clone(),
                None,
                EditorPersistenceMode::Checkpoint,
            )
            .expect("checkpoint current note");
        let loaded = service
            .load_note(&target.id)
            .expect("target load should not fail")
            .expect("target note should exist");

        assert_eq!(checkpointed.title, "Checkpointed before switch");
        assert_eq!(loaded.id, target.id);
        assert_eq!(
            storage
                .get_note(&current.id)
                .expect("read current note")
                .expect("current note should exist")
                .title,
            "Checkpointed before switch"
        );

        drop(service);
        drop(autosave);
        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn protected_note_switching_survives_encrypt_and_lock_state_boundaries() {
        let root = std::env::temp_dir().join(format!(
            "rustnote-vault-switch-{}-{}",
            std::process::id(),
            crate::storage::new_id("vault-switch")
        ));
        let storage = crate::storage::Storage::at_with_backup(
            root.join("data"),
            root.join("backups"),
        )
        .expect("test storage should initialize");
        let mut current = storage.create_note(None).expect("create protected note");
        let target = storage.create_note(None).expect("create target note");
        current.title = "Private finance title".to_string();
        current.updated_at = crate::storage::unix_timestamp();
        current = storage
            .save_note_checkpoint_owned(current)
            .expect("save current note");

        let autosave = AutosaveController::try_new(storage.clone()).expect("start autosave");
        let service = EditorService::new(storage.editor_store(), autosave.clone());
        let vault = crate::application::vault::VaultProtectionService::new(
            storage.vault_store(),
            autosave.clone(),
        );

        vault
            .protect_note(
                &current.id,
                b"correct horse battery staple",
                "Finance vault",
            )
            .expect("protect note");
        let protected = storage
            .get_note(&current.id)
            .expect("read protected note")
            .expect("protected note exists");
        assert!(protected.protection.is_unlocked());
        service
            .synchronize_document(protected, None, EditorPersistenceMode::Checkpoint)
            .expect("unlocked protected note checkpoint");
        service
            .load_note(&target.id)
            .expect("target load")
            .expect("target exists");

        vault.lock_note(&current.id).expect("lock protected note");
        let locked = storage
            .get_note(&current.id)
            .expect("read locked note")
            .expect("locked note exists");
        assert!(locked.protection.is_locked());
        assert_eq!(locked.display_title(), "Finance vault");
        service
            .checkpoint_note_silent(&current.id)
            .expect("locked target checkpoint must be an intentional no-op");
        service
            .synchronize_document(locked, None, EditorPersistenceMode::Checkpoint)
            .expect("locked note has no plaintext editor checkpoint to block switching");
        let loaded = service
            .load_note(&target.id)
            .expect("target load")
            .expect("target exists");
        assert_eq!(loaded.id, target.id);

        drop(vault);
        drop(service);
        drop(autosave);
        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

}

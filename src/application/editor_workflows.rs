//! Cross-service editor workflows.
//!
//! This coordinator owns blocking/exclusive execution choices and durable sequencing for
//! editor-triggered attachment, media, export, and revision operations. It deliberately
//! contains no Dioxus/browser transport; the UI adapter only presents outcomes and
//! dispatches the typed editor bridge.

use std::path::PathBuf;

use crate::application::attachments::{
    AttachmentBatchResult, AttachmentRecoveryResult, AttachmentService, ImageEditRequest,
    ImageEditResult, ImageImportRequest, ImageImportResult, PreparedAttachmentBatchResult,
    PreparedAttachmentImport,
};
use crate::application::editor::{
    EditorService, RevisionRestoreAuthorization, RevisionRestorePersistence,
};
use crate::application::errors::{AttachmentError, EditorWorkflowError};
use crate::application::executor::{
    ApplicationExecutor, ApplicationOperation, ApplicationReadTicket, EditingPauseGuard,
};
use crate::application::export::{ExportService, PdfExportOutcome};
use crate::domain::{Attachment, Note, NoteId};

#[derive(Clone)]
pub(crate) struct EditorWorkflowService {
    editor: EditorService,
    attachments: AttachmentService,
    export: ExportService,
    executor: ApplicationExecutor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaCompatibilityWorkflowAction {
    Probe,
    Prepare,
    Invalidate,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MediaCompatibilityWorkflowOutcome {
    pub(crate) status: String,
    pub(crate) message: String,
}

pub(crate) struct AttachmentImportFinalization {
    stage_failure: Option<String>,
    ticket: ApplicationReadTicket<AttachmentBatchResult, AttachmentError>,
}

#[derive(Debug)]
pub(crate) struct AttachmentImportCompletion {
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) ready: Vec<Attachment>,
    pub(crate) failure: Option<String>,
    pub(crate) request_found: bool,
    pub(crate) all_ready: bool,
    pub(crate) embedding_lookup_error: Option<AttachmentError>,
}

impl EditorWorkflowService {
    pub(crate) fn new(
        editor: EditorService,
        attachments: AttachmentService,
        export: ExportService,
        executor: ApplicationExecutor,
    ) -> Self {
        Self {
            editor,
            attachments,
            export,
            executor,
        }
    }

    pub(crate) fn attachment_operation_active(&self) -> bool {
        let status = self.executor.snapshot();
        status.current == Some(ApplicationOperation::AddAttachments)
            || status.queued == Some(ApplicationOperation::AddAttachments)
    }

    pub(crate) fn pause_editing(&self) -> EditingPauseGuard {
        self.executor.pause_editing()
    }

    pub(crate) async fn save_attachment_insertion_anchor(
        &self,
        note: Note,
    ) -> Result<(), EditorWorkflowError> {
        let service = self.attachments.clone();
        ApplicationExecutor::read("attachment insertion anchor save", move || {
            service.save_insertion_anchor(note)
        })
        .await
        .map_err(EditorWorkflowError::from)
    }

    pub(crate) async fn stage_attachment_import(
        &self,
        current: Note,
        request_id: String,
        paths: Vec<PathBuf>,
    ) -> Result<PreparedAttachmentBatchResult, EditorWorkflowError> {
        let service = self.attachments.clone();
        self.executor
            .execute(ApplicationOperation::AddAttachments, move || {
                service.prepare_import_batch(current, request_id, paths)
            })
            .await
            .map_err(EditorWorkflowError::from)
    }

    /// Start finalization before the caller performs another browser await. Once the
    /// managed stage exists, finalization must continue independently of the editor
    /// surface so a note switch/unmount cannot strand durable ingest state.
    pub(crate) fn start_attachment_finalization(
        &self,
        stage_failure: Option<String>,
        prepared: Vec<PreparedAttachmentImport>,
    ) -> AttachmentImportFinalization {
        let service = self.attachments.clone();
        let ticket = ApplicationExecutor::start_read("attachment finalization", move || {
            service.finalize_import_batch(prepared)
        });
        AttachmentImportFinalization {
            stage_failure,
            ticket,
        }
    }

    pub(crate) async fn complete_attachment_finalization(
        &self,
        finalization: AttachmentImportFinalization,
    ) -> Result<AttachmentImportCompletion, EditorWorkflowError> {
        let AttachmentImportFinalization {
            stage_failure,
            ticket,
        } = finalization;
        let result = ticket.wait().await.map_err(EditorWorkflowError::from)?;
        let failure = result.failure.or(stage_failure);
        let refreshed = result.attachments;
        let request_attachment_ids = Ok(result.request_attachment_ids);

        Ok(resolve_attachment_import_completion(
            refreshed,
            failure,
            request_attachment_ids,
        ))
    }

    pub(crate) async fn media_compatibility(
        &self,
        attachment_id: String,
        profile: String,
        action: MediaCompatibilityWorkflowAction,
    ) -> MediaCompatibilityWorkflowOutcome {
        let service = self.attachments.clone();
        let label = match action {
            MediaCompatibilityWorkflowAction::Probe => "media compatibility probe",
            MediaCompatibilityWorkflowAction::Prepare => "media compatibility preparation",
            MediaCompatibilityWorkflowAction::Invalidate => "media compatibility invalidation",
        };
        let result = ApplicationExecutor::read(label, move || match action {
            MediaCompatibilityWorkflowAction::Probe => {
                service.probe_media_compatibility(&attachment_id, &profile)
            }
            MediaCompatibilityWorkflowAction::Prepare => service
                .prepare_media_compatibility(&attachment_id, &profile)
                .map(|_| "ready".to_string()),
            MediaCompatibilityWorkflowAction::Invalidate => service
                .invalidate_media_compatibility(&attachment_id, &profile)
                .map(|_| "invalidated".to_string()),
        })
        .await;

        match result {
            Ok(status) => MediaCompatibilityWorkflowOutcome {
                status,
                message: String::new(),
            },
            Err(error) => MediaCompatibilityWorkflowOutcome {
                status: match action {
                    MediaCompatibilityWorkflowAction::Prepare => "failed",
                    MediaCompatibilityWorkflowAction::Probe
                    | MediaCompatibilityWorkflowAction::Invalidate => "error",
                }
                .to_string(),
                message: error.to_string(),
            },
        }
    }

    pub(crate) async fn open_pending_attachment(
        &self,
        ingest_id: String,
    ) -> Result<(), EditorWorkflowError> {
        let service = self.attachments.clone();
        ApplicationExecutor::read("pending attachment open", move || {
            service.open_pending(&ingest_id)
        })
        .await
        .map_err(EditorWorkflowError::from)
    }

    pub(crate) async fn recover_pending_attachments(
        &self,
        note_id: NoteId,
    ) -> Result<AttachmentRecoveryResult, EditorWorkflowError> {
        let service = self.attachments.clone();
        ApplicationExecutor::read("attachment recovery", move || {
            service.recover_pending(&note_id)
        })
        .await
        .map_err(EditorWorkflowError::from)
    }

    pub(crate) async fn save_pdf_export(
        &self,
        note_id: NoteId,
        attachment_id: String,
        export_token: String,
    ) -> Result<PdfExportOutcome, EditorWorkflowError> {
        self.export
            .save_staged_pdf_export(note_id, attachment_id, export_token)
            .await
            .map_err(EditorWorkflowError::from)
    }

    pub(crate) fn discard_image_edit_stage(&self, stage_token: &str) {
        AttachmentService::discard_image_edit_stage(stage_token);
    }

    pub(crate) fn discard_image_import_stages<'a>(
        &self,
        stage_tokens: impl IntoIterator<Item = &'a str>,
    ) {
        for stage_token in stage_tokens {
            AttachmentService::discard_image_import_stage(stage_token);
        }
    }

    pub(crate) async fn commit_image_edit(
        &self,
        note_id: NoteId,
        request: ImageEditRequest,
    ) -> Result<ImageEditResult, EditorWorkflowError> {
        let service = self.attachments.clone();
        self.executor
            .execute(ApplicationOperation::AddAttachments, move || {
                service.commit_image_edit(note_id, request)
            })
            .await
            .map_err(EditorWorkflowError::from)
    }

    pub(crate) async fn import_images(
        &self,
        note_id: NoteId,
        images: Vec<ImageImportRequest>,
    ) -> Result<ImageImportResult, EditorWorkflowError> {
        let service = self.attachments.clone();
        self.executor
            .execute(ApplicationOperation::AddAttachments, move || {
                service.import_images(note_id, images)
            })
            .await
            .map_err(EditorWorkflowError::from)
    }

    pub(crate) async fn restore_revision(
        &self,
        current: Note,
        revision_id: String,
    ) -> Result<RevisionRestorePersistence, EditorWorkflowError> {
        let service = self.editor.clone();
        self.executor
            .execute(ApplicationOperation::RestoreNoteRevision, move || {
                service.restore_revision(current, &revision_id)
            })
            .await
            .map_err(EditorWorkflowError::from)
    }

    pub(crate) async fn persist_revision_normalization(&self, note: Note) -> Vec<String> {
        let service = self.editor.clone();
        match self
            .executor
            .execute(ApplicationOperation::RestoreNoteRevision, move || {
                Ok::<_, EditorWorkflowError>(service.persist_runtime_normalization(note))
            })
            .await
        {
            Ok(warnings) => warnings,
            Err(error) => vec![format!(
                "The restored version was committed, but normalization maintenance could not \
                 finish: {error}"
            )],
        }
    }

    pub(crate) async fn persist_revision_normalization_after_protected_restore(
        &self,
        note: Note,
        authorization: RevisionRestoreAuthorization,
    ) -> Vec<String> {
        let service = self.editor.clone();
        match self
            .executor
            .execute(ApplicationOperation::RestoreNoteRevision, move || {
                Ok::<_, EditorWorkflowError>(
                    service.persist_runtime_normalization_after_restore(note, authorization),
                )
            })
            .await
        {
            Ok(warnings) => warnings,
            Err(error) => vec![format!(
                "The restored version was committed, but normalization maintenance could not \
                 finish: {error}"
            )],
        }
    }
}

fn resolve_attachment_import_completion(
    refreshed: Vec<Attachment>,
    failure: Option<String>,
    request_attachment_ids: Result<Vec<String>, AttachmentError>,
) -> AttachmentImportCompletion {
    let request_attachment_ids = match request_attachment_ids {
        Ok(ids) => ids,
        Err(error) => {
            return AttachmentImportCompletion {
                attachments: refreshed,
                ready: Vec::new(),
                failure,
                request_found: false,
                all_ready: false,
                embedding_lookup_error: Some(error),
            };
        }
    };

    if request_attachment_ids.is_empty() {
        return AttachmentImportCompletion {
            attachments: refreshed,
            ready: Vec::new(),
            failure,
            request_found: false,
            all_ready: false,
            embedding_lookup_error: None,
        };
    }

    let ready = request_attachment_ids
        .iter()
        .filter_map(|attachment_id| {
            refreshed
                .iter()
                .find(|attachment| &attachment.id == attachment_id)
                .cloned()
        })
        .collect::<Vec<_>>();
    let all_ready = ready.len() == request_attachment_ids.len();

    AttachmentImportCompletion {
        attachments: refreshed,
        ready,
        failure,
        request_found: true,
        all_ready,
        embedding_lookup_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EditorWorkflowService, MediaCompatibilityWorkflowAction,
        resolve_attachment_import_completion,
    };
    use crate::application::attachments::AttachmentService;
    use crate::application::editor::EditorService;
    use crate::application::errors::AttachmentError;
    use crate::application::executor::ApplicationExecutor;
    use crate::application::export::ExportService;
    use crate::autosave::AutosaveController;
    use crate::domain::{Attachment, NoteId};
    use crate::storage::Storage;
    use serde_json::{Value, json};
    use std::{path::PathBuf, time::Duration};

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rustnote-b4-{label}-{}-{}",
            std::process::id(),
            crate::storage::new_id("test")
        ))
    }

    fn test_workflows(label: &str) -> (PathBuf, Storage, EditorWorkflowService) {
        let root = test_root(label);
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("B4 test storage should initialize");
        let autosave = AutosaveController::try_new(storage.clone())
            .expect("B4 test autosave should initialize");
        let workflows = EditorWorkflowService::new(
            EditorService::new(storage.editor_store(), autosave.clone()),
            AttachmentService::new(storage.attachment_store(), autosave),
            ExportService::new(storage.export_store()),
            ApplicationExecutor::new(),
        );
        (root, storage, workflows)
    }

    fn sample_attachment(id: &str, note_id: &NoteId) -> Attachment {
        Attachment {
            id: id.to_string(),
            note_id: note_id.clone(),
            file_name: format!("{id}.txt"),
            mime_type: Some("text/plain".to_string()),
            byte_size: 3,
            sha256: "0".repeat(64),
            image_pixel_width: None,
            image_pixel_height: None,
            image_format: None,
            image_orientation: None,
            image_has_alpha: None,
            image_animated: None,
            source_attachment_id: None,
            image_edit_recipe: None,
            embedded: false,
            unreferenced_at: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn attachment_completion_preserves_refreshed_state_when_lookup_fails() {
        let note_id = NoteId::parse("note_b4").expect("valid note id");
        let refreshed = vec![sample_attachment("att_ready", &note_id)];
        let completion = resolve_attachment_import_completion(
            refreshed.clone(),
            Some("stage warning".to_string()),
            Err(AttachmentError::Unavailable("lookup failed".to_string())),
        );

        assert_eq!(completion.attachments, refreshed);
        assert!(completion.ready.is_empty());
        assert_eq!(completion.failure.as_deref(), Some("stage warning"));
        assert!(!completion.request_found);
        assert!(!completion.all_ready);
        assert!(completion.embedding_lookup_error.is_some());
    }

    #[test]
    fn media_compatibility_preserves_action_specific_failure_status() {
        let (root, storage, workflows) = test_workflows("media-status");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("B4 Tokio runtime should initialize");

        runtime.block_on(async {
            for (action, expected) in [
                (MediaCompatibilityWorkflowAction::Probe, "error"),
                (MediaCompatibilityWorkflowAction::Prepare, "failed"),
                (MediaCompatibilityWorkflowAction::Invalidate, "error"),
            ] {
                let outcome = workflows
                    .media_compatibility(
                        "att_missing".to_string(),
                        "definitely-not-a-profile".to_string(),
                        action,
                    )
                    .await;
                assert_eq!(outcome.status, expected);
                assert!(!outcome.message.is_empty());
            }
        });

        drop(workflows);
        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn attachment_finalization_runs_independently_before_completion_is_awaited() {
        let (root, storage, workflows) = test_workflows("attachment-finalization");
        let note = storage.create_note(None).expect("create B4 note");
        let source = root.join("source.txt");
        std::fs::write(&source, b"B4 attachment").expect("write B4 attachment source");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("B4 Tokio runtime should initialize");

        runtime.block_on(async {
            // Mirror the real browser workflow: the pending attachment node is
            // persisted before managed staging starts. Storage intentionally
            // cancels a finalization whose durable request anchor has vanished.
            let mut pending_note = note.clone();
            let mut editor_state: Value =
                serde_json::from_str(note.editor_state()).expect("parse B4 editor state");
            editor_state["document"]["content"] = json!([{
                "type": "attachment_pending",
                "attrs": {
                    "requestId": "request-b4",
                    "picker": "file",
                    "label": "Importing attachment",
                    "status": "Staging",
                    "items": null,
                    "nodeSchemaVersion": 1
                }
            }]);
            pending_note.replace_document(
                String::new(),
                serde_json::to_string(&editor_state).expect("serialize B4 editor state"),
                String::new(),
            );

            workflows
                .save_attachment_insertion_anchor(pending_note.clone())
                .await
                .expect("persist B4 attachment insertion anchor");

            let staged = workflows
                .stage_attachment_import(pending_note, "request-b4".to_string(), vec![source])
                .await
                .expect("stage B4 attachment");
            assert_eq!(staged.prepared.len(), 1);

            let finalization =
                workflows.start_attachment_finalization(staged.failure, staged.prepared);

            let mut durable_before_wait = false;
            for _ in 0..500 {
                if storage
                    .list_attachments(&note.id)
                    .expect("inspect B4 attachment state")
                    .len()
                    == 1
                {
                    durable_before_wait = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(
                durable_before_wait,
                "finalization must continue without waiting on the browser-facing completion step"
            );

            let completion = workflows
                .complete_attachment_finalization(finalization)
                .await
                .expect("complete B4 attachment finalization");
            assert_eq!(completion.attachments.len(), 1);
            assert_eq!(completion.ready.len(), 1);
            assert!(completion.request_found);
            assert!(completion.all_ready);
            assert!(completion.embedding_lookup_error.is_none());
        });

        drop(workflows);
        drop(storage);
        let _ = std::fs::remove_dir_all(root);
    }
}

//! Bridge-facing editor workflow adapter.
//!
//! `editor.rs` owns rendering, local signals, and intent guards. This module performs the
//! browser-runtime choreography around `EditorWorkflowService` without leaking Dioxus or
//! JavaScript transport into the application layer.

use std::{
    fmt::{self, Display, Formatter},
    path::Path,
};

use dioxus::prelude::*;
use rfd::AsyncFileDialog;

use crate::application::attachments::{
    AttachmentPickerCategory, AttachmentService, ImageEditRequest, ImageImportRequest,
};
use crate::application::editor_workflows::{
    AttachmentImportCompletion, EditorWorkflowService, MediaCompatibilityWorkflowAction,
};
use crate::application::errors::EditorWorkflowError;
use crate::application::persistence::unix_timestamp;
use crate::application::settings::EditorPreferences;
use crate::editor_bridge::{
    AttachmentPickerKind, EditorBridgeDecodeError, EditorDocumentUpdate, EditorImageEditPayload, EditorImageImportPayload,
};
use crate::domain::{Attachment, Note, NoteLinkTarget};

use super::{
    eval_bridge,
    privacy::{NotePrivacyPublicationScope, PrivacyPublicationToken},
};
use super::live_editor::{
    EditorInstallRequest, editor_install_script, install_editor_bridge,
    live_editor_begin_attachment_import_script, live_editor_complete_attachment_picker_script,
    live_editor_complete_image_edit_script, live_editor_complete_image_import_script,
    live_editor_complete_pdf_export_script, live_editor_dispose_script,
    live_editor_fail_attachment_picker_script, live_editor_fail_image_edit_script,
    live_editor_fail_image_import_script, live_editor_media_compatibility_result_script,
    live_editor_recover_attachment_imports_script, live_editor_replace_state_script,
    live_editor_sync_script, live_editor_update_attachment_import_script,
    live_editor_validate_state_script,
};
use super::note_metadata::NoteRevisionRestoreRequest;
use super::synchronization::{SynchronizationError, apply_editor_update, project_editor_update_async};

#[derive(Debug)]
pub(super) enum EditorCoordinatorError {
    Workflow(EditorWorkflowError),
    Bridge(EditorBridgeDecodeError),
    Synchronization(SynchronizationError),
    BridgeUnavailable,
    EditorChanged,
    UnsupportedRestoredState,
}

impl Display for EditorCoordinatorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workflow(error) => Display::fmt(error, formatter),
            Self::Bridge(error) => Display::fmt(error, formatter),
            Self::Synchronization(error) => Display::fmt(error, formatter),
            Self::BridgeUnavailable => formatter.write_str(
                "could not synchronize the current editor state",
            ),
            Self::EditorChanged => formatter.write_str(
                "the editor changed while version history was preparing the restore",
            ),
            Self::UnsupportedRestoredState => formatter.write_str(
                "the selected version contains an editor state this runtime cannot open",
            ),
        }
    }
}

impl std::error::Error for EditorCoordinatorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Workflow(error) => Some(error),
            Self::Bridge(error) => Some(error),
            Self::Synchronization(error) => Some(error),
            Self::BridgeUnavailable | Self::EditorChanged | Self::UnsupportedRestoredState => None,
        }
    }
}

impl From<EditorWorkflowError> for EditorCoordinatorError {
    fn from(error: EditorWorkflowError) -> Self { Self::Workflow(error) }
}

impl From<EditorBridgeDecodeError> for EditorCoordinatorError {
    fn from(error: EditorBridgeDecodeError) -> Self { Self::Bridge(error) }
}

impl From<SynchronizationError> for EditorCoordinatorError {
    fn from(error: SynchronizationError) -> Self { Self::Synchronization(error) }
}

#[derive(Debug)]
pub(super) struct RestoredRevision {
    pub(super) note: Note,
    pub(super) attachments: Vec<Attachment>,
    pub(super) note_link_targets: Vec<NoteLinkTarget>,
    pub(super) generation: u64,
    pub(super) warnings: Vec<String>,
    pub(super) requires_remount: bool,
    pub(super) publication_token: PrivacyPublicationToken,
    _authorization: Option<crate::application::editor::RevisionRestoreAuthorization>,
}

#[derive(Clone)]
pub(super) struct AttachmentWorkflowPresentation {
    attachment_service: AttachmentService,
    draft: Signal<Note>,
    attachments: Signal<Vec<Attachment>>,
    onchanged: EventHandler<Note>,
    onstatus: EventHandler<String>,
}

impl AttachmentWorkflowPresentation {
    pub(super) fn new(
        attachment_service: AttachmentService,
        draft: Signal<Note>,
        attachments: Signal<Vec<Attachment>>,
        onchanged: EventHandler<Note>,
        onstatus: EventHandler<String>,
    ) -> Self {
        Self {
            attachment_service,
            draft,
            attachments,
            onchanged,
            onstatus,
        }
    }
}

async fn publish_editor_bool(
    publication_token: &PrivacyPublicationToken,
    script: &str,
) -> Option<bool> {
    let _publication_lease = publication_token.begin_publication()?;
    document::eval(script).join::<bool>().await.ok()
}

async fn publish_editor_usize(
    publication_token: &PrivacyPublicationToken,
    script: &str,
) -> Option<usize> {
    let _publication_lease = publication_token.begin_publication()?;
    document::eval(script).join::<usize>().await.ok()
}

async fn publish_editor_string(
    publication_token: &PrivacyPublicationToken,
    script: &str,
) -> Option<String> {
    let _publication_lease = publication_token.begin_publication()?;
    document::eval(script).join::<String>().await.ok()
}

pub(super) async fn run_attachment_import(
    workflows: EditorWorkflowService,
    request_id: String,
    picker: AttachmentPickerKind,
    editor_update: EditorDocumentUpdate,
    publication_token: PrivacyPublicationToken,
    presentation: AttachmentWorkflowPresentation,
) {
    if !publication_token.is_publishable() {
        return;
    }
    let AttachmentWorkflowPresentation {
        attachment_service,
        mut draft,
        mut attachments,
        onchanged,
        onstatus,
    } = presentation;
    let projected = match project_editor_update_async(editor_update).await {
        Ok(projected) if publication_token.is_publishable() => projected,
        Ok(_) => return,
        Err(error) => {
            fail_attachment_picker(&request_id).await;
            onstatus.call(format!(
                "Could not prepare the attachment insertion point: {error}"
            ));
            return;
        }
    };

    let mut anchored_note = draft.read().clone();
    if apply_editor_update(&mut anchored_note, projected) {
        anchored_note.updated_at = unix_timestamp();
        let published = publication_token.publish(|| {
            draft.set(anchored_note.clone());
            onchanged.call(anchored_note.clone());
        });
        if !published {
            return;
        }
    }
    if let Err(error) = workflows
        .save_attachment_insertion_anchor(anchored_note)
        .await
    {
        fail_attachment_picker(&request_id).await;
        onstatus.call(error.to_string());
        return;
    }
    if !publication_token.is_publishable() {
        return;
    }

    let Some(files) = picker.dialog().pick_files().await else {
        fail_attachment_picker(&request_id).await;
        return;
    };
    if !publication_token.is_publishable() {
        return;
    }
    let paths = files
        .iter()
        .map(|file| file.path().to_path_buf())
        .collect::<Vec<_>>();
    if let Some(invalid) = paths.iter().find(|path| !picker.accepts(path)) {
        fail_attachment_picker(&request_id).await;
        onstatus.call(format!(
            "{} is not a supported {}",
            invalid
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Selected file"),
            picker.noun(),
        ));
        return;
    }

    let placeholder_script = live_editor_begin_attachment_import_script(&request_id, &paths);
    if !publication_token.is_publishable() {
        return;
    }
    let _ = publish_editor_bool(&publication_token, &placeholder_script).await;
    if !publication_token.is_publishable() {
        return;
    }

    let mut current = draft.read().clone();
    current.updated_at = unix_timestamp();
    onstatus.call("Preparing managed attachment…".to_string());
    let staged = match workflows
        .stage_attachment_import(current, request_id.clone(), paths)
        .await
    {
        Ok(staged) if !staged.prepared.is_empty() => staged,
        Ok(staged) => {
            fail_attachment_picker(&request_id).await;
            if let Some(message) = staged.failure {
                onstatus.call(message);
            }
            return;
        }
        Err(error) => {
            fail_attachment_picker(&request_id).await;
            onstatus.call(format!("Could not stage attachment: {error}"));
            return;
        }
    };

    if !publication_token.is_publishable() {
        return;
    }
    let update_script = live_editor_update_attachment_import_script(&request_id, &staged.prepared);
    let finalization = workflows.start_attachment_finalization(
        staged.failure,
        staged.prepared,
    );

    if !publication_token.is_publishable() {
        return;
    }
    let _ = publish_editor_bool(&publication_token, &update_script).await;
    if !publication_token.is_publishable() {
        return;
    }
    onstatus.call("Attachment staged safely; verifying integrity in background…".to_string());

    let completed = match workflows.complete_attachment_finalization(finalization).await {
        Ok(completed) => completed,
        Err(error) => {
            onstatus.call(format!(
                "Attachment data is safe; verification will resume later: {error}"
            ));
            return;
        }
    };

    let AttachmentImportCompletion {
        attachments: refreshed,
        ready,
        failure,
        request_found,
        all_ready,
        embedding_lookup_error,
    } = completed;
    let mut updated = draft.read().clone();
    updated.attachment_names = refreshed
        .iter()
        .map(|attachment| attachment.file_name.clone())
        .collect();
    let published = publication_token.publish(|| {
        attachments.set(refreshed);
        draft.set(updated.clone());
        onchanged.call(updated);
    });
    if !published {
        return;
    }

    if let Some(error) = embedding_lookup_error {
        onstatus.call(format!(
            "Attachment data is safe, but embedding will resume later. {error}"
        ));
        return;
    }
    if !request_found {
        fail_attachment_picker(&request_id).await;
        if let Some(message) = failure.as_ref() {
            onstatus.call(message.clone());
        }
        return;
    }
    if !all_ready {
        onstatus.call(match failure.as_ref() {
            Some(message) => format!(
                "Attachment data is safe; verification will resume later. {message}"
            ),
            None => "Attachment data is safe; verification will resume later.".to_string(),
        });
        return;
    }

    let script = live_editor_complete_attachment_picker_script(
        &request_id,
        &attachment_service,
        &ready,
    );
    if !publication_token.is_publishable() {
        return;
    }
    let inserted = publish_editor_usize(&publication_token, &script)
        .await
        .unwrap_or_default();
    if !publication_token.is_publishable() {
        return;
    }
    if inserted != ready.len() {
        onstatus.call(format!(
            concat!(
                "Finalized {} attachment(s), but the document insertion will be recovered ",
                "safely if needed"
            ),
            ready.len()
        ));
    } else if let Some(message) = failure {
        onstatus.call(format!("Embedded {inserted} attachment(s). {message}"));
    } else {
        onstatus.call(format!("Embedded {inserted} attachment(s)"));
    }
}

pub(super) async fn run_media_compatibility(
    workflows: EditorWorkflowService,
    attachment_id: String,
    profile: String,
    action: MediaCompatibilityWorkflowAction,
    publication_token: PrivacyPublicationToken,
) {
    let outcome = workflows
        .media_compatibility(attachment_id.clone(), profile.clone(), action)
        .await;
    if !publication_token.is_publishable() {
        return;
    }
    let script = live_editor_media_compatibility_result_script(
        &attachment_id,
        &profile,
        &outcome.status,
        &outcome.message,
    );
    let _ = publish_editor_bool(&publication_token, &script).await;
}

pub(super) async fn run_pending_attachment_open(
    workflows: EditorWorkflowService,
    ingest_id: String,
    publication_token: PrivacyPublicationToken,
    onstatus: EventHandler<String>,
) {
    if let Err(error) = workflows.open_pending_attachment(ingest_id).await
        && publication_token.is_publishable()
    {
        onstatus.call(format!("Could not open pending attachment: {error}"));
    }
}

pub(super) async fn run_pending_attachment_recovery(
    workflows: EditorWorkflowService,
    note_id: crate::domain::NoteId,
    generation: u64,
    editor_generation: Signal<u64>,
    publication_token: PrivacyPublicationToken,
    presentation: AttachmentWorkflowPresentation,
) {
    let AttachmentWorkflowPresentation {
        attachment_service,
        mut draft,
        mut attachments,
        onchanged,
        onstatus,
    } = presentation;
    let recovered = workflows.recover_pending_attachments(note_id.clone()).await;
    if generation != *editor_generation.read() || !publication_token.is_publishable() {
        return;
    }
    let recovered = match recovered {
        Ok(recovered) => recovered,
        Err(error) => {
            onstatus.call(format!("Attachment recovery will retry later: {error}"));
            return;
        }
    };
    if generation != *editor_generation.read() {
        return;
    }

    let refreshed = recovered.attachments;
    let mut updated = draft.read().clone();
    updated.attachment_names = refreshed
        .iter()
        .map(|attachment| attachment.file_name.clone())
        .collect();
    let published = publication_token.publish(|| {
        attachments.set(refreshed.clone());
        draft.set(updated.clone());
        onchanged.call(updated);
    });
    if !published {
        return;
    }
    let script = live_editor_recover_attachment_imports_script(
        &attachment_service,
        &note_id,
        &refreshed,
    );
    if !publication_token.is_publishable() {
        return;
    }
    let _ = publish_editor_bool(&publication_token, &script).await;
    if !publication_token.is_publishable() {
        return;
    }
    if recovered.unresolved > 0 {
        onstatus.call(format!(
            "{} attachment import(s) are still being recovered",
            recovered.unresolved
        ));
    }
}

pub(super) async fn run_pdf_export(
    workflows: EditorWorkflowService,
    note_id: crate::domain::NoteId,
    request_id: String,
    attachment_id: String,
    export_token: String,
    publication_token: PrivacyPublicationToken,
    onstatus: EventHandler<String>,
) {
    let outcome = workflows
        .save_pdf_export(note_id, attachment_id, export_token)
        .await;
    if !publication_token.is_publishable() {
        return;
    }
    match outcome {
        Ok(crate::application::export::PdfExportOutcome::Saved) => {
            let script = live_editor_complete_pdf_export_script(
                &request_id,
                true,
                "Annotated PDF saved.",
            );
            let _ = publish_editor_bool(&publication_token, &script).await;
            onstatus.call("Annotated PDF saved".to_string());
        }
        Ok(crate::application::export::PdfExportOutcome::Cancelled) => {
            let script = live_editor_complete_pdf_export_script(&request_id, false, "cancelled");
            let _ = publish_editor_bool(&publication_token, &script).await;
        }
        Err(error) => {
            let error_message = error.to_string();
            let script = live_editor_complete_pdf_export_script(
                &request_id,
                false,
                &error_message,
            );
            let _ = publish_editor_bool(&publication_token, &script).await;
            onstatus.call(format!("Could not save annotated PDF: {error_message}"));
        }
    }
}

pub(super) fn reject_image_edit(
    workflows: &EditorWorkflowService,
    request_id: &str,
    stage_token: &str,
    message: &str,
    onstatus: EventHandler<String>,
) {
    workflows.discard_image_edit_stage(stage_token);
    let script = live_editor_fail_image_edit_script(request_id, message);
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
    onstatus.call(message.to_string());
}

pub(super) fn reject_image_import(
    workflows: &EditorWorkflowService,
    request_id: &str,
    images: &[EditorImageImportPayload],
    message: &str,
    onstatus: EventHandler<String>,
) {
    workflows.discard_image_import_stages(images.iter().map(|image| image.stage_token.as_str()));
    let script = live_editor_fail_image_import_script(request_id);
    spawn(async move {
        let _ = document::eval(&script).join::<bool>().await;
    });
    onstatus.call(message.to_string());
}

pub(super) async fn run_image_edit(
    workflows: EditorWorkflowService,
    attachment_service: AttachmentService,
    note_id: crate::domain::NoteId,
    payload: EditorImageEditPayload,
    publication_token: PrivacyPublicationToken,
    mut attachments: Signal<Vec<Attachment>>,
    onstatus: EventHandler<String>,
) {
    let request_id = payload.request_id.clone();
    let stage_token = payload.stage_token.clone();
    let request = ImageEditRequest {
        request_id: payload.request_id,
        attachment_id: payload.attachment_id,
        source_attachment_id: payload.source_attachment_id,
        stage_token: payload.stage_token,
        file_name: payload.file_name,
        mime_type: payload.mime_type,
        edit_recipe: payload.edit_recipe,
    };

    let outcome = workflows.commit_image_edit(note_id, request).await;
    match outcome {
        Ok(result) => {
            let published = publication_token.publish(|| {
                attachments.set(result.attachments.clone());
            });
            if !published {
                return;
            }
            let script = live_editor_complete_image_edit_script(
                &request_id,
                &attachment_service,
                &result.added,
            );
            if !publication_token.is_publishable() {
                return;
            }
            let completed = publish_editor_bool(&publication_token, &script)
                .await
                .unwrap_or(false);
            if !publication_token.is_publishable() {
                return;
            }
            if completed {
                onstatus.call("Image edit applied".to_string());
            } else {
                onstatus.call(
                    concat!(
                        "The edited image was stored, but the editor could not apply it; ",
                        "the unused rendition will be cleaned up safely"
                    )
                    .to_string(),
                );
            }
        }
        Err(error) => {
            workflows.discard_image_edit_stage(&stage_token);
            let error_message = error.to_string();
            let script = live_editor_fail_image_edit_script(&request_id, &error_message);
            let _ = publish_editor_bool(&publication_token, &script).await;
            onstatus.call(format!("Could not apply image edit: {error}"));
        }
    }
}

pub(super) struct ImageImportFlow {
    pub(super) workflows: EditorWorkflowService,
    pub(super) attachment_service: AttachmentService,
    pub(super) note_id: crate::domain::NoteId,
    pub(super) request_id: String,
    pub(super) images: Vec<EditorImageImportPayload>,
    pub(super) publication_token: PrivacyPublicationToken,
    pub(super) attachments: Signal<Vec<Attachment>>,
    pub(super) onstatus: EventHandler<String>,
}

pub(super) async fn run_image_import(flow: ImageImportFlow) {
    let ImageImportFlow {
        workflows,
        attachment_service,
        note_id,
        request_id,
        images,
        publication_token,
        mut attachments,
        onstatus,
    } = flow;
    let images = images
        .into_iter()
        .map(|image| ImageImportRequest {
            stage_token: image.stage_token,
            file_name: image.file_name,
            mime_type: image.mime_type,
        })
        .collect();

    let outcome = workflows.import_images(note_id, images).await;
    match outcome {
        Ok(result) => {
            let published = publication_token.publish(|| {
                attachments.set(result.attachments.clone());
            });
            if !published {
                return;
            }
            if result.added.is_empty() {
                let script = live_editor_fail_image_import_script(&request_id);
                let _ = publish_editor_bool(&publication_token, &script).await;
                if let Some(failure) = result.failure {
                    onstatus.call(failure);
                }
                return;
            }

            let script = live_editor_complete_image_import_script(
                &request_id,
                &attachment_service,
                &result.added,
            );
            if !publication_token.is_publishable() {
                return;
            }
            let inserted = publish_editor_usize(&publication_token, &script)
                .await
                .unwrap_or_default();
            if !publication_token.is_publishable() {
                return;
            }
            if inserted != result.added.len() {
                onstatus.call(format!(
                    concat!(
                        "Imported {} image(s), but only {} could be embedded; ",
                        "unused imports will be cleaned up safely"
                    ),
                    result.added.len(),
                    inserted
                ));
            } else if let Some(failure) = result.failure {
                onstatus.call(format!("Embedded {inserted} image(s). {failure}"));
            } else {
                onstatus.call(format!("Embedded {inserted} image(s)"));
            }
        }
        Err(error) => {
            let script = live_editor_fail_image_import_script(&request_id);
            let _ = publish_editor_bool(&publication_token, &script).await;
            onstatus.call(format!("Could not import image: {error}"));
        }
    }
}

pub(super) struct RevisionRestoreFlow {
    pub(super) attachment_service: AttachmentService,
    pub(super) workflows: EditorWorkflowService,
    pub(super) current: Note,
    pub(super) current_generation: u64,
    pub(super) request: NoteRevisionRestoreRequest,
    pub(super) editor_preferences: EditorPreferences,
    pub(super) pre_restore_publication_token: PrivacyPublicationToken,
    pub(super) publication_scope: NotePrivacyPublicationScope,
}

pub(super) async fn restore_revision_flow(
    flow: RevisionRestoreFlow,
    next_generation: impl FnOnce() -> u64,
) -> Result<RestoredRevision, EditorCoordinatorError> {
    let RevisionRestoreFlow {
        attachment_service,
        workflows,
        mut current,
        current_generation,
        request,
        editor_preferences,
        pre_restore_publication_token,
        publication_scope,
    } = flow;
    let ensure_pre_restore_privacy_current = || {
        if pre_restore_publication_token.is_publishable() {
            Ok(())
        } else {
            Err(EditorCoordinatorError::EditorChanged)
        }
    };
    ensure_pre_restore_privacy_current()?;
    let sync_script = live_editor_sync_script();
    let serialized = eval_bridge::join_idempotent::<String>(&sync_script)
        .await
        .map_err(|_| EditorCoordinatorError::BridgeUnavailable)?;
    ensure_pre_restore_privacy_current()?;
    let current_update = crate::editor_bridge::decode_editor_document_update(&serialized)
        .map_err(EditorCoordinatorError::from)?;
    if current_update.generation != current_generation {
        return Err(EditorCoordinatorError::EditorChanged);
    }
    if current.editor_state() != current_update.editor_state.as_str() {
        let current_update = project_editor_update_async(current_update)
            .await
            .map_err(EditorCoordinatorError::from)?;
        ensure_pre_restore_privacy_current()?;
        let _ = apply_editor_update(&mut current, current_update);
    }
    current.updated_at = unix_timestamp();

    let validate_script = live_editor_validate_state_script(request.editor_state.as_str());
    let valid = publish_editor_bool(&pre_restore_publication_token, &validate_script)
        .await
        .unwrap_or(false);
    ensure_pre_restore_privacy_current()?;
    if !valid {
        return Err(EditorCoordinatorError::UnsupportedRestoredState);
    }

    ensure_pre_restore_privacy_current()?;
    let restored_persistence = workflows
        .restore_revision(current, request.revision_id)
        .await?;

    // Restore admission intentionally invalidates every pre-Restore publication
    // authorization. The committed state must therefore cross the UI boundary
    // under a fresh post-Restore authorization; reusing the old token would
    // deterministically reject the restore after persistence succeeded.
    let publication_token = publication_scope
        .capture()
        .ok_or(EditorCoordinatorError::EditorChanged)?;
    let ensure_privacy_current = || {
        if publication_token.is_publishable() {
            Ok(())
        } else {
            Err(EditorCoordinatorError::EditorChanged)
        }
    };
    ensure_privacy_current()?;
    let mut restored = restored_persistence.note;
    let attachments = restored_persistence.attachments;
    let note_link_targets = restored_persistence.note_link_targets;
    let mut warnings = restored_persistence.warnings;
    let authorization = restored_persistence.authorization;
    let generation = next_generation();
    ensure_privacy_current()?;
    let replacement_script = live_editor_replace_state_script(restored.editor_state(), generation);
    let replacement = publish_editor_string(&publication_token, &replacement_script).await;
    ensure_privacy_current()?;
    let mut confirmed_update = replacement
        .as_deref()
        .and_then(|serialized| crate::editor_bridge::decode_editor_document_update(serialized).ok())
        .filter(|update| update.generation == generation);
    let mut requires_remount = false;

    if confirmed_update.is_none() {
        let _ = document::eval(&live_editor_dispose_script(false))
            .join::<bool>()
            .await;
        ensure_privacy_current()?;
        let install_script = editor_install_script(EditorInstallRequest {
            editor_state: restored.editor_state(),
            note_id: restored.id.as_str(),
            generation,
            editable: false,
            service: &attachment_service,
            attachments: &attachments,
            note_link_targets: &note_link_targets,
            editor_preferences,
        });
        let installed = if let Some(_publication_lease) = publication_token.begin_publication() {
            install_editor_bridge(&install_script, generation)
                .await
                .unwrap_or(false)
        } else {
            false
        };
        ensure_privacy_current()?;
        if installed {
            let sync_script = live_editor_sync_script();
            confirmed_update = eval_bridge::join_idempotent::<String>(&sync_script)
                .await
                .ok()
                .and_then(|serialized| crate::editor_bridge::decode_editor_document_update(&serialized).ok())
                .filter(|update| update.generation == generation);
            ensure_privacy_current()?;
        }
        if confirmed_update.is_none() {
            requires_remount = true;
            warnings.push(
                "The restored version was committed, and the editor surface will be remounted."
                    .to_string(),
            );
        } else {
            warnings.push(
                "The restored version was committed, and the editor surface was reloaded."
                    .to_string(),
            );
        }
    }

    if let Some(replacement_update) = confirmed_update
        && restored.editor_state() != replacement_update.editor_state.as_str()
    {
        match project_editor_update_async(replacement_update).await {
            Ok(replacement_update) => {
                ensure_privacy_current()?;
                let normalized = apply_editor_update(&mut restored, replacement_update);
                if normalized {
                    if let Some(restore_authorization) = authorization.as_ref() {
                        warnings.extend(
                            workflows
                                .persist_revision_normalization_after_protected_restore(
                                    restored.clone(),
                                    restore_authorization.clone(),
                                )
                                .await,
                        );
                    } else {
                        warnings.extend(
                            workflows
                                .persist_revision_normalization(restored.clone())
                                .await,
                        );
                        ensure_privacy_current()?;
                    }
                }
            }
            Err(error) => {
                requires_remount = true;
                warnings.push(format!(
                    concat!(
                        "The restored version was committed, but the runtime-normalized editor ",
                        "state could not be projected and will be remounted: {}"
                    ),
                    error
                ));
            }
        }
    }

    ensure_privacy_current()?;
    Ok(RestoredRevision {
        note: restored,
        attachments,
        note_link_targets,
        generation,
        warnings,
        requires_remount,
        publication_token,
        _authorization: authorization,
    })
}

async fn fail_attachment_picker(request_id: &str) {
    let script = live_editor_fail_attachment_picker_script(request_id);
    let _ = document::eval(&script).join::<bool>().await;
}

impl AttachmentPickerKind {
    fn dialog(self) -> AsyncFileDialog {
        let dialog = AsyncFileDialog::new().set_title(self.prompt());
        let Some(kind) = self.picker_category() else {
            return dialog;
        };
        let extensions = AttachmentService::picker_extensions(kind);
        dialog.add_filter(self.filter_label(), extensions.as_slice())
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::Image => "Choose image files",
            Self::Audio => "Choose audio files",
            Self::Video => "Choose video files",
            Self::Pdf => "Choose PDF files",
            Self::File => "Choose files",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio file",
            Self::Video => "video",
            Self::Pdf => "PDF",
            Self::File => "file",
        }
    }

    fn accepts(self, path: &Path) -> bool {
        match self.picker_category() {
            Some(category) => AttachmentService::path_matches_picker_category(path, category),
            None => true,
        }
    }

    fn picker_category(self) -> Option<AttachmentPickerCategory> {
        match self {
            Self::Image => Some(AttachmentPickerCategory::Image),
            Self::Audio => Some(AttachmentPickerCategory::Audio),
            Self::Video => Some(AttachmentPickerCategory::Video),
            Self::Pdf => Some(AttachmentPickerCategory::Pdf),
            Self::File => None,
        }
    }

    const fn filter_label(self) -> &'static str {
        match self {
            Self::Image => "Images",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Pdf => "PDF",
            Self::File => "Files",
        }
    }
}

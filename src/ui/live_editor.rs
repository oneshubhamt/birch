use std::{path::PathBuf, sync::Arc};

use crate::{
    application::{
        attachments::{AttachmentService, PreparedAttachmentImport},
        settings::EditorPreferences,
    },
    domain::{
        Attachment, NoteCandidate, NoteDocument, NoteId, NoteLinkTarget, NoteLinkTargetState,
    },
    editor_bridge::{
        EditorAttachmentCommand, EditorAttachmentDescriptor, EditorAttachmentImportItem,
        EditorAttachmentImportRecovery, EditorAttachmentImportRecoveryGroup, EditorCommand,
        EditorDocumentUpdate, EditorEvent, EditorInstallPayload, EditorMediaCompatibilityCandidate,
        EditorNoteLinkCommand, EditorNoteLinkInsertDescriptor, decode_editor_document_update,
        decode_editor_event, dispatch_expression, dispatch_script, dispatch_string_script,
        dispatch_usize_script, install_script, runtime_error_script, runtime_ready_script,
        wait_for_generation_then_dispatch_script,
    },
    editor_runtime::EDITOR_RUNTIME_MARKER,
};
use dioxus::core::use_drop;
use dioxus::prelude::*;

use super::{eval_bridge, privacy::PrivacyPublicationToken};

#[derive(Clone)]
pub(super) struct LiveEditorView {
    pub note_id: NoteId,
    pub document: Arc<NoteDocument>,
    pub generation: u64,
    pub editable: bool,
    pub attachment_service: AttachmentService,
    pub attachments: Vec<Attachment>,
    pub note_link_targets: Vec<NoteLinkTarget>,
    pub editor_preferences: EditorPreferences,
    pub publication_token: PrivacyPublicationToken,
}

impl PartialEq for LiveEditorView {
    fn eq(&self, other: &Self) -> bool {
        self.note_id == other.note_id
            && self.generation == other.generation
            && self.editable == other.editable
            && self.attachment_service == other.attachment_service
            && self.attachments == other.attachments
            && self.note_link_targets == other.note_link_targets
            && (Arc::ptr_eq(&self.document, &other.document) || self.document == other.document)
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct LiveEditorActions {
    pub onchange: EventHandler<EditorDocumentUpdate>,
    pub oncommit: EventHandler<EditorDocumentUpdate>,
    pub onattachmentcommand: EventHandler<EditorAttachmentCommand>,
    pub onnotelinkcommand: EventHandler<EditorNoteLinkCommand>,
    pub onstatus: EventHandler<String>,
}

#[component]
pub(super) fn NoteDocumentEditor(view: LiveEditorView, actions: LiveEditorActions) -> Element {
    let LiveEditorView {
        note_id,
        document,
        generation,
        editable,
        attachment_service,
        attachments,
        note_link_targets,
        editor_preferences,
        publication_token,
    } = view;
    let LiveEditorActions {
        onchange,
        oncommit,
        onattachmentcommand,
        onnotelinkcommand,
        onstatus,
    } = actions;
    let install_document = document.clone();

    use_drop(move || {
        let _ = document::eval(&live_editor_dispose_script(false));
    });

    let _runtime_effect = use_effect(move || {
        let script = editor_install_script(EditorInstallRequest {
            editor_state: install_document.editor_state.as_str(),
            note_id: note_id.as_str(),
            generation,
            editable,
            service: &attachment_service,
            attachments: &attachments,
            note_link_targets: &note_link_targets,
            editor_preferences,
        });
        let publication_token = publication_token.clone();
        spawn(async move {
            let Some(publication_lease) = publication_token.begin_publication() else {
                return;
            };
            let installed = install_editor_bridge(&script, generation).await;
            if !publication_token.is_publishable() {
                let _ = document::eval(&live_editor_dispose_script(false))
                    .join::<bool>()
                    .await;
                drop(publication_lease);
                return;
            }
            drop(publication_lease);
            match installed {
                Ok(true) => {}
                Ok(false) => {
                    let error_script = editor_runtime_error_script();
                    let detail = eval_bridge::join_idempotent::<String>(&error_script)
                        .await
                        .unwrap_or_default();
                    let message = if detail.trim().is_empty() {
                        "The editor runtime could not initialise the note".to_string()
                    } else {
                        format!("The editor could not initialise: {detail}")
                    };
                    onstatus.call(message);
                }
                Err(error) => {
                    onstatus.call(format!("The editor bridge could not initialise: {error}"))
                }
            }
        });
    });

    rsx! {
        div {
            id: "note-prosemirror-editor",
            class: "prosemirror-editor markdown-preview",
            role: "presentation",
            "data-placeholder": "Start writing",
            "data-runtime-ready": "false",
            "data-runtime-error": "false",
            onblur: move |_| {
                spawn(async move {
                    let sync_script = live_editor_sync_script();
                    match eval_bridge::join_idempotent::<String>(&sync_script).await
                    {
                        Ok(serialized) => match decode_editor_document_update(&serialized) {
                            Ok(update) => oncommit.call(update),
                            Err(error) => onstatus.call(error.to_string()),
                        },
                        Err(_) => onstatus.call("Could not read the editor while saving".to_string()),
                    }
                });
            }
        }
        textarea {
            id: "note-editor-event-bridge",
            class: "editor-state-bridge",
            value: "",
            tabindex: "-1",
            aria_hidden: "true",
            oninput: move |event| {
                match decode_editor_event(&event.value()) {
                    Ok(EditorEvent::DocumentChanged(update)) => onchange.call(update),
                    Ok(EditorEvent::AttachmentCommand(command)) => onattachmentcommand.call(command),
                    Ok(EditorEvent::NoteLinkCommand(command)) => onnotelinkcommand.call(command),
                    Err(error) => onstatus.call(error.to_string()),
                }
            },
        }
    }
}

fn attachment_runtime_descriptor(
    service: &AttachmentService,
    attachment: &Attachment,
) -> EditorAttachmentDescriptor {
    let runtime = service.runtime_descriptor(attachment);
    EditorAttachmentDescriptor {
        id: attachment.id.clone(),
        kind: runtime.kind,
        file_name: attachment.file_name.clone(),
        mime_type: runtime.mime_type,
        compatible_playback_candidates: runtime
            .compatible_playback_candidates
            .into_iter()
            .map(|candidate| EditorMediaCompatibilityCandidate {
                profile: candidate.profile,
                mime_type: candidate.mime_type,
                preview_url: candidate.preview_url,
            })
            .collect(),
        byte_size: attachment.byte_size,
        available: runtime.available,
        preview_url: runtime.preview_url,
        compatible_preview_url: runtime.compatible_preview_url,
        screen_preview_url: runtime.screen_preview_url,
        image_pixel_width: attachment.image_pixel_width,
        image_pixel_height: attachment.image_pixel_height,
        image_format: attachment.image_format.clone(),
        image_orientation: attachment.image_orientation,
        image_has_alpha: attachment.image_has_alpha,
        image_animated: attachment.image_animated,
        source_attachment_id: attachment.source_attachment_id.clone(),
        image_edit_recipe: attachment.image_edit_recipe.clone(),
    }
}

fn attachment_runtime_descriptors(
    service: &AttachmentService,
    attachments: &[Attachment],
) -> Vec<EditorAttachmentDescriptor> {
    attachments
        .iter()
        .map(|attachment| attachment_runtime_descriptor(service, attachment))
        .collect()
}

fn attachment_import_recovery(
    service: &AttachmentService,
    note_id: &str,
) -> EditorAttachmentImportRecovery {
    let Ok(note_id) = NoteId::parse(note_id) else {
        return EditorAttachmentImportRecovery {
            authoritative: false,
            groups: Vec::new(),
        };
    };
    let Ok(groups) = service.import_recovery_groups(&note_id) else {
        return EditorAttachmentImportRecovery {
            authoritative: false,
            groups: Vec::new(),
        };
    };
    EditorAttachmentImportRecovery {
        authoritative: true,
        groups: groups
            .into_iter()
            .map(|group| EditorAttachmentImportRecoveryGroup {
                request_id: group.request_id,
                attachment_ids: group.attachment_ids,
            })
            .collect(),
    }
}

pub(super) struct EditorInstallRequest<'a> {
    pub editor_state: &'a str,
    pub note_id: &'a str,
    pub generation: u64,
    pub editable: bool,
    pub service: &'a AttachmentService,
    pub attachments: &'a [Attachment],
    pub note_link_targets: &'a [NoteLinkTarget],
    pub editor_preferences: EditorPreferences,
}

pub(super) fn editor_install_script(request: EditorInstallRequest<'_>) -> String {
    let EditorInstallRequest {
        editor_state,
        note_id,
        generation,
        editable,
        service,
        attachments,
        note_link_targets,
        editor_preferences,
    } = request;
    let runtime_entries = attachment_runtime_descriptors(service, attachments);
    let command = EditorCommand::Install(EditorInstallPayload {
        initial_editor_state: editor_state.to_string(),
        note_id: note_id.to_string(),
        generation,
        editable,
        image_import_stage_url: service.image_import_stage_url(),
        attachments: runtime_entries,
        note_link_targets: note_link_targets.to_vec(),
        attachment_import_intents: attachment_import_recovery(service, note_id),
        auto_indent_enabled: editor_preferences.auto_indent,
        auto_first_line_indent: editor_preferences.auto_first_line_indent,
        default_line_spacing: editor_preferences
            .default_line_spacing
            .map(|value| value.as_str().to_string()),
        spellcheck_enabled: editor_preferences.spellcheck,
    });
    install_script(&command, EDITOR_RUNTIME_MARKER)
}

pub(super) async fn install_editor_bridge(
    script: &str,
    generation: u64,
) -> Result<bool, document::EvalError> {
    let first = document::eval(script).join::<bool>().await;
    if !matches!(&first, Err(document::EvalError::Finished)) {
        return first;
    }

    match editor_runtime_is_ready(generation).await {
        Ok(true) => return Ok(true),
        Ok(false) | Err(document::EvalError::Finished) => {}
        Err(error) => return Err(error),
    }

    let retry = document::eval(script).join::<bool>().await;
    if !matches!(&retry, Err(document::EvalError::Finished)) {
        return retry;
    }
    editor_runtime_is_ready(generation).await
}

async fn editor_runtime_is_ready(generation: u64) -> Result<bool, document::EvalError> {
    let ready_script = live_editor_runtime_ready_script(generation);
    eval_bridge::join_idempotent::<bool>(&ready_script).await
}

pub(super) fn live_editor_runtime_ready_script(generation: u64) -> String {
    runtime_ready_script(generation)
}

pub(super) fn editor_runtime_error_script() -> String {
    runtime_error_script()
}

pub(super) fn live_editor_recover_attachment_imports_script(
    service: &AttachmentService,
    note_id: &NoteId,
    attachments: &[Attachment],
) -> String {
    let runtime_entries = attachment_runtime_descriptors(service, attachments);
    dispatch_script(&EditorCommand::RecoverAttachmentImports {
        recovery: attachment_import_recovery(service, note_id.as_str()),
        attachments: runtime_entries,
    })
}

pub(super) fn live_editor_set_attachment_registry_script(
    service: &AttachmentService,
    attachments: &[Attachment],
) -> String {
    let runtime_entries = attachment_runtime_descriptors(service, attachments);
    dispatch_script(&EditorCommand::SetAttachmentRegistry {
        attachments: runtime_entries,
    })
}

pub(super) fn live_editor_complete_image_import_script(
    request_id: &str,
    service: &AttachmentService,
    attachments: &[Attachment],
) -> String {
    let descriptors = attachment_runtime_descriptors(service, attachments);
    dispatch_usize_script(&EditorCommand::CompleteImageImport {
        request_id: request_id.to_string(),
        attachments: descriptors,
    })
}

pub(super) fn live_editor_remove_attachment_from_document_script(attachment_id: &str) -> String {
    dispatch_script(&EditorCommand::RemoveAttachmentFromDocument {
        attachment_id: attachment_id.to_string(),
    })
}

pub(super) fn live_editor_complete_image_edit_script(
    request_id: &str,
    service: &AttachmentService,
    attachment: &Attachment,
) -> String {
    let descriptor = attachment_runtime_descriptor(service, attachment);
    dispatch_script(&EditorCommand::CompleteImageEdit {
        request_id: request_id.to_string(),
        descriptor: Box::new(descriptor),
    })
}

pub(super) fn live_editor_fail_image_edit_script(request_id: &str, message: &str) -> String {
    dispatch_script(&EditorCommand::FailImageEdit {
        request_id: request_id.to_string(),
        message: message.to_string(),
    })
}

pub(super) fn live_editor_begin_attachment_import_script(
    request_id: &str,
    paths: &[PathBuf],
) -> String {
    let items = paths
        .iter()
        .map(|path| EditorAttachmentImportItem {
            file_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("Attachment")
                .to_string(),
            kind: AttachmentService::runtime_kind_for_path(path),
            status: None,
            ingest_id: None,
        })
        .collect::<Vec<_>>();
    dispatch_script(&EditorCommand::BeginAttachmentImport {
        request_id: request_id.to_string(),
        items,
    })
}

pub(super) fn live_editor_update_attachment_import_script(
    request_id: &str,
    prepared: &[PreparedAttachmentImport],
) -> String {
    let items = prepared
        .iter()
        .map(|item| EditorAttachmentImportItem {
            file_name: item.file_name.clone(),
            kind: AttachmentService::runtime_kind(&item.file_name, item.mime_type.as_deref()),
            status: Some("Ready · verifying integrity…".to_string()),
            ingest_id: Some(item.ingest_id.clone()),
        })
        .collect::<Vec<_>>();
    dispatch_script(&EditorCommand::UpdateAttachmentImport {
        request_id: request_id.to_string(),
        items,
    })
}

pub(super) fn live_editor_complete_attachment_picker_script(
    request_id: &str,
    service: &AttachmentService,
    attachments: &[Attachment],
) -> String {
    let descriptors = attachment_runtime_descriptors(service, attachments);
    dispatch_usize_script(&EditorCommand::CompleteAttachmentPicker {
        request_id: request_id.to_string(),
        attachments: descriptors,
    })
}

pub(super) fn live_editor_fail_attachment_picker_script(request_id: &str) -> String {
    dispatch_script(&EditorCommand::FailAttachmentPicker {
        request_id: request_id.to_string(),
    })
}

pub(super) fn live_editor_media_compatibility_result_script(
    attachment_id: &str,
    profile: &str,
    status: &str,
    message: &str,
) -> String {
    dispatch_script(&EditorCommand::MediaCompatibilityResult {
        attachment_id: attachment_id.to_string(),
        profile: profile.to_string(),
        status: status.to_string(),
        message: message.to_string(),
    })
}

pub(super) fn live_editor_complete_pdf_export_script(
    request_id: &str,
    success: bool,
    message: &str,
) -> String {
    dispatch_script(&EditorCommand::CompletePdfExport {
        request_id: request_id.to_string(),
        success,
        message: message.to_string(),
    })
}

pub(super) fn live_editor_fail_image_import_script(request_id: &str) -> String {
    dispatch_script(&EditorCommand::FailImageImport {
        request_id: request_id.to_string(),
    })
}

pub(super) fn live_editor_set_note_link_registry_script(targets: &[NoteLinkTarget]) -> String {
    dispatch_script(&EditorCommand::SetNoteLinkRegistry {
        targets: targets.to_vec(),
    })
}

pub(super) fn live_editor_complete_note_link_picker_script(
    request_id: &str,
    candidate: &NoteCandidate,
) -> String {
    let descriptor = EditorNoteLinkInsertDescriptor {
        note_id: candidate.id.as_str().to_string(),
        title: candidate.display_title().to_string(),
        state: if candidate.archived {
            NoteLinkTargetState::Archived
        } else {
            NoteLinkTargetState::Active
        },
    };
    dispatch_script(&EditorCommand::CompleteNoteLinkPicker {
        request_id: request_id.to_string(),
        descriptor,
    })
}

pub(super) fn live_editor_fail_note_link_picker_script(request_id: &str) -> String {
    dispatch_script(&EditorCommand::FailNoteLinkPicker {
        request_id: request_id.to_string(),
    })
}

pub(super) fn live_editor_set_editable_script(editable: bool) -> String {
    dispatch_script(&EditorCommand::SetEditable { editable })
}

pub(super) fn live_editor_sync_script() -> String {
    dispatch_string_script(&EditorCommand::ReadState {
        dispatch_input: false,
    })
}

pub(super) fn live_editor_sync_script_for_generation(generation: u64) -> String {
    wait_for_generation_then_dispatch_script(
        generation,
        &EditorCommand::ReadState {
            dispatch_input: false,
        },
    )
}

pub(super) fn live_editor_dispose_script(flush: bool) -> String {
    dispatch_script(&EditorCommand::Dispose { flush })
}

pub(super) fn live_editor_validate_state_script(editor_state: &str) -> String {
    dispatch_script(&EditorCommand::ValidateState {
        editor_state: editor_state.to_string(),
    })
}

pub(super) fn live_editor_replace_state_script(editor_state: &str, generation: u64) -> String {
    dispatch_string_script(&EditorCommand::ReplaceState {
        editor_state: editor_state.to_string(),
        generation,
    })
}

pub(super) fn live_editor_format_script(action: &str) -> String {
    dispatch_script(&EditorCommand::Format {
        action: action.to_string(),
    })
}

pub(super) fn live_editor_format_and_dismiss_script(action: &str, surface_id: &str) -> String {
    let expression = dispatch_expression(&EditorCommand::Format {
        action: action.to_string(),
    });
    let surface_id = serde_json::to_string(surface_id).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "const applied = {expression} ?? false; if (applied) window.__rustnoteDismissTransientSurface?.({surface_id}, false); return applied;"
    )
}

pub(super) fn live_editor_typography_script(kind: &str, value: &str) -> String {
    dispatch_script(&EditorCommand::SetTypography {
        kind: kind.to_string(),
        value: value.to_string(),
    })
}

pub(super) fn live_editor_preferences_script(preferences: EditorPreferences) -> String {
    let preferences = preferences.normalized();
    dispatch_script(&EditorCommand::SetEditorPreferences {
        auto_indent_enabled: preferences.auto_indent,
        auto_first_line_indent: preferences.auto_first_line_indent,
        default_line_spacing: preferences
            .default_line_spacing
            .map(|value| value.as_str().to_string()),
        spellcheck_enabled: preferences.spellcheck,
    })
}

pub(super) fn live_editor_adjust_font_size_script(delta: i32) -> String {
    dispatch_script(&EditorCommand::AdjustFontSize { delta })
}

pub(super) fn live_editor_font_size_input_script() -> String {
    dispatch_script(&EditorCommand::ApplyFontSizeInput)
}

pub(super) fn live_editor_refresh_surface_script() -> String {
    dispatch_script(&EditorCommand::RefreshSurface)
}

pub(super) fn live_editor_focus_script() -> String {
    dispatch_script(&EditorCommand::FocusEditor)
}

pub(super) fn live_editor_toggle_focus_script() -> String {
    dispatch_script(&EditorCommand::ToggleEditorFocus)
}

pub(super) fn live_editor_exit_focus_script() -> String {
    dispatch_script(&EditorCommand::ExitFocus)
}

pub(super) fn live_editor_toggle_window_fullscreen_script() -> String {
    dispatch_script(&EditorCommand::ToggleWindowFullscreen)
}

pub(super) fn live_editor_open_insert_menu_script() -> String {
    dispatch_script(&EditorCommand::OpenInsertMenu)
}

pub(super) fn live_editor_find_script(query: &str, requested_index: usize) -> String {
    dispatch_usize_script(&EditorCommand::Find {
        query: query.to_string(),
        requested_index,
    })
}

pub(super) fn live_editor_capture_link_script() -> String {
    dispatch_string_script(&EditorCommand::CaptureLinkSelection)
}

pub(super) fn live_editor_insert_link_script(url: &str, label: &str) -> String {
    dispatch_script(&EditorCommand::InsertLink {
        url: url.to_string(),
        label: label.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_state_response_accepts_the_current_document_update_contract() {
        let update = decode_editor_document_update(
            r#"{"generation":7,"revision":11,"editor_state":"{\"schema_version\":2}"}"#,
        )
        .expect("decode editor update");
        assert_eq!(update.generation, 7);
        assert_eq!(update.revision, 11);
        assert_eq!(update.editor_state, r#"{"schema_version":2}"#);

        assert!(decode_editor_document_update(r#"{"generation":7,"editor_state":"{}"}"#).is_err());
        assert!(decode_editor_document_update("not json").is_err());
    }

    #[test]
    fn host_to_editor_calls_use_only_the_typed_bridge_namespace() {
        let url = "https://example.test/?q=\"quoted\"&line=one\ntwo";
        let label = "label'); window.injected = true; //";
        let script = live_editor_insert_link_script(url, label);
        let encoded_url = serde_json::to_string(url).expect("encode URL");
        let encoded_label = serde_json::to_string(label).expect("encode label");

        assert!(script.contains("window.__rustnoteEditorBridge?.dispatch("));
        assert!(script.contains(&encoded_url));
        assert!(script.contains(&encoded_label));
        assert!(!script.contains("?q=\"quoted\"&line=one\ntwo"));
    }
}

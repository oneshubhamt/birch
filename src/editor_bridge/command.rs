use serde::Serialize;

use crate::domain::{NoteLinkTarget, NoteLinkTargetState};

use super::EDITOR_BRIDGE_PROTOCOL_VERSION;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditorMediaCompatibilityCandidate {
    pub profile: String,
    pub mime_type: String,
    pub preview_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditorAttachmentDescriptor {
    pub id: String,
    pub kind: String,
    pub file_name: String,
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub compatible_playback_candidates: Vec<EditorMediaCompatibilityCandidate>,
    pub byte_size: i64,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatible_preview_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_preview_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_pixel_width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_pixel_height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_orientation: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_has_alpha: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_animated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_attachment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_edit_recipe: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditorAttachmentImportRecoveryGroup {
    pub request_id: String,
    pub attachment_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditorAttachmentImportRecovery {
    pub authoritative: bool,
    pub groups: Vec<EditorAttachmentImportRecoveryGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditorAttachmentImportItem {
    pub file_name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingest_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditorNoteLinkInsertDescriptor {
    pub note_id: String,
    pub title: String,
    pub state: NoteLinkTargetState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum EditorCommand {
    Install(EditorInstallPayload),
    ReadState {
        dispatch_input: bool,
    },
    SetEditable {
        editable: bool,
    },
    SetAttachmentRegistry {
        attachments: Vec<EditorAttachmentDescriptor>,
    },
    RemoveAttachmentFromDocument {
        attachment_id: String,
    },
    RecoverAttachmentImports {
        recovery: EditorAttachmentImportRecovery,
        attachments: Vec<EditorAttachmentDescriptor>,
    },
    SetNoteLinkRegistry {
        targets: Vec<NoteLinkTarget>,
    },
    MediaCompatibilityResult {
        attachment_id: String,
        profile: String,
        status: String,
        message: String,
    },
    CompleteNoteLinkPicker {
        request_id: String,
        descriptor: EditorNoteLinkInsertDescriptor,
    },
    FailNoteLinkPicker {
        request_id: String,
    },
    BeginAttachmentImport {
        request_id: String,
        items: Vec<EditorAttachmentImportItem>,
    },
    UpdateAttachmentImport {
        request_id: String,
        items: Vec<EditorAttachmentImportItem>,
    },
    CompleteAttachmentPicker {
        request_id: String,
        attachments: Vec<EditorAttachmentDescriptor>,
    },
    FailAttachmentPicker {
        request_id: String,
    },
    CompletePdfExport {
        request_id: String,
        success: bool,
        message: String,
    },
    CompleteImageImport {
        request_id: String,
        attachments: Vec<EditorAttachmentDescriptor>,
    },
    FailImageImport {
        request_id: String,
    },
    CompleteImageEdit {
        request_id: String,
        descriptor: Box<EditorAttachmentDescriptor>,
    },
    FailImageEdit {
        request_id: String,
        message: String,
    },
    Dispose {
        flush: bool,
    },
    ValidateState {
        editor_state: String,
    },
    ReplaceState {
        editor_state: String,
        generation: u64,
    },
    Format {
        action: String,
    },
    SetTypography {
        kind: String,
        value: String,
    },
    SetEditorPreferences {
        auto_indent_enabled: bool,
        auto_first_line_indent: u32,
        default_line_spacing: Option<String>,
        spellcheck_enabled: bool,
    },
    ApplyFontSizeInput,
    AdjustFontSize {
        delta: i32,
    },
    RefreshSurface,
    FocusEditor,
    OpenInsertMenu,
    Find {
        query: String,
        requested_index: usize,
    },
    ToggleEditorFocus,
    ExitFocus,
    ToggleWindowFullscreen,
    CaptureLinkSelection,
    InsertLink {
        url: String,
        label: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct EditorInstallPayload {
    pub initial_editor_state: String,
    pub note_id: String,
    pub generation: u64,
    pub editable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_import_stage_url: Option<String>,
    pub attachments: Vec<EditorAttachmentDescriptor>,
    pub note_link_targets: Vec<NoteLinkTarget>,
    pub attachment_import_intents: EditorAttachmentImportRecovery,
    pub auto_indent_enabled: bool,
    pub auto_first_line_indent: u32,
    pub default_line_spacing: Option<String>,
    pub spellcheck_enabled: bool,
}

pub fn serialize_editor_command(command: &EditorCommand) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_value(command)?;
    if let serde_json::Value::Object(ref mut object) = value {
        object.insert(
            "version".to_string(),
            serde_json::Value::from(EDITOR_BRIDGE_PROTOCOL_VERSION),
        );
    }
    serde_json::to_string(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_envelope_is_versioned_typed_and_json_safe() {
        let command = EditorCommand::InsertLink {
            url: "https://example.test/?q=\"quoted\"&line=one\ntwo".to_string(),
            label: "label'); window.injected = true; //".to_string(),
        };
        let serialized = serialize_editor_command(&command).expect("serialize command");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("parse command");
        assert_eq!(value["version"], 1);
        assert_eq!(value["command"], "insert_link");
        assert_eq!(
            value["payload"]["url"],
            "https://example.test/?q=\"quoted\"&line=one\ntwo"
        );
        assert_eq!(
            value["payload"]["label"],
            "label'); window.injected = true; //"
        );
    }

    #[test]
    fn stable_command_payloads_are_serialized_from_concrete_dtos() {
        let command = EditorCommand::BeginAttachmentImport {
            request_id: "request-1".to_string(),
            items: vec![EditorAttachmentImportItem {
                file_name: "report.pdf".to_string(),
                kind: "pdf".to_string(),
                status: None,
                ingest_id: None,
            }],
        };
        let serialized = serialize_editor_command(&command).expect("serialize command");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("parse command");
        assert_eq!(value["payload"]["items"][0]["file_name"], "report.pdf");
        assert!(value["payload"]["items"][0].get("status").is_none());
        assert!(value["payload"]["items"][0].get("ingest_id").is_none());
    }
}

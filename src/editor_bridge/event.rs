use serde::Deserialize;

use crate::domain::NoteId;

pub const EDITOR_BRIDGE_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug)]
pub enum EditorBridgeDecodeError {
    EventEnvelope(serde_json::Error),
    UnsupportedVersion(u16),
    DocumentEvent(serde_json::Error),
    AttachmentEvent(serde_json::Error),
    NoteLinkEvent(serde_json::Error),
    UnsupportedEvent(String),
    DocumentState(serde_json::Error),
}

impl std::fmt::Display for EditorBridgeDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EventEnvelope(error) => {
                write!(formatter, "Could not decode the editor event: {error}")
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "Unsupported editor bridge protocol version: {version}"
            ),
            Self::DocumentEvent(error) => write!(
                formatter,
                "Could not decode the editor document event: {error}"
            ),
            Self::AttachmentEvent(error) => write!(
                formatter,
                "Could not decode the editor attachment event: {error}"
            ),
            Self::NoteLinkEvent(error) => write!(
                formatter,
                "Could not decode the editor note-link event: {error}"
            ),
            Self::UnsupportedEvent(event) => write!(formatter, "Unsupported editor event: {event}"),
            Self::DocumentState(error) => {
                write!(formatter, "Could not decode the editor state: {error}")
            }
        }
    }
}

impl std::error::Error for EditorBridgeDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::EventEnvelope(error)
            | Self::DocumentEvent(error)
            | Self::AttachmentEvent(error)
            | Self::NoteLinkEvent(error)
            | Self::DocumentState(error) => Some(error),
            Self::UnsupportedVersion(_) | Self::UnsupportedEvent(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct EditorDocumentUpdate {
    pub generation: u64,
    pub revision: u64,
    pub editor_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct EditorImageImportPayload {
    pub file_name: String,
    pub mime_type: String,
    pub stage_token: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct EditorImageEditPayload {
    pub request_id: String,
    pub attachment_id: String,
    pub source_attachment_id: String,
    pub file_name: String,
    pub mime_type: String,
    pub stage_token: String,
    pub edit_recipe: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentPickerKind {
    Image,
    Audio,
    Video,
    Pdf,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorAttachmentFileAction {
    Open,
    SaveAs,
    Reveal,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditorAttachmentCommand {
    ImportImages {
        request_id: String,
        images: Vec<EditorImageImportPayload>,
    },
    CommitImageEdit(EditorImageEditPayload),
    PickAttachments {
        request_id: String,
        picker: AttachmentPickerKind,
        editor_update: EditorDocumentUpdate,
    },
    FileAction {
        attachment_id: String,
        action: EditorAttachmentFileAction,
    },
    OpenPendingAttachment {
        ingest_id: String,
    },
    RecoverPendingAttachments {
        generation: u64,
    },
    SavePdfExport {
        request_id: String,
        attachment_id: String,
        export_token: String,
    },
    ProbeMediaCompatibility {
        attachment_id: String,
        profile: String,
    },
    PrepareMediaCompatibility {
        attachment_id: String,
        profile: String,
    },
    InvalidateMediaCompatibility {
        attachment_id: String,
        profile: String,
    },
    ToggleWindowFullscreen,
    Status {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditorNoteLinkCommand {
    PickNoteLink {
        request_id: String,
        initial_query: String,
    },
    OpenNote {
        note_id: NoteId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "event", content = "payload", rename_all = "snake_case")]
pub enum EditorEvent {
    DocumentChanged(EditorDocumentUpdate),
    AttachmentCommand(EditorAttachmentCommand),
    NoteLinkCommand(EditorNoteLinkCommand),
}

#[derive(Deserialize)]
struct EditorEventEnvelope {
    version: u16,
    event: String,
    payload: serde_json::Value,
}

pub fn decode_editor_event(serialized: &str) -> Result<EditorEvent, EditorBridgeDecodeError> {
    let envelope: EditorEventEnvelope =
        serde_json::from_str(serialized).map_err(EditorBridgeDecodeError::EventEnvelope)?;
    if envelope.version != EDITOR_BRIDGE_PROTOCOL_VERSION {
        return Err(EditorBridgeDecodeError::UnsupportedVersion(
            envelope.version,
        ));
    }
    match envelope.event.as_str() {
        "document_changed" => serde_json::from_value(envelope.payload)
            .map(EditorEvent::DocumentChanged)
            .map_err(EditorBridgeDecodeError::DocumentEvent),
        "attachment_command" => serde_json::from_value(envelope.payload)
            .map(EditorEvent::AttachmentCommand)
            .map_err(EditorBridgeDecodeError::AttachmentEvent),
        "note_link_command" => serde_json::from_value(envelope.payload)
            .map(EditorEvent::NoteLinkCommand)
            .map_err(EditorBridgeDecodeError::NoteLinkEvent),
        other => Err(EditorBridgeDecodeError::UnsupportedEvent(other.to_string())),
    }
}

pub fn decode_editor_document_update(
    serialized: &str,
) -> Result<EditorDocumentUpdate, EditorBridgeDecodeError> {
    serde_json::from_str(serialized).map_err(EditorBridgeDecodeError::DocumentState)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_event_bridge_decodes_document_attachment_and_note_link_events() {
        let document = decode_editor_event(
            r#"{"version":1,"event":"document_changed","payload":{"generation":7,"revision":11,"editor_state":"{\"schema_version\":2}"}}"#,
        )
        .expect("decode document event");
        assert!(matches!(
            document,
            EditorEvent::DocumentChanged(EditorDocumentUpdate {
                generation: 7,
                revision: 11,
                ..
            })
        ));

        let attachment = decode_editor_event(
            r#"{"version":1,"event":"attachment_command","payload":{"kind":"file_action","attachment_id":"att-1","action":"save_as"}}"#,
        )
        .expect("decode attachment event");
        assert_eq!(
            attachment,
            EditorEvent::AttachmentCommand(EditorAttachmentCommand::FileAction {
                attachment_id: "att-1".to_string(),
                action: EditorAttachmentFileAction::SaveAs,
            })
        );

        let note_link = decode_editor_event(
            r#"{"version":1,"event":"note_link_command","payload":{"kind":"open_note","note_id":"note-1"}}"#,
        )
        .expect("decode note-link event");
        assert_eq!(
            note_link,
            EditorEvent::NoteLinkCommand(EditorNoteLinkCommand::OpenNote {
                note_id: NoteId::parse("note-1").expect("valid note id"),
            })
        );
    }

    #[test]
    fn versioned_event_bridge_rejects_wrong_versions_unknown_events_and_invalid_payloads() {
        assert!(decode_editor_event(
            r#"{"version":2,"event":"document_changed","payload":{"generation":1,"revision":1,"editor_state":"{}"}}"#
        )
        .is_err());
        assert!(
            decode_editor_event(r#"{"version":1,"event":"future_event","payload":{}}"#).is_err()
        );
        assert!(decode_editor_event(
            r#"{"version":1,"event":"attachment_command","payload":{"kind":"file_action","attachment_id":"att-1"}}"#
        )
        .is_err());
        assert!(decode_editor_event("not json").is_err());
    }
}

use serde::{Deserialize, Serialize};

use super::note::NoteId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub note_id: NoteId,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_pixel_width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_pixel_height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_orientation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_has_alpha: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_animated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_attachment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_edit_recipe: Option<String>,
    pub embedded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreferenced_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}


#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttachmentAudit {
    pub checked: usize,
    pub missing: Vec<String>,
    pub corrupt: Vec<String>,
}

impl AttachmentAudit {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.corrupt.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttachmentRecoveryReport {
    pub replayed_adds: usize,
    pub replayed_removes: usize,
    pub removed_incoming_files: usize,
    pub removed_unreferenced_attachment_records: usize,
    pub quarantined_orphans: usize,
    pub purged_quarantine_files: usize,
    pub missing_attachment_records: usize,
    pub unresolved_operations: usize,
}


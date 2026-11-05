use serde::{Deserialize, Serialize};

use super::{
    attachment::Attachment,
    note::{Note, NoteId, NoteTitleStyle},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevisionRetention {
    pub max_revisions_per_note: i64,
    pub max_age_days: i64,
    pub max_total_bytes: i64,
}

impl Default for RevisionRetention {
    fn default() -> Self {
        Self {
            max_revisions_per_note: 250,
            max_age_days: 365,
            max_total_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRevisionSummary {
    pub id: String,
    pub note_id: NoteId,
    pub revision_no: i64,
    pub byte_size: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRevision {
    pub id: String,
    pub note_id: NoteId,
    pub revision_no: i64,
    pub checksum: String,
    pub snapshot_json: String,
    #[serde(default)]
    pub byte_size: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionAttachment {
    pub id: String,
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
    pub created_at: i64,
}

impl From<Attachment> for RevisionAttachment {
    fn from(attachment: Attachment) -> Self {
        Self {
            id: attachment.id,
            file_name: attachment.file_name,
            mime_type: attachment.mime_type,
            byte_size: attachment.byte_size,
            sha256: attachment.sha256,
            image_pixel_width: attachment.image_pixel_width,
            image_pixel_height: attachment.image_pixel_height,
            image_format: attachment.image_format,
            image_orientation: attachment.image_orientation,
            image_has_alpha: attachment.image_has_alpha,
            image_animated: attachment.image_animated,
            source_attachment_id: attachment.source_attachment_id,
            image_edit_recipe: attachment.image_edit_recipe,
            created_at: attachment.created_at,
        }
    }
}

/// Note-owned state captured by version history.
///
/// This deliberately contains only state that belongs to the note revision itself.
/// Relationships stored independently of the document (for example the `note_parent`
/// hierarchy table, source tables, or citation tables) are outside the revision
/// contract and must remain current when a note revision is restored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionNoteSnapshot {
    pub id: NoteId,
    pub notebook_id: String,
    pub title: String,
    #[serde(default)]
    pub title_style: NoteTitleStyle,
    /// Canonical document state. Markdown and plain text are regenerated on demand.
    pub editor_state: std::sync::Arc<String>,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub archived: bool,
    pub trashed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purge_after: Option<i64>,
    pub created_at: i64,
}

impl RevisionNoteSnapshot {
    pub fn from_note(note: &Note) -> Self {
        Self {
            id: note.id.clone(),
            notebook_id: note.notebook_id.clone(),
            title: note.title.clone(),
            title_style: note.title_style.clone(),
            editor_state: note.document.editor_state.clone(),
            tags: note.tags.clone(),
            pinned: note.pinned,
            archived: note.archived,
            trashed: note.trashed,
            purge_after: note.purge_after,
            created_at: note.created_at,
        }
    }

    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            "Untitled note"
        } else {
            title
        }
    }

    pub fn tags_text(&self) -> String {
        self.tags.join(", ")
    }
}

/// Persisted version-history snapshot.
///
/// The ProseMirror document and versioned attachments are revision-owned. Structural
/// indexes derived from that document (such as internal note links and attachment membership)
/// are rebuilt from the restored document. Independently stored relationships are not
/// part of this snapshot and are intentionally not restored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionSnapshot {
    pub format_version: u32,
    pub note: RevisionNoteSnapshot,
    pub attachments: Vec<RevisionAttachment>,
}

impl RevisionSnapshot {
    pub const FORMAT_VERSION: u32 = 4;
    pub fn new(note: &Note, attachments: Vec<Attachment>) -> Self {
        Self {
            format_version: Self::FORMAT_VERSION,
            note: RevisionNoteSnapshot::from_note(note),
            attachments: attachments
                .into_iter()
                .map(RevisionAttachment::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRevisionRestoreResult {
    pub note: Note,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RevisionStorageUsage {
    pub revision_count: i64,
    pub total_bytes: i64,
}


use serde::{Deserialize, Serialize};

pub type TagId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteIdError;

impl std::fmt::Display for NoteIdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("note identifier is invalid")
    }
}

impl std::error::Error for NoteIdError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteTitleStyle {
    #[serde(default = "default_title_bold")]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(default = "default_title_font_family")]
    pub font_family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u8>,
    #[serde(default = "default_title_text_color")]
    pub text_color: String,
}

const fn default_title_bold() -> bool {
    true
}

fn default_title_font_family() -> String {
    "system".to_string()
}

fn default_title_text_color() -> String {
    "default".to_string()
}

impl Default for NoteTitleStyle {
    fn default() -> Self {
        Self {
            bold: true,
            italic: false,
            underline: false,
            strikethrough: false,
            font_family: default_title_font_family(),
            font_size: None,
            text_color: default_title_text_color(),
        }
    }
}

impl NoteTitleStyle {
    pub fn normalized(mut self) -> Self {
        if !matches!(
            self.font_family.as_str(),
            "system" | "serif" | "rounded" | "mono"
        ) {
            self.font_family = default_title_font_family();
        }
        if let Some(size) = self.font_size {
            self.font_size = Some(size.clamp(8, 72));
        }
        if !matches!(
            self.text_color.as_str(),
            "default" | "grey" | "red" | "orange" | "green" | "blue" | "purple"
        ) {
            self.text_color = default_title_text_color();
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct NoteId(String);

impl NoteId {
    pub const MAX_LEN: usize = 256;

    pub fn parse(value: impl Into<String>) -> Result<Self, NoteIdError> {
        let value = value.into();
        if value.is_empty() || value.len() > Self::MAX_LEN || value.chars().any(char::is_control) {
            return Err(NoteIdError);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NoteId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for NoteId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::borrow::Borrow<str> for NoteId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for NoteId {
    type Error = NoteIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for NoteId {
    type Error = NoteIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for NoteId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(<D::Error as serde::de::Error>::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteCandidate {
    pub id: NoteId,
    pub title: String,
    pub notebook_name: String,
    pub archived: bool,
}

impl NoteCandidate {
    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            "Untitled note"
        } else {
            title
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteLinkTargetState {
    Active,
    Archived,
    Trashed,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NoteLinkTarget {
    pub note_id: NoteId,
    pub title: Option<String>,
    pub state: NoteLinkTargetState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacklinkSummary {
    pub source_note_id: NoteId,
    pub title: String,
    pub notebook_name: String,
    pub archived: bool,
    pub trashed: bool,
}

impl BacklinkSummary {
    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            "Untitled note"
        } else {
            title
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HierarchyNoteSummary {
    pub id: NoteId,
    pub title: String,
    pub notebook_name: String,
    pub archived: bool,
    pub trashed: bool,
}

impl HierarchyNoteSummary {
    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            "Untitled note"
        } else {
            title
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteHierarchy {
    pub parent: Option<HierarchyNoteSummary>,
    pub children: Vec<HierarchyNoteSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotePropertiesContext {
    pub attachment_count: usize,
    pub outgoing_link_count: usize,
    pub backlink_count: usize,
    pub parent: Option<HierarchyNoteSummary>,
    pub child_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteDocument {
    /// Derived Markdown projection retained for portability and export.
    pub body: String,
    /// Canonical versioned ProseMirror document JSON. Shared separately so
    /// durability/history snapshots can retain it without copying large JSON.
    pub editor_state: std::sync::Arc<String>,
    /// Derived plain-text projection used for search, previews, and word counts.
    pub plain_text: String,
}

impl NoteDocument {
    pub fn new(body: String, editor_state: std::sync::Arc<String>, plain_text: String) -> Self {
        Self {
            body,
            editor_state,
            plain_text,
        }
    }
}

impl Default for NoteDocument {
    fn default() -> Self {
        Self {
            body: String::new(),
            editor_state: std::sync::Arc::new(
                crate::editor_document::EMPTY_EDITOR_STATE
                    .trim()
                    .to_string(),
            ),
            plain_text: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteProtectionState {
    #[default]
    Unprotected,
    Locked,
    Unlocked,
}

impl NoteProtectionState {
    pub const fn is_protected(self) -> bool {
        !matches!(self, Self::Unprotected)
    }

    pub const fn is_locked(self) -> bool {
        matches!(self, Self::Locked)
    }

    pub const fn is_unlocked(self) -> bool {
        matches!(self, Self::Unlocked)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    #[serde(skip)]
    pub protection: NoteProtectionState,
    pub notebook_id: String,
    pub title: String,
    #[serde(default)]
    pub title_style: NoteTitleStyle,
    #[serde(flatten)]
    pub document: std::sync::Arc<NoteDocument>,
    pub tags: Vec<String>,
    #[serde(skip)]
    pub attachment_names: Vec<String>,
    pub pinned: bool,
    pub archived: bool,
    pub trashed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purge_after: Option<i64>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub updated_at: i64,
}

impl PartialEq for Note {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.protection == other.protection
            && self.notebook_id == other.notebook_id
            && self.title == other.title
            && self.title_style == other.title_style
            && (std::sync::Arc::ptr_eq(&self.document, &other.document)
                || self.document == other.document)
            && self.tags == other.tags
            && self.attachment_names == other.attachment_names
            && self.pinned == other.pinned
            && self.archived == other.archived
            && self.trashed == other.trashed
            && self.purge_after == other.purge_after
            && self.created_at == other.created_at
            && self.updated_at == other.updated_at
    }
}

impl Eq for Note {}

impl Note {
    pub fn body(&self) -> &str {
        &self.document.body
    }

    pub fn editor_state(&self) -> &str {
        self.document.editor_state.as_str()
    }

    pub fn plain_text(&self) -> &str {
        &self.document.plain_text
    }

    pub fn replace_document(&mut self, body: String, editor_state: String, plain_text: String) {
        self.document = std::sync::Arc::new(NoteDocument::new(
            body,
            std::sync::Arc::new(editor_state),
            plain_text,
        ));
    }
    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if self.protection.is_locked() {
            return if title.is_empty() {
                "Locked note"
            } else {
                title
            };
        }
        if title.is_empty() {
            "Untitled note"
        } else {
            title
        }
    }

    pub fn searchable_body(&self) -> &str {
        self.plain_text()
    }

    pub fn preview(&self) -> String {
        if self.protection.is_locked() {
            return "Encrypted note".to_string();
        }
        let plain = self.searchable_body();
        let compact = plain.split_whitespace().collect::<Vec<_>>().join(" ");
        if compact.is_empty() {
            return "No additional text".to_string();
        }
        let preview = compact.chars().take(150).collect::<String>();
        if compact.chars().count() > 150 {
            format!("{preview}…")
        } else {
            preview
        }
    }

    pub fn tags_text(&self) -> String {
        self.tags.join(", ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: NoteId,
    pub protection: NoteProtectionState,
    pub notebook_id: String,
    pub title: String,
    pub preview_text: String,
    pub tags: Vec<String>,
    pub attachment_names: Vec<String>,
    pub pinned: bool,
    pub archived: bool,
    pub trashed: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl NoteSummary {
    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if self.protection.is_locked() {
            return if title.is_empty() {
                "Locked note"
            } else {
                title
            };
        }
        if title.is_empty() {
            "Untitled note"
        } else {
            title
        }
    }

    pub fn preview(&self) -> String {
        if self.protection.is_locked() {
            return "Encrypted note".to_string();
        }
        let compact = self
            .preview_text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if compact.is_empty() {
            return "No additional text".to_string();
        }
        let preview = compact.chars().take(150).collect::<String>();
        if compact.chars().count() > 150 {
            format!("{preview}…")
        } else {
            preview
        }
    }
}

impl From<&Note> for NoteSummary {
    fn from(note: &Note) -> Self {
        Self {
            id: note.id.clone(),
            protection: note.protection,
            notebook_id: note.notebook_id.clone(),
            title: note.title.clone(),
            preview_text: note.plain_text().chars().take(600).collect(),
            tags: note.tags.clone(),
            attachment_names: note.attachment_names.clone(),
            pinned: note.pinned,
            archived: note.archived,
            trashed: note.trashed,
            created_at: note.created_at,
            updated_at: note.updated_at,
        }
    }
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

#[cfg(test)]
mod b6_typed_id_tests {
    use super::*;

    #[test]
    fn identifier_parse_preserves_typed_validation_error() {
        assert!(matches!(NoteId::parse(""), Err(NoteIdError)));
        assert!(matches!(NoteId::parse("bad\nidentifier"), Err(NoteIdError)));
        assert!(NoteId::parse("valid-id").is_ok());
    }

    #[test]
    fn title_style_normalization_preserves_safe_defaults() {
        let normalized = NoteTitleStyle {
            bold: false,
            italic: true,
            underline: true,
            strikethrough: false,
            font_family: "unsupported".to_string(),
            font_size: Some(200),
            text_color: "unsupported".to_string(),
        }
        .normalized();

        assert!(!normalized.bold);
        assert!(normalized.italic);
        assert_eq!(normalized.font_family, "system");
        assert_eq!(normalized.font_size, Some(72));
        assert_eq!(normalized.text_color, "default");
    }
}

//! Portable editable-format codecs.
//!
//! File parsing/serialization is deliberately independent of ProseMirror and the
//! system WebView. Importers produce a normalized document tree which is resolved
//! to the canonical Rustnote document before storage accepts it.

use std::path::{Path, PathBuf};

use serde_json::Value;

mod document;
mod html;
mod markdown;
mod resource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortabilityCodecError {
    message: String,
}

impl PortabilityCodecError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PortabilityCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PortabilityCodecError {}

impl From<String> for PortabilityCodecError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

pub(crate) type PortabilityResult<T> = Result<T, PortabilityCodecError>;

pub(crate) use document::{
    ImportedDocument, ResourceResolution, canonical_editor_state, remap_canonical_editor_state,
    resolve_import_placeholders,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportPlan {
    pub(crate) source: PathBuf,
    pub(crate) inspection: ImportInspection,
    pub(crate) payload: ImportPlanPayload,
}

impl ImportPlan {
    pub(crate) fn inspection(&self) -> &ImportInspection {
        &self.inspection
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImportPlanPayload {
    Editable {
        root: PathBuf,
        files: Vec<PlannedImportFile>,
    },
    RustnoteBundle {
        semantic_sha256: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlannedImportFile {
    pub(crate) path: PathBuf,
    pub(crate) relative_path: PathBuf,
    pub(crate) format: PortabilityFormat,
    pub(crate) byte_size: u64,
    pub(crate) sha256: String,
    pub(crate) resources: Vec<PlannedImportResource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlannedImportResource {
    pub(crate) source_path: PathBuf,
    pub(crate) byte_size: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PortabilityCapabilities {
    pub(crate) rich_text: bool,
    pub(crate) attachments: bool,
    pub(crate) internal_note_links: bool,
    pub(crate) paragraph_layout: bool,
    pub(crate) rustnote_metadata: bool,
    pub(crate) revision_history: bool,
    pub(crate) lossless: bool,
}

impl PortabilityCapabilities {
    pub(crate) const fn description(self) -> &'static str {
        if self.lossless && self.rustnote_metadata && self.revision_history {
            "Lossless Rustnote transfer with attachments, hierarchy, sources, annotations, and version history."
        } else if self.rich_text
            && self.attachments
            && self.internal_note_links
            && self.paragraph_layout
        {
            "Semantic rich text with safe local attachment assets, relative note links, and supported paragraph layout."
        } else if self.rich_text && self.attachments && self.internal_note_links {
            "Editable rich text with safe local attachment assets and relative note links; Rustnote-only paragraph layout is omitted."
        } else if self.rich_text && self.paragraph_layout {
            "Portable rich text with common paragraph layout; attachments and Rustnote-specific metadata are omitted."
        } else {
            "Simple text projection; formatting, attachments, links, and Rustnote-specific metadata are intentionally omitted."
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PortabilityScope {
    Note(crate::domain::NoteId),
    Notebook(String),
    Library,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportInspection {
    pub(crate) files: usize,
    pub(crate) total_bytes: u64,
    pub(crate) plain_text: usize,
    pub(crate) markdown: usize,
    pub(crate) html: usize,
    pub(crate) rustnote_bundles: usize,
    pub(crate) notes: usize,
    pub(crate) notebooks: usize,
    pub(crate) attachments: usize,
    pub(crate) revisions: usize,
    pub(crate) sources: usize,
    pub(crate) citations: usize,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportUndoToken {
    pub(crate) note_ids: Vec<crate::domain::NoteId>,
    pub(crate) notebook_ids: Vec<String>,
    pub(crate) blob_hashes: Vec<String>,
}

impl ImportUndoToken {
    pub(crate) fn is_empty(&self) -> bool {
        self.note_ids.is_empty() && self.notebook_ids.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportSummary {
    pub(crate) notes: usize,
    pub(crate) notebooks: usize,
    pub(crate) attachments: usize,
    pub(crate) warnings: Vec<String>,
    pub(crate) undo: Option<ImportUndoToken>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortabilityFormat {
    PlainText,
    Markdown,
    Html,
    RustnoteBundle,
}

impl PortabilityFormat {
    pub(crate) const ALL: [Self; 4] = [
        Self::PlainText,
        Self::Markdown,
        Self::Html,
        Self::RustnoteBundle,
    ];

    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::PlainText => "txt",
            Self::Markdown => "md",
            Self::Html => "html",
            Self::RustnoteBundle => "rustnote",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::PlainText => "Plain text",
            Self::Markdown => "Markdown",
            Self::Html => "HTML",
            Self::RustnoteBundle => "Rustnote bundle",
        }
    }

    pub(crate) const fn capabilities(self) -> PortabilityCapabilities {
        match self {
            Self::PlainText => PortabilityCapabilities {
                rich_text: false,
                attachments: false,
                internal_note_links: false,
                paragraph_layout: false,
                rustnote_metadata: false,
                revision_history: false,
                lossless: false,
            },
            Self::Markdown => PortabilityCapabilities {
                rich_text: true,
                attachments: true,
                internal_note_links: true,
                paragraph_layout: false,
                rustnote_metadata: false,
                revision_history: false,
                lossless: false,
            },
            Self::Html => PortabilityCapabilities {
                rich_text: true,
                attachments: true,
                internal_note_links: true,
                paragraph_layout: true,
                rustnote_metadata: false,
                revision_history: false,
                lossless: false,
            },
            Self::RustnoteBundle => PortabilityCapabilities {
                rich_text: true,
                attachments: true,
                internal_note_links: true,
                paragraph_layout: true,
                rustnote_metadata: true,
                revision_history: true,
                lossless: true,
            },
        }
    }

    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "txt" => Some(Self::PlainText),
            "md" | "markdown" => Some(Self::Markdown),
            "html" | "htm" => Some(Self::Html),
            "rustnote" => Some(Self::RustnoteBundle),
            _ => None,
        }
    }
}

pub(crate) fn decode_editable(
    format: PortabilityFormat,
    bytes: &[u8],
) -> PortabilityResult<ImportedDocument> {
    match format {
        PortabilityFormat::PlainText => {
            let text = decode_text(bytes)?;
            Ok(document::plain_text_document(&text))
        }
        PortabilityFormat::Markdown => {
            let text = decode_text(bytes)?;
            markdown::decode(&text)
        }
        PortabilityFormat::Html => {
            let text = decode_text(bytes)?;
            html::decode(&text)
        }
        PortabilityFormat::RustnoteBundle => Err(PortabilityCodecError::new(
            "Rustnote bundles use the native bundle importer",
        )),
    }
}

pub(crate) fn encode_editable(
    format: PortabilityFormat,
    title: &str,
    editor_state: &str,
    attachment_paths: &std::collections::HashMap<String, String>,
    note_paths: &std::collections::HashMap<String, String>,
) -> PortabilityResult<Vec<u8>> {
    match format {
        PortabilityFormat::PlainText => {
            let projection = crate::editor_projection::derive_editor_projections(editor_state)
                .map_err(|error| error.to_string())?;
            let mut output = String::new();
            output.push_str(projection.plain_text.trim_end());
            output.push('\n');
            Ok(output.into_bytes())
        }
        PortabilityFormat::Markdown => {
            markdown::encode(title, editor_state, attachment_paths, note_paths)
                .map(String::into_bytes)
        }
        PortabilityFormat::Html => {
            html::encode(title, editor_state, attachment_paths, note_paths).map(String::into_bytes)
        }
        PortabilityFormat::RustnoteBundle => Err(PortabilityCodecError::new(
            "Rustnote bundles use the native bundle exporter",
        )),
    }
}

pub(crate) use resource::{
    is_asset_namespace_name, is_asset_namespace_path,
    safe_relative_path as safe_relative_resource_path,
};

fn decode_text(bytes: &[u8]) -> PortabilityResult<String> {
    if bytes.starts_with(&[0xff, 0xfe]) {
        let mut units = Vec::with_capacity((bytes.len().saturating_sub(2)) / 2);
        for pair in bytes[2..].chunks_exact(2) {
            units.push(u16::from_le_bytes([pair[0], pair[1]]));
        }
        return String::from_utf16(&units)
            .map_err(|_| PortabilityCodecError::new("text file contains invalid UTF-16"));
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        let mut units = Vec::with_capacity((bytes.len().saturating_sub(2)) / 2);
        for pair in bytes[2..].chunks_exact(2) {
            units.push(u16::from_be_bytes([pair[0], pair[1]]));
        }
        return String::from_utf16(&units)
            .map_err(|_| PortabilityCodecError::new("text file contains invalid UTF-16"));
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec())
        .map_err(|_| PortabilityCodecError::new("text file is not valid UTF-8 or UTF-16"))
}

pub(crate) fn rewrite_note_link_placeholders(
    document: &mut Value,
    resolve: &impl Fn(&str) -> Option<crate::domain::NoteId>,
) {
    document::rewrite_note_link_placeholders(document, resolve);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_format_detection_is_narrow() {
        assert_eq!(
            PortabilityFormat::from_path(Path::new("note.MD")),
            Some(PortabilityFormat::Markdown)
        );
        assert_eq!(PortabilityFormat::from_path(Path::new("note.docx")), None);
    }

    #[test]
    fn resource_paths_cannot_escape_import_root() {
        assert_eq!(
            safe_relative_resource_path("assets/image.png"),
            Some(PathBuf::from("assets/image.png"))
        );
        assert_eq!(safe_relative_resource_path("../secret.txt"), None);
        assert_eq!(safe_relative_resource_path("/tmp/secret.txt"), None);
        assert_eq!(
            safe_relative_resource_path("https://example.com/a.png"),
            None
        );
    }

    #[test]
    fn format_capabilities_are_declarative_and_match_fidelity_contract() {
        let text = PortabilityFormat::PlainText.capabilities();
        assert!(!text.rich_text && !text.attachments && !text.lossless);

        let markdown = PortabilityFormat::Markdown.capabilities();
        assert!(markdown.rich_text && markdown.attachments && markdown.internal_note_links);
        assert!(!markdown.paragraph_layout && !markdown.lossless);

        let html = PortabilityFormat::Html.capabilities();
        assert!(
            html.rich_text && html.attachments && html.internal_note_links && html.paragraph_layout
        );
        assert!(!html.lossless);

        let native = PortabilityFormat::RustnoteBundle.capabilities();
        assert!(
            native.lossless
                && native.rustnote_metadata
                && native.revision_history
                && native.attachments
        );
    }
}

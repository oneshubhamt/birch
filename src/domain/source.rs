use serde::{Deserialize, Serialize};

use super::note::NoteId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceIdError;

impl std::fmt::Display for SourceIdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("source identifier is invalid")
    }
}

impl std::error::Error for SourceIdError {}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SourceId(String);

impl SourceId {
    pub const MAX_LEN: usize = 256;

    pub fn parse(value: impl Into<String>) -> Result<Self, SourceIdError> {
        let value = value.into();
        if value.is_empty() || value.len() > Self::MAX_LEN || value.chars().any(char::is_control) {
            return Err(SourceIdError);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for SourceId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::borrow::Borrow<str> for SourceId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for SourceId {
    type Error = SourceIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for SourceId {
    type Error = SourceIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for SourceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(<D::Error as serde::de::Error>::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Web,
    Book,
    Journal,
    Pdf,
    Report,
    Other,
}

impl SourceKind {
    pub fn value(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Book => "book",
            Self::Journal => "journal",
            Self::Pdf => "pdf",
            Self::Report => "report",
            Self::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Web => "Web",
            Self::Book => "Book",
            Self::Journal => "Journal",
            Self::Pdf => "PDF",
            Self::Report => "Report",
            Self::Other => "Other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "web" => Some(Self::Web),
            "book" => Some(Self::Book),
            "journal" => Some(Self::Journal),
            "pdf" => Some(Self::Pdf),
            "report" => Some(Self::Report),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    pub const ALL: [Self; 6] = [
        Self::Web,
        Self::Book,
        Self::Journal,
        Self::Pdf,
        Self::Report,
        Self::Other,
    ];
}

/// Reusable bibliographic/source entity. It is deliberately independent of note
/// attachments; a later schema can add optional attachment backing (for PDFs)
/// without changing note-source or citation identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    pub kind: SourceKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Source {
    pub fn display_title(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            "Untitled source"
        } else {
            title
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub id: String,
    pub note_id: NoteId,
    pub source_id: SourceId,
    pub locator: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSourceSummary {
    pub source: Source,
    pub citations: Vec<Citation>,
}

#[cfg(test)]
mod b6_typed_id_tests {
    use super::*;

    #[test]
    fn identifier_parse_preserves_typed_validation_error() {
        assert!(matches!(SourceId::parse(""), Err(SourceIdError)));
        assert!(matches!(
            SourceId::parse("bad\nidentifier"),
            Err(SourceIdError)
        ));
        assert!(SourceId::parse("valid-id").is_ok());
    }
}

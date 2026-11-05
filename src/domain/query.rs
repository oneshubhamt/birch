use serde::{Deserialize, Serialize};

use super::note::{NoteId, NoteSummary, TagId};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteFilters {
    #[serde(default)]
    pub tags: Vec<TagId>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub with_attachments: bool,
    #[serde(default)]
    pub without_tags: bool,
}

impl NoteFilters {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && !self.pinned && !self.with_attachments && !self.without_tags
    }

    pub fn active_count(&self) -> usize {
        self.tags.len()
            + usize::from(self.pinned)
            + usize::from(self.with_attachments)
            + usize::from(self.without_tags)
    }


    pub fn from_storage(value: &str) -> Self {
        serde_json::from_str::<Self>(value)
            .unwrap_or_default()
            .normalized()
    }

    pub fn to_storage(&self) -> String {
        serde_json::to_string(&self.clone().normalized()).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn normalized(mut self) -> Self {
        self.tags.sort_by_key(|tag| tag.to_lowercase());
        self.tags
            .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        if !self.tags.is_empty() {
            self.without_tags = false;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Notes,
    Pinned,
    Archive,
    Trash,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Notes => "All notes",
            Self::Pinned => "Pinned",
            Self::Archive => "Archive",
            Self::Trash => "Trash",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteSort {
    #[default]
    UpdatedNewest,
    UpdatedOldest,
    CreatedNewest,
    TitleAscending,
    TitleDescending,
}

impl NoteSort {
    pub fn value(self) -> &'static str {
        match self {
            Self::UpdatedNewest => "updated-newest",
            Self::UpdatedOldest => "updated-oldest",
            Self::CreatedNewest => "created-newest",
            Self::TitleAscending => "title-ascending",
            Self::TitleDescending => "title-descending",
        }
    }

    pub fn from_value(value: &str) -> Self {
        match value {
            "updated-oldest" => Self::UpdatedOldest,
            "created-newest" => Self::CreatedNewest,
            "title-ascending" => Self::TitleAscending,
            "title-descending" => Self::TitleDescending,
            _ => Self::UpdatedNewest,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::UpdatedNewest => "Updated: newest",
            Self::UpdatedOldest => "Updated: oldest",
            Self::CreatedNewest => "Created: newest",
            Self::TitleAscending => "Title: A–Z",
            Self::TitleDescending => "Title: Z–A",
        }
    }
}

pub const NOTE_LIST_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteListQuery {
    pub section: Section,
    pub notebook_id: Option<String>,
    pub search: String,
    pub sort: NoteSort,
    pub filters: NoteFilters,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteListCursor {
    pub pinned: bool,
    pub updated_at: i64,
    pub created_at: i64,
    pub title_key: String,
    pub id: NoteId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteListPage {
    pub notes: Vec<NoteSummary>,
    pub next_cursor: Option<NoteListCursor>,
    pub total_count: Option<usize>,
}


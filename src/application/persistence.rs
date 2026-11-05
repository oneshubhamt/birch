//! Application-facing persistence intent used by UI adapters.
//!
//! Storage save scopes remain an internal optimization detail. UI code reports
//! which logical part of a note changed; application services translate that
//! intent to the persistence representation.

use crate::storage::NoteSaveScope;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoteChangeScope {
    None,
    Title,
    Tags,
    State,
    Document,
}

impl NoteChangeScope {
    pub(super) const fn storage_scope(self) -> NoteSaveScope {
        match self {
            Self::None => NoteSaveScope::NONE,
            Self::Title => NoteSaveScope::TITLE,
            Self::Tags => NoteSaveScope::TAGS,
            Self::State => NoteSaveScope::STATE,
            Self::Document => NoteSaveScope::DOCUMENT,
        }
    }
}

pub(crate) fn unix_timestamp() -> i64 {
    crate::storage::unix_timestamp()
}

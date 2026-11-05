//! Canonical UI command services shared by Rust-owned surfaces.
//!
//! Context menus decide which actions are visible. Application-level attachment file
//! commands live in `application::attachments`; this module retains UI-owned tag commands.

use crate::domain::NoteId;
use crate::application::{
    executor::{ApplicationExecutionError, ExecutionFailure},
    notes::NoteService,
};
use crate::application::errors::DataAccessError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TagCommand {
    Rename { from: String, to: String },
    RemoveFromNote { name: String },
    Delete { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TagCommandResult {
    pub affected_notes: usize,
    pub current_tags: Vec<String>,
}

#[derive(Debug)]
pub(super) enum TagCommandError {
    Data(DataAccessError),
    NoteMissing,
    Execution(ApplicationExecutionError),
}

impl std::fmt::Display for TagCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Data(error) => std::fmt::Display::fmt(error, formatter),
            Self::NoteMissing => formatter.write_str("note no longer exists"),
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for TagCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Data(error) => Some(error),
            Self::Execution(error) => Some(error),
            Self::NoteMissing => None,
        }
    }
}

impl From<DataAccessError> for TagCommandError {
    fn from(error: DataAccessError) -> Self { Self::Data(error) }
}

impl ExecutionFailure for TagCommandError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self { Self::Execution(error) }
}

pub(super) fn execute_tag_command_blocking(
    notes: &NoteService,
    note_id: &NoteId,
    command: &TagCommand,
) -> Result<TagCommandResult, TagCommandError> {
    let affected_notes = match command {
        TagCommand::Rename { from, to } => notes
            .rename_tag(from, to)
            ?,
        TagCommand::RemoveFromNote { name } => usize::from(
            notes
                .remove_tag_from_note(note_id, name)
                ?,
        ),
        TagCommand::Delete { name } => notes
            .delete_tag(name)
            ?,
    };
    let current_tags = notes
        .get(note_id)
        ?
        .ok_or(TagCommandError::NoteMissing)?
        .tags;
    Ok(TagCommandResult {
        affected_notes,
        current_tags,
    })
}

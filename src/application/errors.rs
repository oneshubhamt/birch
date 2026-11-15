use std::fmt::{self, Display, Formatter};

use crate::media_compat::MediaCompatibilityError;
use crate::native::{NativeError, staging::PdfExportStagingError};
use crate::storage::StorageError;
use crate::{
    application::executor::{ApplicationExecutionError, ExecutionFailure},
    autosave::AutosaveError,
    editor_document::DocumentValidationError,
};

#[derive(Debug)]
pub(crate) enum DataAccessError {
    Storage(StorageError),
}

impl Display for DataAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for DataAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
        }
    }
}

impl From<StorageError> for DataAccessError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

pub(crate) type DataResult<T> = Result<T, DataAccessError>;

#[derive(Debug)]
pub(crate) enum EditorError {
    Storage(StorageError),
    Autosave(AutosaveError),
    InvalidDocument(DocumentValidationError),
    Execution(ApplicationExecutionError),
    PersistenceListenerRestarted,
}

impl Display for EditorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => Display::fmt(error, formatter),
            Self::Autosave(error) => Display::fmt(error, formatter),
            Self::InvalidDocument(error) => Display::fmt(error, formatter),
            Self::Execution(error) => Display::fmt(error, formatter),
            Self::PersistenceListenerRestarted => {
                formatter.write_str("autosave event listener was unexpectedly restarted")
            }
        }
    }
}

impl std::error::Error for EditorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Autosave(error) => Some(error),
            Self::InvalidDocument(error) => Some(error),
            Self::Execution(error) => Some(error),
            Self::PersistenceListenerRestarted => None,
        }
    }
}

impl From<StorageError> for EditorError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl ExecutionFailure for EditorError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Debug)]
pub(crate) enum AttachmentError {
    Storage(StorageError),
    InvalidRequest(String),
    Autosave(AutosaveError),
    Unavailable(String),
    Native(NativeError),
    MediaCompatibility(MediaCompatibilityError),
    Execution(ApplicationExecutionError),
}

impl Display for AttachmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => Display::fmt(error, formatter),
            Self::Autosave(error) => Display::fmt(error, formatter),
            Self::Native(error) => Display::fmt(error, formatter),
            Self::MediaCompatibility(error) => Display::fmt(error, formatter),
            Self::InvalidRequest(message) | Self::Unavailable(message) => {
                formatter.write_str(message)
            }
            Self::Execution(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for AttachmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Autosave(error) => Some(error),
            Self::Native(error) => Some(error),
            Self::MediaCompatibility(error) => Some(error),
            Self::Execution(error) => Some(error),
            Self::InvalidRequest(_) | Self::Unavailable(_) => None,
        }
    }
}

impl From<StorageError> for AttachmentError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl ExecutionFailure for AttachmentError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Debug)]
pub(crate) enum ExportError {
    Storage(StorageError),
    PdfStaging(PdfExportStagingError),
    Execution(ApplicationExecutionError),
}

impl Display for ExportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => Display::fmt(error, formatter),
            Self::PdfStaging(error) => Display::fmt(error, formatter),
            Self::Execution(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::PdfStaging(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<StorageError> for ExportError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<PdfExportStagingError> for ExportError {
    fn from(error: PdfExportStagingError) -> Self {
        Self::PdfStaging(error)
    }
}

impl ExecutionFailure for ExportError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Debug)]
pub(crate) enum PortabilityError {
    Storage(StorageError),
    Execution(ApplicationExecutionError),
}

impl Display for PortabilityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => Display::fmt(error, formatter),
            Self::Execution(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for PortabilityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<StorageError> for PortabilityError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl ExecutionFailure for PortabilityError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Debug)]
pub(crate) enum EditorWorkflowError {
    Attachment(AttachmentError),
    Editor(EditorError),
    Export(ExportError),
    Execution(ApplicationExecutionError),
}

impl Display for EditorWorkflowError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Attachment(error) => Display::fmt(error, formatter),
            Self::Editor(error) => Display::fmt(error, formatter),
            Self::Export(error) => Display::fmt(error, formatter),
            Self::Execution(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for EditorWorkflowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Attachment(error) => Some(error),
            Self::Editor(error) => Some(error),
            Self::Export(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<AttachmentError> for EditorWorkflowError {
    fn from(error: AttachmentError) -> Self {
        Self::Attachment(error)
    }
}

impl From<EditorError> for EditorWorkflowError {
    fn from(error: EditorError) -> Self {
        Self::Editor(error)
    }
}

impl From<ExportError> for EditorWorkflowError {
    fn from(error: ExportError) -> Self {
        Self::Export(error)
    }
}

impl ExecutionFailure for EditorWorkflowError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_errors_preserve_error_categories() {
        assert!(matches!(
            DataAccessError::from(StorageError::Validation("bad data".to_string())),
            DataAccessError::Storage(StorageError::Validation(_))
        ));
        assert!(matches!(
            EditorError::Autosave(AutosaveError::Operation("save failed".to_string())),
            EditorError::Autosave(_)
        ));
        assert!(matches!(
            EditorError::InvalidDocument(DocumentValidationError::InvalidDocument(
                "bad document".to_string()
            )),
            EditorError::InvalidDocument(_)
        ));
        assert!(matches!(
            AttachmentError::InvalidRequest("bad request".to_string()),
            AttachmentError::InvalidRequest(_)
        ));
        assert!(matches!(
            ExportError::from(PdfExportStagingError::InvalidToken),
            ExportError::PdfStaging(PdfExportStagingError::InvalidToken)
        ));
        assert!(matches!(
            EditorWorkflowError::Attachment(AttachmentError::InvalidRequest("bad".to_string())),
            EditorWorkflowError::Attachment(_)
        ));
    }
}

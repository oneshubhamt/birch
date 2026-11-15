//! Typed adapter for UI-owned background tasks that call application data capabilities.
//!
//! Application data services cannot depend on the UI executor, so this small boundary type
//! preserves `DataAccessError` while still representing executor/runtime failures.

use crate::application::{
    errors::DataAccessError,
    executor::{ApplicationExecutionError, ExecutionFailure},
};

#[derive(Debug)]
pub(super) enum DataTaskError {
    Data(DataAccessError),
    Execution(ApplicationExecutionError),
}

impl std::fmt::Display for DataTaskError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Data(error) => std::fmt::Display::fmt(error, formatter),
            Self::Execution(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for DataTaskError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Data(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<DataAccessError> for DataTaskError {
    fn from(error: DataAccessError) -> Self {
        Self::Data(error)
    }
}

impl ExecutionFailure for DataTaskError {
    fn from_execution_error(error: ApplicationExecutionError) -> Self {
        Self::Execution(error)
    }
}

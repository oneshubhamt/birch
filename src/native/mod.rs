use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum NativeError {
    Platform(crate::platform::PlatformError),
    Operation(String),
    Context {
        context: String,
        source: Box<NativeError>,
    },
}

impl NativeError {
    pub(super) fn operation(message: impl Into<String>) -> Self {
        Self::Operation(message.into())
    }

    pub fn context(self, context: impl Into<String>) -> Self {
        Self::Context {
            context: context.into(),
            source: Box::new(self),
        }
    }
}

impl std::fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform(error) => std::fmt::Display::fmt(error, formatter),
            Self::Operation(message) => formatter.write_str(message),
            Self::Context { context, source } => write!(formatter, "{context}: {source}"),
        }
    }
}

impl std::error::Error for NativeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Platform(error) => Some(error),
            Self::Context { source, .. } => Some(source.as_ref()),
            Self::Operation(_) => None,
        }
    }
}

impl From<crate::platform::PlatformError> for NativeError {
    fn from(error: crate::platform::PlatformError) -> Self {
        Self::Platform(error)
    }
}

impl From<String> for NativeError {
    fn from(message: String) -> Self {
        Self::Operation(message)
    }
}

pub type NativeResult<T> = Result<T, NativeError>;

fn native_error<T>(message: impl Into<String>) -> NativeResult<T> {
    Err(NativeError::operation(message))
}

pub(crate) mod privacy;
pub(crate) mod private_temp;
pub(crate) mod temp_owner;

pub mod handoff;
pub mod output;
pub mod rendition;
pub mod staging;

fn output_token() -> String {
    rand::random::<[u8; 16]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn copy_file_atomically(source: &Path, destination: &Path, label: &str) -> NativeResult<()> {
    output::copy_file_atomically(source, destination).map_err(|error| {
        NativeError::operation(format!("Could not save {label}: {error}"))
    })
}

fn existing_path(path: &Path) -> NativeResult<PathBuf> {
    if !path.is_file() {
        return native_error(format!("File is missing: {}", path.display()));
    }
    path.canonicalize().map_err(|error| {
        NativeError::operation(format!("Could not resolve {}: {error}", path.display()))
    })
}

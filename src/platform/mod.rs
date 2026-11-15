//! Narrow operating-system capability layer.
//!
//! Rustnote's attachment/storage policy stays platform-neutral. This module owns only
//! OS primitives whose implementation genuinely differs across desktop/mobile targets.

mod file_clone;
mod handoff;
mod image;
mod mobile;
// Temporary staged secure-store boundary; consumed by the next backup-storage pass.
#[expect(
    dead_code,
    reason = "staged secure-store boundary awaits encrypted backup repository integration"
)]
mod secure_key_store;

use std::{fs, io, path::Path};

#[cfg(any(target_os = "android", target_os = "ios"))]
use std::path::PathBuf;

pub use mobile::{MobilePlatformAdapter, register_mobile_platform_adapter};
pub(crate) use secure_key_store::{
    BackupProtectorKind, BackupWrapAlgorithm, DeviceBackupKeyProtector,
    system_device_backup_key_protector,
};
pub use secure_key_store::{
    DeviceBackupRootCreation, DeviceBackupRootStatus, SecureKeyMaterial, SecureKeyStoreError,
    SecureKeyStoreResult,
};

#[derive(Debug)]
pub enum PlatformError {
    Operation(String),
    Unsupported(&'static str),
}

impl PlatformError {
    pub(super) fn operation(message: impl Into<String>) -> Self {
        Self::Operation(message.into())
    }
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation(message) => formatter.write_str(message),
            Self::Unsupported(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for PlatformError {}

impl From<String> for PlatformError {
    fn from(message: String) -> Self {
        Self::Operation(message)
    }
}

pub type PlatformResult<T> = Result<T, PlatformError>;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn ensure_mobile_platform_ready() -> PlatformResult<()> {
    let adapter = mobile::adapter().ok_or_else(|| {
        "Rustnote's mobile platform adapter must be registered before storage opens".to_string()
    })?;
    let support = adapter.application_support_directory();
    if support.as_os_str().is_empty() || !support.is_absolute() {
        return Err(PlatformError::operation(
            "Rustnote's mobile application-support directory must be an absolute path",
        ));
    }
    if let Some(documents) = adapter.documents_directory()
        && (documents.as_os_str().is_empty() || !documents.is_absolute())
    {
        return Err(PlatformError::operation(
            "Rustnote's mobile documents directory must be an absolute path",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlatformImageProperties {
    pub dimensions: Option<(usize, usize)>,
    pub has_alpha: Option<bool>,
}

/// Best-effort copy-on-write clone. Returns `Ok(true)` only when `destination`
/// contains a complete private CoW view of `source`. Unsupported filesystems and
/// ordinary clone refusals return `Ok(false)` with no destination left behind so
/// callers can fall back to their normal durable copy path.
pub(crate) fn try_clone_file(source: &Path, destination: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "copy-on-write clone destination already exists",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    file_clone::try_clone_file(source, destination)
}

/// Decode the source image and write a PNG rendition at `destination`.
/// `max_long_edge = None` preserves full pixel dimensions; `Some(n)` performs a
/// proportional downscale only when the source exceeds `n` pixels on its long edge.
pub(crate) fn render_image_to_png(
    source: &Path,
    destination: &Path,
    max_long_edge: Option<u32>,
) -> PlatformResult<()> {
    image::render_image_to_png(source, destination, max_long_edge)
}

pub(crate) fn image_properties(path: &Path, format: Option<&str>) -> PlatformImageProperties {
    image::image_properties(path, format)
}

pub(crate) fn open_external_file(
    path: &Path,
    file_name: Option<&str>,
    mime_type: Option<&str>,
) -> PlatformResult<()> {
    handoff::open_external_file(path, file_name, mime_type)
}

pub(crate) fn reveal_external_file(
    path: &Path,
    file_name: Option<&str>,
    mime_type: Option<&str>,
) -> PlatformResult<()> {
    handoff::reveal_external_file(path, file_name, mime_type)
}

/// Sandboxed application-support root supplied by the mobile shell. Desktop
/// platforms intentionally return `None`; their established location policy remains
/// in `storage::locations`.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn mobile_application_support_directory() -> Option<PathBuf> {
    mobile::application_support_directory()
}

/// User-visible documents root supplied by the mobile shell when one exists.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn mobile_documents_directory() -> Option<PathBuf> {
    mobile::documents_directory()
}

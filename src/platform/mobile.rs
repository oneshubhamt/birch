use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Rust-side contract implemented by the eventual Dioxus Android/iOS shell.
///
/// Keeping this interface in the core means attachment storage and editor code do not
/// acquire mobile-shell language/ABI conditionals. The mobile shell registers one
/// process-wide adapter during startup and owns native URI/security-scope details.
/// Methods that present native UI must marshal that work onto the platform main
/// thread; Rustnote may invoke attachment operations from a blocking worker.
pub trait MobilePlatformAdapter: Send + Sync + 'static {
    /// Sandboxed durable application-support directory (not a cache directory).
    /// The shell must register this adapter before `Storage::open()`.
    fn application_support_directory(&self) -> PathBuf;

    /// Optional user-visible documents directory. Returning `None` keeps Rustnote's
    /// automatic library/backup data inside the sandbox and uses system pickers for export.
    fn documents_directory(&self) -> Option<PathBuf> {
        None
    }

    /// Open a canonical managed file through the platform's safe read-only/content-URI
    /// mechanism. The shell owns any temporary materialization required by that OS.
    fn open_file(
        &self,
        path: &Path,
        file_name: &str,
        mime_type: Option<&str>,
    ) -> super::PlatformResult<()>;

    /// Present the platform's file/share UI for a canonical managed file. The shell
    /// should preserve `file_name` as the user-visible name without mutating the source.
    fn reveal_or_share_file(
        &self,
        path: &Path,
        file_name: &str,
        mime_type: Option<&str>,
    ) -> super::PlatformResult<()>;

    /// Probe dimensions/alpha through the platform decoder when Rustnote's
    /// lightweight container parser cannot do so itself.
    fn image_properties(&self, _path: &Path) -> super::PlatformImageProperties {
        super::PlatformImageProperties::default()
    }

    /// Decode a platform-supported source image and write a PNG rendition.
    fn render_image_to_png(
        &self,
        source: &Path,
        destination: &Path,
        max_long_edge: Option<u32>,
    ) -> super::PlatformResult<()>;

    /// Android shell bridge for the non-exportable `AndroidKeyStore` AES-256-GCM
    /// wrapping key defined by the managed-backup key-authority contract. The shell must
    /// use a device-local, non-user-auth-gated key with randomized encryption.
    #[cfg(target_os = "android")]
    fn backup_root_status(&self) -> super::SecureKeyStoreResult<super::DeviceBackupRootStatus>;

    #[cfg(target_os = "android")]
    fn create_backup_root_if_absent_for_fresh_state(
        &self,
    ) -> super::SecureKeyStoreResult<super::DeviceBackupRootCreation>;

    /// Seal exactly the repository-keyset plaintext with AES-256-GCM. The
    /// returned envelope must be `12-byte IV || ciphertext || 16-byte tag`.
    #[cfg(target_os = "android")]
    fn seal_backup_repository_keyset(
        &self,
        aad: &[u8],
        keyset: &[u8; super::secure_key_store::BACKUP_REPOSITORY_KEYSET_BYTES],
    ) -> super::SecureKeyStoreResult<Vec<u8>>;

    /// Open the AES-GCM repository-key envelope in Android Keystore. Root-key
    /// material itself must never be exported to Rust.
    #[cfg(target_os = "android")]
    fn open_backup_repository_keyset(
        &self,
        aad: &[u8],
        envelope: &[u8],
    ) -> super::SecureKeyStoreResult<super::SecureKeyMaterial>;
}

static MOBILE_ADAPTER: OnceLock<Box<dyn MobilePlatformAdapter>> = OnceLock::new();

/// Register the native mobile shell once during application startup.
/// Desktop builds may call this only in tests; normal desktop paths never require it.
pub fn register_mobile_platform_adapter(
    adapter: Box<dyn MobilePlatformAdapter>,
) -> Result<(), &'static str> {
    MOBILE_ADAPTER
        .set(adapter)
        .map_err(|_| "Rustnote's mobile platform adapter is already registered")
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn adapter() -> Option<&'static dyn MobilePlatformAdapter> {
    MOBILE_ADAPTER.get().map(Box::as_ref)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn application_support_directory() -> Option<PathBuf> {
    adapter().map(MobilePlatformAdapter::application_support_directory)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(super) fn documents_directory() -> Option<PathBuf> {
    adapter()?.documents_directory()
}

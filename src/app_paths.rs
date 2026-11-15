//! Process-independent Rustnote application paths shared by storage and native services.
//!
//! Keep platform location policy here so crash-recovery metadata does not depend
//! on an OS-temporary namespace and lower-level native code does not depend back
//! on the storage module.

use std::{env, path::PathBuf};

#[cfg(not(any(test, feature = "internal-test-support")))]
use std::io;

#[cfg(not(any(test, feature = "internal-test-support")))]
pub(crate) fn configuration_directory() -> PathBuf {
    configured_directory().unwrap_or_else(|| env::temp_dir().join("rustnote-config"))
}

#[cfg(any(test, feature = "internal-test-support"))]
pub(crate) fn configuration_directory() -> PathBuf {
    static TEST_CONFIGURATION_DIRECTORY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    TEST_CONFIGURATION_DIRECTORY
        .get_or_init(|| {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            env::temp_dir().join(format!(
                "rustnote-test-config-{}-{nonce}",
                std::process::id()
            ))
        })
        .clone()
}

/// Output recovery metadata must survive process death and routine OS-temp
/// cleanup. Unlike older non-security configuration callers, this path is not
/// allowed to fall back to a temporary directory.
#[cfg(not(any(test, feature = "internal-test-support")))]
pub(crate) fn durable_configuration_directory() -> io::Result<PathBuf> {
    let path = configured_directory().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no durable per-user Rustnote configuration directory is available",
        )
    })?;
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Rustnote durable configuration directory is not absolute",
        ));
    }
    Ok(path)
}

#[cfg(not(any(test, feature = "internal-test-support")))]
fn configured_directory() -> Option<PathBuf> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if let Some(root) = crate::platform::mobile_application_support_directory() {
        return Some(root.join("Rustnote Configuration"));
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Rustnote"),
            );
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = env::var_os("APPDATA").or_else(|| env::var_os("LOCALAPPDATA")) {
            return Some(PathBuf::from(app_data).join("Rustnote"));
        }
    }
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("rustnote"));
    }
    if let Some(home) = env::var_os("HOME") {
        return Some(PathBuf::from(home).join(".config").join("rustnote"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_is_process_scoped_and_outside_user_configuration() {
        let first = configuration_directory();
        let second = configuration_directory();
        assert_eq!(
            first, second,
            "test configuration directory must be stable per process"
        );
        assert!(
            first.starts_with(env::temp_dir()),
            "test configuration must remain in the OS temporary namespace"
        );
        assert!(
            first
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("rustnote-test-config-")),
            "test configuration must use a dedicated Rustnote test namespace"
        );
    }
}

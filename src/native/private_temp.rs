use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

/// Create or validate a directory without changing permissions on an existing
/// parent that Rustnote may not own (for example the system temporary root).
/// Callers that own the directory itself must use `ensure_private_directory`
/// on their Rustnote-specific child instead.
pub(crate) fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "temporary parent is not a real directory",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // Create only the requested leaf. Sensitive-temp callers must
            // establish their parent explicitly; recursive creation could
            // otherwise follow an attacker-controlled intermediate symlink.
            fs::create_dir(path)?;
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "temporary parent resolved to an unsafe directory",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

/// Prepare a Rustnote-owned private namespace beneath a stable per-user
/// temporary root. The root is created one level below the OS temp directory
/// with `create_dir` (never `create_dir_all`), so a pre-existing intermediate
/// symlink cannot redirect sensitive staging. The user-scope digest avoids a
/// shared `/tmp/rustnote` ownership collision on multi-user Unix systems.
pub(crate) fn rustnote_temp_directory(name: &str) -> io::Result<PathBuf> {
    use std::path::Component;

    let relative = Path::new(name);
    if relative.components().count() != 1
        || !matches!(relative.components().next(), Some(Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Rustnote temporary namespace name is invalid",
        ));
    }

    let root = rustnote_temp_root()?;
    let namespace = root.join(relative);
    ensure_private_leaf_directory(&namespace)?;
    Ok(namespace)
}

fn rustnote_temp_root() -> io::Result<PathBuf> {
    let system_temp = std::env::temp_dir();
    let metadata = fs::metadata(&system_temp)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "system temporary path is not a directory",
        ));
    }

    // Keep the namespace stable across routine shell/session changes. HOME (or
    // USERPROFILE on Windows) is a substantially more durable per-user anchor
    // than transient variables such as USER or XDG_CONFIG_HOME. The OS temp
    // path is included so different temp volumes cannot alias the same name.
    let mut scope = Vec::new();
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    if let Some(home) = home {
        let home = fs::canonicalize(&home).unwrap_or_else(|_| PathBuf::from(home));
        scope.extend_from_slice(home.to_string_lossy().as_bytes());
        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(&home) {
            use std::os::unix::fs::MetadataExt;
            scope.extend_from_slice(&metadata.uid().to_le_bytes());
        }
    } else if let Some(user) = std::env::var_os("USERNAME").or_else(|| std::env::var_os("USER")) {
        scope.extend_from_slice(user.to_string_lossy().as_bytes());
    }
    scope.push(0);
    scope.extend_from_slice(system_temp.to_string_lossy().as_bytes());
    let digest = Sha256::digest(&scope);
    scope.fill(0);
    let suffix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let root = system_temp.join(format!("rustnote-{suffix}"));
    ensure_private_leaf_directory(&root)?;
    Ok(root)
}

fn ensure_private_leaf_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "private staging path is not a real directory",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // Another Rustnote thread/process may create this stable private leaf
            // after our absence check. Treat AlreadyExists as a benign race only
            // after re-inspecting the final path below; a symlink or non-directory
            // created by a competing actor still fails closed.
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "private staging path resolved to an unsafe directory",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Create or validate an application-private staging directory.
///
/// Sensitive staging must not follow a pre-existing symlink at the directory
/// itself. Unix permissions are tightened even when the directory already
/// exists, so an unusually permissive process umask cannot widen the boundary.
pub(crate) fn ensure_private_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "private staging path is not a real directory",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "private staging path resolved to an unsafe directory",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Create a new private staging file without following an existing final-path
/// symlink. `create_new` supplies the atomic no-overwrite guarantee.
pub(crate) fn create_private_file(path: &Path) -> io::Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "private file has no parent"))?;
    ensure_private_directory(parent)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Create a private file inside an existing directory without changing that
/// directory's permissions. This is required for Rustnote-owned hidden output
/// siblings located beside an explicitly user-selected destination.
pub(crate) fn create_private_file_in_existing_directory(path: &Path) -> io::Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "private file has no parent"))?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "private file parent is not a real directory",
        ));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Create a private leaf directory without changing its existing parent.
/// Ownership metadata must already exist before this is called.
pub(crate) fn create_private_directory_in_existing_directory(path: &Path) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "private directory has no parent",
        )
    })?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "private directory parent is not a real directory",
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700).create(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(path)?;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private directory creation produced an unsafe path",
        ));
    }
    Ok(())
}

/// Tighten a converter-created sensitive file after creation.
///
/// External image/PDF tools may insist on creating their own output path, so
/// Rustnote cannot always supply `create_private_file` directly. Validate that
/// the result is a real regular file (never a symlink) and normalize its Unix
/// mode immediately before Rustnote publishes or serves it.
pub(crate) fn harden_existing_file(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "private staging output is not a real file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("rustnote-private-temp-{label}-{nonce}"))
    }

    #[test]
    fn private_directory_and_file_are_created() {
        let root = root("create");
        ensure_private_directory(&root).expect("private directory");
        let path = root.join("secret.bin");
        drop(create_private_file(&path).expect("private file"));
        assert!(path.is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rustnote_temp_namespace_is_private_and_nonrecursive() {
        let name = format!("namespace-{}", rand::random::<u64>());
        let path = rustnote_temp_directory(&name).expect("Rustnote temp namespace");
        assert!(path.is_dir());
        assert!(
            path.parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("rustnote-")),
            "sensitive namespace must live under the per-user Rustnote temp root",
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn concurrent_private_leaf_creation_accepts_only_the_safe_directory() {
        use std::sync::{Arc, Barrier};

        let parent = root("concurrent-leaf-parent");
        fs::create_dir(&parent).expect("parent");
        let path = parent.join("shared-private-leaf");
        let workers = 32;
        let barrier = Arc::new(Barrier::new(workers));
        let mut threads = Vec::with_capacity(workers);

        for _ in 0..workers {
            let barrier = Arc::clone(&barrier);
            let path = path.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                ensure_private_leaf_directory(&path)
            }));
        }

        for thread in threads {
            thread
                .join()
                .expect("private-leaf worker must not panic")
                .expect("safe concurrent private-leaf creation must succeed");
        }
        let metadata = fs::symlink_metadata(&path).expect("private leaf metadata");
        assert!(metadata.is_dir());
        assert!(!metadata.file_type().is_symlink());
        let _ = fs::remove_dir_all(parent);
    }

    #[cfg(unix)]
    #[test]
    fn private_permissions_are_restrictive_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let root = root("permissions");
        ensure_private_directory(&root).expect("private directory");
        let path = root.join("secret.bin");
        drop(create_private_file(&path).expect("private file"));
        assert_eq!(
            fs::metadata(&root)
                .expect("dir metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("widen file mode");
        harden_existing_file(&path).expect("re-harden file");
        assert_eq!(
            fs::metadata(&path)
                .expect("hardened metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }
}

use std::{
    collections::HashMap,
    fs,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

const OWNER_LOCK_FILE: &str = ".rustnote-owner.lock";
const CLEANUP_CLAIM_FILE: &str = ".rustnote-cleaning";
const SESSION_TOKEN_BYTES: usize = 16;

/// Process/session ownership for sensitive temporary plaintext.
///
/// PID names are useful diagnostics but are not ownership proof because operating
/// systems reuse PIDs. The OS-held exclusive file lock is the proof: it is
/// released by a normal drop and by the OS after a hard process death.
pub(crate) struct SensitiveTempOwner {
    root: PathBuf,
    lock_file: Mutex<Option<fs::File>>,
}


#[derive(Clone)]
pub(crate) struct ProcessPrivateOwner {
    owner: Arc<SensitiveTempOwner>,
}

impl ProcessPrivateOwner {
    pub(crate) fn root(&self) -> &Path {
        self.owner.root()
    }
}

impl std::fmt::Debug for ProcessPrivateOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProcessPrivateOwner([REDACTED])")
    }
}

impl SensitiveTempOwner {
    pub(crate) fn acquire(parent: &Path, prefix: &str) -> super::NativeResult<Self> {
        // The caller may supply a shared/system parent. Validate/create it, but
        // never chmod a directory Rustnote does not own. Only the freshly-created
        // Rustnote session child is hardened below.
        super::private_temp::ensure_directory(parent).map_err(|error| {
            super::NativeError::operation(format!(
                "Could not prepare sensitive temporary parent {}: {error}",
                parent.display()
            ))
        })?;

        for _ in 0..8 {
            let token = random_token();
            let root = parent.join(format!("{prefix}v2-{}-{token}", std::process::id()));
            match fs::create_dir(&root) {
                Ok(()) => {
                    super::private_temp::ensure_private_directory(&root).map_err(|error| {
                        let _ = super::privacy::remove_tree(&root);
                        super::NativeError::operation(format!(
                            "Could not secure sensitive temporary directory {}: {error}",
                            root.display()
                        ))
                    })?;
                    let lock_file = match acquire_owner_lock(&root) {
                        Ok(lock_file) => lock_file,
                        Err(error) => {
                            let _ = super::privacy::remove_tree(&root);
                            return Err(error);
                        }
                    };
                    // A concurrent startup scrub may have observed the v2 child
                    // before its ownership marker was published and removed it.
                    // Never expose such a half-owned session: verify both the
                    // directory and marker after the OS lock is acquired, then
                    // retry allocation if either disappeared.
                    let cleanup_claimed = crate::security::managed_path_is_present(
                        &root.join(CLEANUP_CLAIM_FILE),
                    )
                    .map_err(|error| {
                        super::NativeError::operation(format!(
                            "Could not verify sensitive temporary cleanup claim: {error}"
                        ))
                    })?;
                    if !root.is_dir()
                        || !root.join(OWNER_LOCK_FILE).is_file()
                        || cleanup_claimed
                    {
                        drop(lock_file);
                        let _ = super::privacy::remove_tree(&root);
                        continue;
                    }
                    return Ok(Self {
                        root,
                        lock_file: Mutex::new(Some(lock_file)),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return super::native_error(format!(
                        "Could not create sensitive temporary directory: {error}"
                    ));
                }
            }
        }
        super::native_error("Could not allocate sensitive temporary session")
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl std::fmt::Debug for SensitiveTempOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SensitiveTempOwner([REDACTED])")
    }
}

impl Drop for SensitiveTempOwner {
    fn drop(&mut self) {
        if let Ok(mut lock_file) = self.lock_file.lock() {
            // Closing the file releases the OS lock before deleting the
            // directory. This ordering is required on Windows.
            drop(lock_file.take());
        }
        let _ = super::privacy::remove_tree(&self.root);
    }
}

fn acquire_owner_lock(root: &Path) -> super::NativeResult<fs::File> {
    let path = root.join(OWNER_LOCK_FILE);
    let file = super::private_temp::create_private_file(&path).map_err(|error| {
        super::NativeError::operation(format!(
            "Could not create sensitive temporary ownership lock: {error}"
        ))
    })?;
    file.lock().map_err(|error| {
        super::NativeError::operation(format!(
            "Could not acquire sensitive temporary ownership lock: {error}"
        ))
    })?;
    file.sync_all().map_err(|error| {
        super::NativeError::operation(format!(
            "Could not persist sensitive temporary ownership marker: {error}"
        ))
    })?;
    Ok(file)
}

fn random_token() -> String {
    rand::random::<[u8; SESSION_TOKEN_BYTES]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn is_owner_artifact_name(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| matches!(name, OWNER_LOCK_FILE | CLEANUP_CLAIM_FILE))
}

fn valid_owner_marker(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(!metadata.file_type().is_symlink() && metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
fn owner_is_live(root: &Path) -> Option<bool> {
    let lock_path = root.join(OWNER_LOCK_FILE);
    match valid_owner_marker(&lock_path) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(_) => return Some(true),
    }
    let Ok(file) = fs::OpenOptions::new().read(true).write(true).open(&lock_path) else {
        // An unreadable owner marker is not permission to delete potentially
        // live plaintext belonging to another process.
        return Some(true);
    };
    match file.try_lock() {
        Ok(()) => Some(false),
        Err(fs::TryLockError::WouldBlock) => Some(true),
        Err(fs::TryLockError::Error(_)) => Some(true),
    }
}

fn claim_abandoned_owner_for_cleanup(root: &Path) -> Option<bool> {
    let lock_path = root.join(OWNER_LOCK_FILE);
    match valid_owner_marker(&lock_path) {
        Ok(true) => {}
        Ok(false) => return None,
        // Ambiguous owner-marker observation is never permission to claim and
        // delete a potentially live plaintext session.
        Err(_) => return Some(false),
    }
    let Ok(file) = fs::OpenOptions::new().read(true).write(true).open(&lock_path) else {
        return Some(false);
    };
    match file.try_lock() {
        Ok(()) => {
            // Publish the cleanup claim *while the owner lock is held*. A racing
            // acquirer that was waiting on the same lock must observe this marker
            // after it acquires and abandon the half-owned session.
            let claim_path = root.join(CLEANUP_CLAIM_FILE);
            let claim_ready = match super::private_temp::create_private_file(&claim_path) {
                Ok(claim) => claim.sync_all().is_ok(),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => true,
                Err(_) => false,
            };
            Some(claim_ready)
        }
        Err(fs::TryLockError::WouldBlock) => Some(false),
        Err(fs::TryLockError::Error(_)) => Some(false),
    }
}

fn claim_unowned_v2_for_cleanup(root: &Path) -> bool {
    // Close the directory-created/owner-marker-not-yet-published race. Cleanup
    // must claim a markerless v2 session before deciding to delete it; an
    // acquirer always checks this claim after locking its owner marker and will
    // abandon the session instead of publishing plaintext into a directory
    // already claimed for cleanup.
    let claim_path = root.join(CLEANUP_CLAIM_FILE);
    match super::private_temp::create_private_file(&claim_path) {
        Ok(claim) => claim.sync_all().is_ok(),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => true,
        Err(_) => false,
    }
}

pub(crate) fn cleanup_session_directories(root: &Path, label: &str) -> super::NativeResult<usize> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return super::native_error(format!(
                "Could not inspect {label}: sensitive temporary root is not a real directory"
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return super::native_error(format!("Could not inspect {label}: {error}"));
        }
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return super::native_error(format!("Could not inspect {label}: {error}")),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(|error| {
            super::NativeError::operation(format!("Could not inspect {label}: {error}"))
        })?;
        if !entry.file_type().map_err(|error| {
            super::NativeError::operation(format!("Could not inspect {label}: {error}"))
        })?.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        if !name.starts_with("session-v2-") {
            continue;
        }
        let path = entry.path();
        let removable = match claim_abandoned_owner_for_cleanup(&path) {
            Some(true) => true,
            Some(false) => false,
            // v2 ownership is deliberately binary, but cleanup must first
            // publish a claim so an owner being created concurrently cannot
            // subsequently expose this directory as live.
            None => claim_unowned_v2_for_cleanup(&path),
        };
        if removable {
            super::privacy::remove_tree(&path).map_err(|error| {
                super::NativeError::operation(format!("Could not clear {label}: {error}"))
            })?;
            removed += 1;
        }
    }
    Ok(removed)
}

static PROCESS_SESSIONS: OnceLock<Mutex<HashMap<PathBuf, Arc<SensitiveTempOwner>>>> = OnceLock::new();

/// Return the one sensitive-temp session owned by this Rustnote process beneath
/// a given parent. The owner object remains live for the process lifetime.
pub(crate) fn process_session_owner(
    parent: &Path,
    label: &str,
) -> super::NativeResult<ProcessPrivateOwner> {
    // These parents are Rustnote-owned namespaces, unlike an arbitrary system
    // temp root supplied to `SensitiveTempOwner::acquire`.
    super::private_temp::ensure_private_directory(parent).map_err(|error| {
        super::NativeError::operation(format!("Could not prepare {label}: {error}"))
    })?;
    let sessions = PROCESS_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut sessions = sessions.lock().map_err(|_| {
        super::NativeError::operation("Sensitive temporary session registry is unavailable")
    })?;
    if let Some(owner) = sessions.get(parent) {
        return Ok(ProcessPrivateOwner {
            owner: Arc::clone(owner),
        });
    }
    let owner = Arc::new(SensitiveTempOwner::acquire(parent, "session-").map_err(|error| {
        error.context(format!("Could not prepare {label}"))
    })?);
    sessions.insert(parent.to_path_buf(), Arc::clone(&owner));
    Ok(ProcessPrivateOwner { owner })
}

pub(crate) fn process_session_root(parent: &Path, label: &str) -> super::NativeResult<PathBuf> {
    Ok(process_session_owner(parent, label)?.root().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustnote-{label}-{}", random_token()))
    }

    #[cfg(unix)]
    #[test]
    fn acquire_never_changes_existing_parent_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let parent = temp_root("owner-parent-mode");
        fs::create_dir_all(&parent).expect("parent");
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).expect("parent mode");
        let owner = SensitiveTempOwner::acquire(&parent, "session-").expect("owner");
        assert_eq!(
            fs::metadata(&parent).expect("metadata").permissions().mode() & 0o777,
            0o755,
            "SensitiveTempOwner must not harden a parent it does not own",
        );
        drop(owner);
        let _ = super::super::privacy::remove_tree(&parent);
    }

    #[cfg(unix)]
    #[test]
    fn ambiguous_owner_marker_metadata_never_authorizes_cleanup() {
        use std::os::unix::fs::symlink;

        let parent = temp_root("owner-marker-ambiguous");
        fs::create_dir_all(&parent).expect("parent");
        let loop_path = parent.join("loop");
        symlink("loop", &loop_path).expect("symlink loop");

        assert_eq!(
            owner_is_live(&loop_path),
            Some(true),
            "ambiguous owner metadata must be treated as potentially live",
        );
        assert_eq!(
            claim_abandoned_owner_for_cleanup(&loop_path),
            Some(false),
            "ambiguous owner metadata must never authorize cleanup",
        );

        fs::remove_file(&loop_path).expect("remove loop");
        let _ = super::super::privacy::remove_tree(&parent);
    }

    #[test]
    fn live_owner_lock_prevents_cleanup() {
        let parent = temp_root("owner-live");
        let owner = SensitiveTempOwner::acquire(&parent, "session-").expect("owner");
        assert_eq!(owner_is_live(owner.root()), Some(true));
        assert_eq!(
            cleanup_session_directories(&parent, "test sessions").expect("cleanup"),
            0
        );
        assert!(owner.root().exists());
        drop(owner);
        let _ = super::super::privacy::remove_tree(&parent);
    }

    #[test]
    fn abandoned_owner_lock_is_removed_without_pid_heuristics() {
        let parent = temp_root("owner-abandoned");
        let owner = SensitiveTempOwner::acquire(&parent, "session-").expect("owner");
        let root = owner.root().to_path_buf();
        if let Ok(mut lock_file) = owner.lock_file.lock() {
            drop(lock_file.take());
        }
        std::mem::forget(owner);
        assert_eq!(owner_is_live(&root), Some(false));
        assert_eq!(
            cleanup_session_directories(&parent, "test sessions").expect("cleanup"),
            1
        );
        assert!(!root.exists());
        let _ = super::super::privacy::remove_tree(&parent);
    }

    #[test]
    fn markerless_v2_cleanup_publishes_claim_before_removal() {
        let parent = temp_root("owner-markerless-claim");
        let interrupted = parent.join(format!(
            "session-v2-{}-claimwindow",
            std::process::id()
        ));
        fs::create_dir_all(&interrupted).expect("interrupted session directory");
        assert!(!interrupted.join(CLEANUP_CLAIM_FILE).exists());
        assert!(claim_unowned_v2_for_cleanup(&interrupted));
        assert!(
            interrupted.join(CLEANUP_CLAIM_FILE).is_file(),
            "markerless v2 cleanup must publish a claim before deletion"
        );
        let _ = super::super::privacy::remove_tree(&parent);
    }

    #[test]
    fn v2_owner_setup_crash_window_is_removed_without_pid_heuristics() {
        let parent = temp_root("owner-setup-crash");
        let interrupted = parent.join(format!(
            "session-v2-{}-deadbeef",
            std::process::id()
        ));
        fs::create_dir_all(&interrupted).expect("interrupted session directory");
        fs::write(interrupted.join("plaintext.tmp"), b"secret").expect("staged plaintext");

        assert_eq!(
            cleanup_session_directories(&parent, "interrupted sessions").expect("cleanup"),
            1
        );
        assert!(!interrupted.exists());
        let _ = super::super::privacy::remove_tree(&parent);
    }
}

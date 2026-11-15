//! Explicit runtime context for the currently opened Rustnote library.
//!
//! Library-scoped services receive this context (or a narrower capability derived
//! from it) instead of discovering a mutable process-global data directory.

use std::{
    fmt,
    sync::{Arc, RwLock},
};

pub use crate::library_paths::LibraryPaths;
use crate::storage::{AttachmentTransportStore, Storage, StorageResult};

#[derive(Clone, Debug)]
pub struct LibraryContext {
    storage: Storage,
    paths: LibraryPaths,
}

impl LibraryContext {
    pub fn open() -> StorageResult<Self> {
        Storage::open().map(Self::from_storage)
    }

    pub(crate) fn from_storage(storage: Storage) -> Self {
        let paths = LibraryPaths::from_data_directory(storage.data_directory());
        Self { storage, paths }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn paths(&self) -> &LibraryPaths {
        &self.paths
    }

    pub fn transport_store(&self) -> AttachmentTransportStore {
        self.storage.transport_store()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryRuntimeError {
    LockPoisoned,
    RestartRequired,
}

impl fmt::Display for LibraryRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LockPoisoned => formatter.write_str("library runtime lock is poisoned"),
            Self::RestartRequired => formatter.write_str(
                "Rustnote cannot switch the active library without restarting the application",
            ),
        }
    }
}

impl std::error::Error for LibraryRuntimeError {}

#[derive(Clone, Default)]
pub struct LibraryRuntime {
    inner: Arc<RwLock<Option<Arc<LibraryContext>>>>,
}

impl fmt::Debug for LibraryRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LibraryRuntime")
            .field("active", &self.current().is_some())
            .finish()
    }
}

impl LibraryRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish the fully initialized library to long-lived transport services.
    /// Re-publishing the same library is harmless; switching libraries in-process
    /// is intentionally rejected because Rustnote currently requires restart after
    /// storage-location changes.
    pub fn activate(&self, library: LibraryContext) -> Result<(), LibraryRuntimeError> {
        let mut active = self
            .inner
            .write()
            .map_err(|_| LibraryRuntimeError::LockPoisoned)?;
        if let Some(current) = active.as_ref() {
            if current.storage().data_directory() == library.storage().data_directory()
                && current.storage().automatic_backup_directory()
                    == library.storage().automatic_backup_directory()
            {
                return Ok(());
            }
            return Err(LibraryRuntimeError::RestartRequired);
        }
        *active = Some(Arc::new(library));
        Ok(())
    }

    pub fn current(&self) -> Option<Arc<LibraryContext>> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn test_library(label: &str) -> (LibraryContext, PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rustnote-a4-library-{label}-{}-{unique}",
            std::process::id()
        ));
        let storage = Storage::at_with_backup(root.join("data"), root.join("backups"))
            .expect("test storage should initialize");
        (LibraryContext::from_storage(storage), root)
    }

    #[test]
    fn paths_are_bound_to_the_specific_storage_instance() {
        let (library, root) = test_library("paths");
        assert_eq!(
            library.paths().data_directory(),
            library.storage().data_directory()
        );
        assert!(
            library
                .paths()
                .attachment_handoff_directory()
                .starts_with(library.paths().data_directory())
        );
        assert!(
            library
                .paths()
                .media_compat_directory()
                .starts_with(library.paths().data_directory())
        );
        drop(library);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_rejects_silent_library_switching() {
        let runtime = LibraryRuntime::new();
        let (first, first_root) = test_library("runtime-first");
        let (second, second_root) = test_library("runtime-second");
        runtime
            .activate(first.clone())
            .expect("activate first library");
        runtime
            .activate(first)
            .expect("re-activate same library should be harmless");
        assert!(runtime.activate(second).is_err());
        drop(runtime);
        let _ = fs::remove_dir_all(first_root);
        let _ = fs::remove_dir_all(second_root);
    }
}

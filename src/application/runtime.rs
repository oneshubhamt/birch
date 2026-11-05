use std::path::PathBuf;

use crate::library::LibraryPaths;
use crate::storage::{DeferredMaintenanceReport, RuntimeStore};

use super::privacy::WholeLibraryOperationLease;

use super::errors::{DataAccessError, DataResult};

/// Library/process lifecycle operations that should not be orchestrated by Dioxus surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeService {
    storage: RuntimeStore,
    paths: LibraryPaths,
}

impl RuntimeService {
    pub(crate) fn new(storage: RuntimeStore, paths: LibraryPaths) -> Self {
        Self { storage, paths }
    }

    /// Best-effort cache/handoff cleanup. These are derived/transient resources, so
    /// cleanup failures remain warnings exactly as before and do not block library open.
    pub(crate) fn cleanup_library_transients(&self) {
        if let Err(error) = crate::media_compat::cleanup_stale_temporary_files(
            &self.paths.media_compat_directory(),
        ) {
            eprintln!("Rustnote media compatibility cleanup skipped: {error}");
        }
        if let Err(error) = crate::media_compat::prune_cache(
            &self.paths.media_compat_directory(),
        ) {
            eprintln!("Rustnote media compatibility cache pruning skipped: {error}");
        }
        if let Err(error) = crate::native::handoff::cleanup_stale_attachment_handoffs(
            &self.paths.attachment_handoff_directory(),
        ) {
            eprintln!("Rustnote attachment handoff cleanup skipped: {error}");
        }
    }

    pub(crate) fn run_deferred_maintenance(&self) -> DataResult<DeferredMaintenanceReport> {
        self.storage
            .run_deferred_maintenance()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn reconcile_pending_attachment_ingests(&self) -> DataResult<(usize, usize)> {
        self.storage
            .reconcile_pending_attachment_ingests()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn purge_expired_notes(&self) -> DataResult<usize> {
        self.storage
            .purge_expired_notes()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn automatic_backup_due(&self) -> DataResult<bool> {
        self.storage
            .automatic_backup_due()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn maybe_create_automatic_backup(
        &self,
        _privacy_lease: &WholeLibraryOperationLease,
    ) -> DataResult<Option<PathBuf>> {
        self.storage
            .maybe_create_automatic_backup()
            .map_err(DataAccessError::from)
    }

    pub(crate) fn durability_checkpoint(
        &self,
        _privacy_lease: &WholeLibraryOperationLease,
    ) -> DataResult<()> {
        self.storage
            .durability_checkpoint()
            .map_err(DataAccessError::from)
    }
}

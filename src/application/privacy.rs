use crate::security::{
    PrivacyPublicationAuthorization as CoordinatorPublicationAuthorization,
    PrivacyPublicationLease as CoordinatorPublicationLease,
    WholeLibraryOperationLease as CoordinatorWholeLibraryOperationLease,
};
use crate::storage::{PrivacyOperationStore, PrivacyPublicationStore};

use super::errors::{DataAccessError, DataResult};

/// Application-facing publication authority. This service intentionally exposes
/// no note data and no transition primitives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrivacyPublicationService {
    storage: PrivacyPublicationStore,
}

/// Application-facing authority for whole-library durability/snapshot barriers.
/// This is deliberately separate from publication tokens: backups, archives and
/// close checkpoints do not publish protected plaintext into UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrivacyOperationService {
    storage: PrivacyOperationStore,
}

/// Opaque lease spanning one whole-library durability/snapshot operation.
pub(crate) struct WholeLibraryOperationLease {
    _inner: CoordinatorWholeLibraryOperationLease,
}

/// Opaque application authorization used only by the UI privacy-token authority.
/// Coordinator generations and admission state remain hidden below this layer.
#[derive(Clone)]
pub(crate) struct PrivacyPublicationAuthorization {
    inner: CoordinatorPublicationAuthorization,
}

/// Opaque lease spanning one actual publication handoff.
pub(crate) struct PrivacyPublicationLease {
    _inner: CoordinatorPublicationLease,
}

impl PrivacyOperationService {
    pub(crate) fn new(storage: PrivacyOperationStore) -> Self {
        Self { storage }
    }

    pub(crate) fn begin_whole_library_operation(
        &self,
    ) -> DataResult<Option<WholeLibraryOperationLease>> {
        self.storage
            .begin_whole_library_operation()
            .map(|lease| lease.map(|inner| WholeLibraryOperationLease { _inner: inner }))
            .map_err(DataAccessError::from)
    }
}

impl std::fmt::Debug for WholeLibraryOperationLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("WholeLibraryOperationLease([REDACTED])")
    }
}

impl PrivacyPublicationService {
    pub(crate) fn new(storage: PrivacyPublicationStore) -> Self {
        Self { storage }
    }

    pub(crate) fn capture_note(
        &self,
        note_id: &crate::domain::NoteId,
    ) -> DataResult<Option<PrivacyPublicationAuthorization>> {
        self.storage
            .capture_note(note_id)
            .map(|authorization| authorization.map(PrivacyPublicationAuthorization::new))
            .map_err(DataAccessError::from)
    }

    pub(crate) fn capture_multi_note(&self) -> DataResult<Option<PrivacyPublicationAuthorization>> {
        self.storage
            .capture_multi_note()
            .map(|authorization| authorization.map(PrivacyPublicationAuthorization::new))
            .map_err(DataAccessError::from)
    }
}

impl PrivacyPublicationAuthorization {
    fn new(inner: CoordinatorPublicationAuthorization) -> Self {
        Self { inner }
    }

    pub(crate) fn is_current(&self) -> bool {
        self.inner.is_current().unwrap_or(false)
    }

    pub(crate) fn begin_publication(&self) -> Option<PrivacyPublicationLease> {
        self.inner
            .begin_publication()
            .ok()
            .flatten()
            .map(|inner| PrivacyPublicationLease { _inner: inner })
    }
}

impl std::fmt::Debug for PrivacyPublicationAuthorization {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PrivacyPublicationAuthorization([REDACTED])")
    }
}

impl std::fmt::Debug for PrivacyPublicationLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PrivacyPublicationLease([REDACTED])")
    }
}

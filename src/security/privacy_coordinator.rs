//! Canonical privacy-transition authority.
//!
//! Privacy lock choreography is centralized here rather than reconstructed by
//! callers. A mutation is admitted first. Revoking note transitions publish and
//! drain Vault readers before
//! plaintext derivative producers are drained; Protect drains derivatives without
//! Vault revocation. Only then is the storage mutation mutex acquired. The
//! returned guard retains every acquired boundary until the mutation scope ends.

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

use crate::domain::NoteId;

use super::derivative::{PlaintextDerivativeGate, PlaintextDerivativeTransition};
use super::runtime::{VaultRevocationTransition, VaultRuntime};
use super::{PlaintextDerivativePermit, VaultNoteAccess, VaultReadPermit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NotePrivacyTransition {
    Protect,
    Lock,
    Unlock,
    Unprotect,
    Delete,
    Restore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LibraryPrivacyTransition {
    Restore,
}

#[derive(Default)]
struct PrivacyAdmissionState {
    note_transitions: HashMap<NoteId, NotePrivacyTransition>,
    library_transition: Option<LibraryPrivacyTransition>,
    library_publication_generation: u64,
    multi_note_publication_generation: u64,
    note_publication_generations: HashMap<NoteId, u64>,
    active_multi_note_publications: usize,
    active_note_publications: HashMap<NoteId, usize>,
    active_whole_library_operations: usize,
}

struct PrivacyCoordinatorInner {
    admission: Mutex<PrivacyAdmissionState>,
    admission_changed: Condvar,
    publication_drained: Condvar,
    whole_library_operation_drained: Condvar,
    operation_lock: Mutex<()>,
    plaintext_derivatives: PlaintextDerivativeGate,
    vault_runtime: Arc<VaultRuntime>,
}

/// One authority for privacy mutation admission, plaintext producer gating,
/// Vault reader revocation/drain, storage mutation serialization, and privacy
/// generation advancement.
#[derive(Clone)]
pub(crate) struct PrivacyCoordinator {
    inner: Arc<PrivacyCoordinatorInner>,
}

/// Snapshot authorizing publication of decrypted/protected-derived information.
///
/// Note-local snapshots are invalidated only by a transition for that note or
/// by a whole-library transition. Multi-note snapshots are invalidated by any
/// note or library transition. Capture fails whenever the requested scope is
/// actively revoking, so callers cannot mint a fresh authorization inside that
/// scope's revocation window.
#[derive(Clone)]
pub(crate) struct PrivacyPublicationAuthorization {
    inner: Arc<PrivacyCoordinatorInner>,
    scope: PrivacyPublicationScope,
}

#[derive(Clone)]
enum PrivacyPublicationScope {
    Note {
        note_id: NoteId,
        library_generation: u64,
        note_generation: u64,
    },
    MultiNote {
        library_generation: u64,
        multi_note_generation: u64,
    },
}

/// Short-lived lease covering the actual publication handoff. A transition is
/// admitted before it waits for these leases, so no new lease can begin once
/// revocation starts and already-started publication completes before the
/// transition continues into plaintext/Vault teardown.
pub(crate) struct PrivacyPublicationLease {
    inner: Arc<PrivacyCoordinatorInner>,
    scope: PrivacyPublicationLeaseScope,
    active: bool,
}

/// Read-side privacy barrier for a whole-library durability or snapshot operation.
///
/// Unlike `PrivacyPublicationLease`, this guard authorizes no UI handoff. It
/// exists solely to keep Lock/Protect/Unlock/Unprotect/Restore from crossing a
/// whole-library checkpoint or persistent snapshot that has already begun. A
/// later privacy transition is admitted first (blocking newer work), then waits
/// for this already-started operation to drain before revoking runtime/storage
/// authority. Callers must therefore perform any note-local saves *before*
/// acquiring this lease; the guarded phase is snapshot/durability-only.
pub(crate) struct WholeLibraryOperationLease {
    inner: Arc<PrivacyCoordinatorInner>,
    active: bool,
}

#[derive(Clone)]
enum PrivacyPublicationLeaseScope {
    Note(NoteId),
    MultiNote,
}

impl fmt::Debug for PrivacyPublicationAuthorization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PrivacyPublicationAuthorization([REDACTED])")
    }
}

impl PrivacyPublicationAuthorization {
    pub(crate) fn is_current(&self) -> Result<bool, &'static str> {
        let state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        Ok(match &self.scope {
            PrivacyPublicationScope::Note {
                note_id,
                library_generation,
                note_generation,
            } => {
                state.library_transition.is_none()
                    && !state.note_transitions.contains_key(note_id)
                    && state.library_publication_generation == *library_generation
                    && state
                        .note_publication_generations
                        .get(note_id)
                        .copied()
                        .unwrap_or(0)
                        == *note_generation
            }
            PrivacyPublicationScope::MultiNote {
                library_generation,
                multi_note_generation,
            } => {
                state.library_transition.is_none()
                    && state.note_transitions.is_empty()
                    && state.library_publication_generation == *library_generation
                    && state.multi_note_publication_generation == *multi_note_generation
            }
        })
    }

    /// Linearize an asynchronous UI handoff against privacy-transition
    /// admission. The caller should hold the returned lease only across the
    /// smallest operation that actually publishes protected-derived data.
    pub(crate) fn begin_publication(
        &self,
    ) -> Result<Option<PrivacyPublicationLease>, &'static str> {
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        let lease_scope = match &self.scope {
            PrivacyPublicationScope::Note {
                note_id,
                library_generation,
                note_generation,
            } => {
                if state.library_transition.is_some()
                    || state.note_transitions.contains_key(note_id)
                    || state.library_publication_generation != *library_generation
                    || state
                        .note_publication_generations
                        .get(note_id)
                        .copied()
                        .unwrap_or(0)
                        != *note_generation
                {
                    return Ok(None);
                }
                let count = state
                    .active_note_publications
                    .entry(note_id.clone())
                    .or_insert(0);
                *count = count
                    .checked_add(1)
                    .ok_or("privacy publication lease count overflow")?;
                PrivacyPublicationLeaseScope::Note(note_id.clone())
            }
            PrivacyPublicationScope::MultiNote {
                library_generation,
                multi_note_generation,
            } => {
                if state.library_transition.is_some()
                    || !state.note_transitions.is_empty()
                    || state.library_publication_generation != *library_generation
                    || state.multi_note_publication_generation != *multi_note_generation
                {
                    return Ok(None);
                }
                state.active_multi_note_publications = state
                    .active_multi_note_publications
                    .checked_add(1)
                    .ok_or("privacy publication lease count overflow")?;
                PrivacyPublicationLeaseScope::MultiNote
            }
        };
        Ok(Some(PrivacyPublicationLease {
            inner: Arc::clone(&self.inner),
            scope: lease_scope,
            active: true,
        }))
    }
}

impl fmt::Debug for PrivacyPublicationLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PrivacyPublicationLease([REDACTED])")
    }
}

impl fmt::Debug for WholeLibraryOperationLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WholeLibraryOperationLease([REDACTED])")
    }
}

impl Drop for PrivacyPublicationLease {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut state = self
            .inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &self.scope {
            PrivacyPublicationLeaseScope::Note(note_id) => {
                if let Some(count) = state.active_note_publications.get_mut(note_id) {
                    debug_assert!(*count > 0);
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        state.active_note_publications.remove(note_id);
                    }
                }
            }
            PrivacyPublicationLeaseScope::MultiNote => {
                debug_assert!(state.active_multi_note_publications > 0);
                state.active_multi_note_publications =
                    state.active_multi_note_publications.saturating_sub(1);
            }
        }
        self.active = false;
        self.inner.publication_drained.notify_all();
    }
}

impl Drop for WholeLibraryOperationLease {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut state = self
            .inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert!(state.active_whole_library_operations > 0);
        state.active_whole_library_operations =
            state.active_whole_library_operations.saturating_sub(1);
        self.active = false;
        self.inner.whole_library_operation_drained.notify_all();
    }
}

/// A fully acquired note-level privacy mutation boundary.
///
/// Field order is deliberate: on drop, the storage mutex is released first,
/// then Vault revocation, then derivative gating, and admission last.
pub(crate) struct NotePrivacyMutationGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
    _vault_transition: Option<VaultRevocationTransition>,
    _derivative_transition: Option<PlaintextDerivativeTransition>,
    _admission: PrivacyAdmissionGuard,
}

/// Canonical authorization for mutating one already-unlocked protected note.
///
/// The coordinator, not the storage caller, owns the only permitted ordering:
/// acquire Vault reader authorization first, then serialize storage mutation.
/// The storage mutex is declared first so it is released before the Vault
/// reader on drop, matching the transition/drain ordering.
pub(crate) struct ProtectedNoteMutationGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
    access: VaultNoteAccess,
}

/// Canonical authorization for one storage mutation spanning protected notes.
pub(crate) struct ProtectedNotesMutationGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
    accesses: Vec<VaultNoteAccess>,
}

/// One linearized authorization for any operation whose lifetime may expose or
/// materialize note-owned plaintext. The coordinator acquires the derivative
/// permit and the currently available Vault reader under the same admission
/// mutex, so a privacy transition can linearize only before or after the pair.
///
/// Field order is deliberate: Vault access is released before the derivative
/// permit. A revoking transition therefore remains blocked at derivative drain
/// until the complete protected authorization has disappeared.
#[derive(Clone)]
pub(crate) struct PlaintextOperationPermit {
    access: Option<VaultNoteAccess>,
    _derivative: PlaintextDerivativePermit,
}

/// Linearized plaintext lifetime for a known set of ordinary-export notes.
/// The permits are acquired atomically under privacy admission so Protect/Lock/
/// Unprotect/Delete for any included note can linearize only before or after the
/// multi-note operation. Portable notebook export rejects protected notes under
/// the storage lock, so no Vault reader is required in this scope capability.
pub(crate) struct MultiNotePlaintextOperationPermit {
    _derivatives: Vec<PlaintextDerivativePermit>,
}

/// Storage serialization for an already-linearized plaintext operation. This
/// does not reacquire privacy admission; doing so would recreate the circular
/// wait that the compound permit exists to prevent.
pub(crate) struct PlaintextOperationMutationGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
    _authorization: PlaintextOperationPermit,
}

/// Canonical Restore boundary for one unlocked protected note. The coordinator
/// owns the exceptional ordering required by Restore: acquire the existing
/// Vault-session reader first, then admit the Restore transition, drain
/// plaintext producers, and serialize storage mutation.
///
/// The retained reader is intentionally returned after the Restore transition
/// ends so the caller may finish authorized post-Restore normalization without
/// reacquiring Vault access behind a concurrently admitted Lock.
pub(crate) struct ProtectedNoteRestoreGuard<'a> {
    access: VaultNoteAccess,
    _transition: NotePrivacyMutationGuard<'a>,
}

/// Storage serialization for an already-retained Vault authorization. This is
/// used only when a sensitive operation legitimately carries its original
/// reader across an earlier privacy mutation (currently revision Restore).
pub(crate) struct RetainedProtectedNoteMutationGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
}

/// Canonical boundary for ordinary mutations that retire plaintext derivatives.
///
/// This is intentionally not a privacy transition: it does not revoke Vault
/// readers or advance privacy generations. The coordinator still owns the
/// ordering between privacy admission, derivative drainage, and storage
/// serialization so callers cannot recreate that choreography themselves.
pub(crate) struct PlaintextDerivativeCleanupGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
    _derivative_transition: PlaintextDerivativeTransition,
}

/// A fully acquired whole-library privacy mutation boundary.
pub(crate) struct LibraryPrivacyMutationGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
    _vault_transition: VaultRevocationTransition,
    _derivative_transition: PlaintextDerivativeTransition,
    _admission: PrivacyAdmissionGuard,
}

enum PrivacyAdmissionScope {
    Note(NoteId),
    Notes(Vec<NoteId>),
    Library,
}

struct PrivacyAdmissionGuard {
    inner: Arc<PrivacyCoordinatorInner>,
    scope: PrivacyAdmissionScope,
    active: bool,
}

impl fmt::Debug for PrivacyCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PrivacyCoordinator")
    }
}

impl fmt::Debug for NotePrivacyMutationGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NotePrivacyMutationGuard([REDACTED])")
    }
}

impl fmt::Debug for ProtectedNoteMutationGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProtectedNoteMutationGuard([REDACTED])")
    }
}

impl fmt::Debug for ProtectedNotesMutationGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProtectedNotesMutationGuard([REDACTED])")
    }
}

impl fmt::Debug for PlaintextOperationPermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaintextOperationPermit([REDACTED])")
    }
}

impl fmt::Debug for PlaintextOperationMutationGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaintextOperationMutationGuard([REDACTED])")
    }
}

impl fmt::Debug for ProtectedNoteRestoreGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProtectedNoteRestoreGuard([REDACTED])")
    }
}

impl fmt::Debug for RetainedProtectedNoteMutationGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RetainedProtectedNoteMutationGuard([REDACTED])")
    }
}

impl fmt::Debug for PlaintextDerivativeCleanupGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaintextDerivativeCleanupGuard([REDACTED])")
    }
}

impl ProtectedNoteMutationGuard<'_> {
    pub(crate) fn access(&self) -> &VaultNoteAccess {
        &self.access
    }
}

impl ProtectedNotesMutationGuard<'_> {
    pub(crate) fn access_for(&self, note_id: &NoteId) -> Option<&VaultNoteAccess> {
        self.accesses
            .iter()
            .find(|access| access.permit().note_id() == note_id)
    }
}

impl PlaintextOperationPermit {
    pub(crate) fn note_id(&self) -> &NoteId {
        self._derivative.note_id()
    }

    pub(crate) fn access(&self) -> Option<&VaultNoteAccess> {
        self.access.as_ref()
    }

    pub(crate) fn vault_permit(&self) -> Option<&VaultReadPermit> {
        self.access.as_ref().map(VaultNoteAccess::permit)
    }
}

impl fmt::Debug for MultiNotePlaintextOperationPermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MultiNotePlaintextOperationPermit([REDACTED])")
    }
}

impl ProtectedNoteRestoreGuard<'_> {
    pub(crate) fn access(&self) -> &VaultNoteAccess {
        &self.access
    }

    /// End Restore admission while retaining the exact reader authorization
    /// that existed before Restore. Pre-Restore UI publication authorization
    /// remains invalid because privacy generation advancement is independent
    /// of this Vault reader lifetime.
    pub(crate) fn into_retained_access(self) -> VaultNoteAccess {
        let Self { access, _transition } = self;
        drop(_transition);
        access
    }
}

impl fmt::Debug for LibraryPrivacyMutationGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LibraryPrivacyMutationGuard([REDACTED])")
    }
}

impl Default for PrivacyCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyCoordinator {
    pub(crate) fn cleanup_stale_plaintext() -> crate::native::NativeResult<()> {
        VaultRuntime::cleanup_stale_plaintext()
    }

    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(PrivacyCoordinatorInner {
                admission: Mutex::new(PrivacyAdmissionState::default()),
                admission_changed: Condvar::new(),
                publication_drained: Condvar::new(),
                whole_library_operation_drained: Condvar::new(),
                operation_lock: Mutex::new(()),
                plaintext_derivatives: PlaintextDerivativeGate::default(),
                vault_runtime: Arc::new(VaultRuntime::default()),
            }),
        }
    }

    /// Ordinary storage mutations still serialize on the same mutex, but the
    /// mutex is owned here so privacy callers cannot assemble a second ordering.
    pub(crate) fn lock_storage(&self) -> Result<MutexGuard<'_, ()>, &'static str> {
        self.inner
            .operation_lock
            .lock()
            .map_err(|_| "storage operation lock is poisoned")
    }

    /// Acquire a producer/derivative permit for an ordinary or unlocked note.
    /// Privacy mutations gate this exact authority before they drain/revoke.
    #[cfg(test)]
    pub(crate) fn acquire_plaintext_derivative(
        &self,
        note_id: &NoteId,
    ) -> Result<PlaintextDerivativePermit, &'static str> {
        // Admission is the first privacy boundary. Once a same-note or
        // whole-library transition is admitted, no new plaintext producer may
        // enter behind it. Ordinary work waits for that transition to finish,
        // then re-authorizes against canonical state. Holding admission while
        // acquiring the derivative permit closes the admit/acquire gap.
        let state = self.wait_for_note_admission_clear(note_id)?;
        let permit = self.inner.plaintext_derivatives.acquire(note_id)?;
        drop(state);
        Ok(permit)
    }


    /// Atomically acquire every privacy authority required by a plaintext
    /// operation. Admission remains held while both the derivative gate and
    /// current Vault reader are captured, eliminating the historical
    /// derivative-then-reader deadlock window.
    pub(crate) fn acquire_plaintext_operation(
        &self,
        note_id: &NoteId,
    ) -> Result<PlaintextOperationPermit, &'static str> {
        let state = self.wait_for_note_admission_clear(note_id)?;
        let derivative = self.inner.plaintext_derivatives.acquire(note_id)?;
        let access = self.inner.vault_runtime.acquire_note_access(note_id)?;
        drop(state);
        Ok(PlaintextOperationPermit {
            access,
            _derivative: derivative,
        })
    }

    /// Atomically retain plaintext-derivative authority for a known note set.
    /// This is intentionally narrower than a whole-library operation lease: a
    /// notebook export blocks privacy transitions only for notes actually in its
    /// stabilized scope, leaving unrelated note transitions concurrent.
    pub(crate) fn acquire_multi_note_plaintext_operation(
        &self,
        note_ids: &[NoteId],
    ) -> Result<MultiNotePlaintextOperationPermit, &'static str> {
        let mut ids = note_ids.to_vec();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.dedup_by(|left, right| left.as_str() == right.as_str());
        if ids.is_empty() {
            return Err("multi-note plaintext operation requires at least one note");
        }
        let state = self.wait_for_notes_admission_clear(&ids)?;
        let derivatives = self.inner.plaintext_derivatives.acquire_notes(&ids)?;
        drop(state);
        Ok(MultiNotePlaintextOperationPermit {
            _derivatives: derivatives,
        })
    }

    /// Serialize storage work that is already covered by a compound plaintext
    /// operation permit. Re-entering admission or re-checking Vault revocation
    /// here would allow an admitted Lock/Protect to wait for the permit while
    /// this operation refused to finish, recreating a circular wait. The permit
    /// itself proves that this operation linearized before any later transition.
    pub(crate) fn begin_plaintext_operation_mutation(
        &self,
        permit: &PlaintextOperationPermit,
    ) -> Result<Option<PlaintextOperationMutationGuard<'_>>, &'static str> {
        let operation_guard = self.lock_storage()?;
        Ok(Some(PlaintextOperationMutationGuard {
            _operation_guard: operation_guard,
            _authorization: permit.clone(),
        }))
    }

    /// Drain and freeze plaintext derivative producers for one ordinary note,
    /// then serialize the storage mutation that retires those derivatives.
    /// Admission is held while the derivative gate is acquired so this work
    /// cannot slip behind an already-admitted privacy transition.
    pub(crate) fn begin_plaintext_derivative_cleanup(
        &self,
        note_id: &NoteId,
    ) -> Result<PlaintextDerivativeCleanupGuard<'_>, &'static str> {
        let state = self.wait_for_note_admission_clear(note_id)?;
        let derivative_transition = self.inner.plaintext_derivatives.begin_transition(note_id)?;
        drop(state);
        let operation_guard = self.lock_storage()?;
        Ok(PlaintextDerivativeCleanupGuard {
            _operation_guard: operation_guard,
            _derivative_transition: derivative_transition,
        })
    }

    /// Whole-library variant used by deferred attachment recovery. It is
    /// deliberately narrow: Vault readers remain untouched, but no plaintext
    /// derivative producer can race cache/canonical cleanup while the storage
    /// reconciliation pass runs.
    pub(crate) fn begin_library_plaintext_derivative_cleanup(
        &self,
    ) -> Result<PlaintextDerivativeCleanupGuard<'_>, &'static str> {
        let state = self.wait_for_all_admission_clear()?;
        let derivative_transition = self.inner.plaintext_derivatives.begin_library_transition()?;
        drop(state);
        let operation_guard = self.lock_storage()?;
        Ok(PlaintextDerivativeCleanupGuard {
            _operation_guard: operation_guard,
            _derivative_transition: derivative_transition,
        })
    }

    pub(crate) fn begin_note_transition(
        &self,
        note_id: &NoteId,
        transition: NotePrivacyTransition,
    ) -> Result<NotePrivacyMutationGuard<'_>, &'static str> {
        let admission = self.admit_note(note_id, transition)?;

        // Lock/Unprotect/Delete must publish Vault revocation before waiting on
        // compound derivative lifetimes. A protected native resource can then
        // observe revocation, finish with its already-held permits, and unblock
        // both drains without a storage-lock cycle. Protect has no Vault reader
        // to revoke; Unlock needs neither drain.
        let vault_transition = match transition {
            NotePrivacyTransition::Lock
            | NotePrivacyTransition::Unprotect
            | NotePrivacyTransition::Delete => {
                let (_removed, guard) = self.inner.vault_runtime.begin_note_revocation(note_id)?;
                Some(guard)
            }
            NotePrivacyTransition::Protect
            | NotePrivacyTransition::Unlock
            | NotePrivacyTransition::Restore => None,
        };

        let derivative_transition = if transition == NotePrivacyTransition::Unlock {
            None
        } else {
            Some(self.inner.plaintext_derivatives.begin_transition(note_id)?)
        };

        let operation_guard = self.lock_storage()?;
        Ok(NotePrivacyMutationGuard {
            _operation_guard: operation_guard,
            _vault_transition: vault_transition,
            _derivative_transition: derivative_transition,
            _admission: admission,
        })
    }

    /// Atomically admit a set of note mutations before draining any of them.
    /// Canonical sorting makes diagnostics/testing deterministic; admission is
    /// all-or-nothing, so an overlap cannot leave a partial multi-note boundary.
    pub(crate) fn begin_note_transitions(
        &self,
        note_ids: &[NoteId],
        transition: NotePrivacyTransition,
    ) -> Result<MultiNotePrivacyMutationGuard<'_>, &'static str> {
        let mut ids = note_ids.to_vec();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.dedup_by(|left, right| left.as_str() == right.as_str());
        if ids.is_empty() {
            return Err("privacy transition requires at least one note");
        }
        let admission = self.admit_notes(&ids, transition)?;

        let vault_transition = if matches!(
            transition,
            NotePrivacyTransition::Lock
                | NotePrivacyTransition::Unprotect
                | NotePrivacyTransition::Delete
        ) {
            let (_removed, guard) = self.inner.vault_runtime.begin_note_revocations(&ids)?;
            Some(guard)
        } else {
            None
        };

        let derivative_transition = if transition == NotePrivacyTransition::Unlock {
            None
        } else {
            Some(self.inner.plaintext_derivatives.begin_note_transitions(&ids)?)
        };

        let operation_guard = self.lock_storage()?;
        Ok(MultiNotePrivacyMutationGuard {
            _operation_guard: operation_guard,
            _vault_transition: vault_transition,
            _derivative_transition: derivative_transition,
            _admission: admission,
        })
    }

    pub(crate) fn begin_library_transition(
        &self,
        transition: LibraryPrivacyTransition,
    ) -> Result<LibraryPrivacyMutationGuard<'_>, &'static str> {
        let admission = self.admit_library(transition)?;
        let derivative_transition = self.inner.plaintext_derivatives.begin_library_transition()?;
        let (_removed, vault_transition) = self.inner.vault_runtime.begin_library_revocation()?;
        let operation_guard = self.lock_storage()?;
        Ok(LibraryPrivacyMutationGuard {
            _operation_guard: operation_guard,
            _vault_transition: vault_transition,
            _derivative_transition: derivative_transition,
            _admission: admission,
        })
    }

    pub(crate) fn insert_note_key(
        &self,
        note_id: NoteId,
        key: super::VaultKey,
    ) -> Result<(), &'static str> {
        self.inner.vault_runtime.insert_note_key(note_id, key)
    }

    /// Begin protected revision Restore without exposing its exceptional
    /// reader-before-transition ordering to storage callers. Admission and
    /// reader capture are linearized under the same admission mutex: a
    /// conflicting same-note/library transition is rejected immediately rather
    /// than queued, while no new conflicting transition can appear between the
    /// reader capture and Restore admission.
    pub(crate) fn begin_protected_note_restore(
        &self,
        note_id: &NoteId,
    ) -> Result<Option<ProtectedNoteRestoreGuard<'_>>, &'static str> {
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        if state.library_transition.is_some() || state.note_transitions.contains_key(note_id) {
            return Err("privacy transition is already in progress");
        }
        let Some(access) = self.inner.vault_runtime.acquire_note_access(note_id)? else {
            return Ok(None);
        };
        state
            .note_transitions
            .insert(note_id.clone(), NotePrivacyTransition::Restore);
        state.multi_note_publication_generation =
            state.multi_note_publication_generation.wrapping_add(1);
        let generation = state
            .note_publication_generations
            .entry(note_id.clone())
            .or_insert(0);
        *generation = generation.wrapping_add(1);
        while state.active_multi_note_publications != 0
            || state
                .active_note_publications
                .get(note_id)
                .copied()
                .unwrap_or(0)
                != 0
        {
            state = self
                .inner
                .publication_drained
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state = self.wait_for_whole_library_operations_to_drain(state);
        drop(state);

        let admission = PrivacyAdmissionGuard {
            inner: Arc::clone(&self.inner),
            scope: PrivacyAdmissionScope::Note(note_id.clone()),
            active: true,
        };
        let derivative_transition = self.inner.plaintext_derivatives.begin_transition(note_id)?;
        let operation_guard = self.lock_storage()?;
        let transition = NotePrivacyMutationGuard {
            _operation_guard: operation_guard,
            _vault_transition: None,
            _derivative_transition: Some(derivative_transition),
            _admission: admission,
        };
        Ok(Some(ProtectedNoteRestoreGuard {
            access,
            _transition: transition,
        }))
    }

    /// Serialize a mutation that must reuse an already-retained Vault reader.
    /// The coordinator owns the storage-lock boundary and rechecks revocation
    /// after acquiring it. A Lock that admitted first revokes this permit and
    /// makes the operation fail closed; a mutation that wins the storage lock
    /// finishes while the later Lock waits for the retained reader.
    pub(crate) fn begin_retained_protected_note_mutation(
        &self,
        access: &VaultNoteAccess,
    ) -> Result<Option<RetainedProtectedNoteMutationGuard<'_>>, &'static str> {
        let state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        let note_id = access.permit().note_id();
        if state.library_transition.is_some() || state.note_transitions.contains_key(note_id) {
            return Ok(None);
        }
        if !self.inner.vault_runtime.read_permit_is_publishable(access.permit())? {
            return Ok(None);
        }

        // Keep admission held through storage serialization. This linearizes
        // the retained mutation against Lock: whichever side owns admission
        // first wins, and the losing mutation never waits behind the reader it
        // would itself need to release.
        let operation_guard = self.lock_storage()?;
        if !self.inner.vault_runtime.read_permit_is_publishable(access.permit())? {
            drop(operation_guard);
            return Ok(None);
        }
        drop(state);
        Ok(Some(RetainedProtectedNoteMutationGuard {
            _operation_guard: operation_guard,
        }))
    }

    /// Begin one protected-note storage mutation without exposing lock ordering
    /// to the caller. Returns `None` when the note has no active Vault key.
    pub(crate) fn begin_protected_note_mutation(
        &self,
        note_id: &NoteId,
    ) -> Result<Option<ProtectedNoteMutationGuard<'_>>, &'static str> {
        let state = self.wait_for_note_admission_clear(note_id)?;
        let Some(access) = self.inner.vault_runtime.acquire_note_access(note_id)? else {
            drop(state);
            return Ok(None);
        };
        // Keep admission linearized through storage serialization. A transition
        // cannot revoke this reader between capture and mutation ownership.
        let operation_guard = self.lock_storage()?;
        drop(state);
        Ok(Some(ProtectedNoteMutationGuard {
            _operation_guard: operation_guard,
            access,
        }))
    }

    /// Begin one storage mutation spanning a known set of protected notes. All
    /// Vault authorizations are acquired before the storage mutex. If any note
    /// is locked, no storage mutation begins and all acquired permits are
    /// released.
    pub(crate) fn begin_protected_notes_mutation(
        &self,
        note_ids: &[NoteId],
    ) -> Result<Option<ProtectedNotesMutationGuard<'_>>, &'static str> {
        let mut ids = note_ids.to_vec();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.dedup_by(|left, right| left.as_str() == right.as_str());
        if ids.is_empty() {
            return Err("protected-note mutation requires at least one note");
        }

        // One admission critical section covers the complete set. No library or
        // overlapping note transition can be admitted after reader A is held
        // but before reader B is captured. Storage serialization is acquired
        // before releasing admission for the same reason.
        let state = self.wait_for_notes_admission_clear(&ids)?;
        let mut accesses = Vec::with_capacity(ids.len());
        for note_id in &ids {
            let Some(access) = self.inner.vault_runtime.acquire_note_access(note_id)? else {
                drop(state);
                return Ok(None);
            };
            accesses.push(access);
        }
        let operation_guard = self.lock_storage()?;
        drop(state);
        Ok(Some(ProtectedNotesMutationGuard {
            _operation_guard: operation_guard,
            accesses,
        }))
    }

    pub(crate) fn acquire_note_access(
        &self,
        note_id: &NoteId,
    ) -> Result<Option<VaultNoteAccess>, &'static str> {
        // Same linearization rule as plaintext producers: an admitted privacy
        // mutation owns the note boundary before revocation/drain work starts,
        // so a new Vault reader cannot slip into that interval.
        let state = self.wait_for_note_admission_clear(note_id)?;
        let access = self.inner.vault_runtime.acquire_note_access(note_id)?;
        drop(state);
        Ok(access)
    }

    pub(crate) fn read_permit_is_publishable(
        &self,
        permit: &VaultReadPermit,
    ) -> Result<bool, &'static str> {
        self.inner.vault_runtime.read_permit_is_publishable(permit)
    }

    pub(crate) fn materialized_attachment(
        &self,
        permit: &VaultReadPermit,
        note_id: &NoteId,
        attachment_id: &str,
    ) -> Result<Option<std::path::PathBuf>, &'static str> {
        self.inner
            .vault_runtime
            .materialized_attachment(permit, note_id, attachment_id)
    }

    pub(crate) fn allocate_vault_plaintext_path(&self) -> std::io::Result<std::path::PathBuf> {
        self.inner.vault_runtime.allocate_attachment_path()
    }

    /// Begin a whole-library read/durability barrier without reusing UI
    /// publication authority. Capture is atomic with transition admission: if a
    /// note/library privacy transition is already admitted, callers receive
    /// `None`; if this lease begins first, a later transition waits for it to
    /// drain before crossing the privacy boundary.
    pub(crate) fn begin_whole_library_operation(
        &self,
    ) -> Result<Option<WholeLibraryOperationLease>, &'static str> {
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        if state.library_transition.is_some() || !state.note_transitions.is_empty() {
            return Ok(None);
        }
        state.active_whole_library_operations = state
            .active_whole_library_operations
            .checked_add(1)
            .ok_or("whole-library privacy operation lease count overflow")?;
        Ok(Some(WholeLibraryOperationLease {
            inner: Arc::clone(&self.inner),
            active: true,
        }))
    }

    /// Capture note-local publication authority. An unrelated note transition
    /// does not invalidate this authorization; a transition for this note or a
    /// whole-library transition does.
    pub(crate) fn capture_note_publication_authorization(
        &self,
        note_id: &NoteId,
    ) -> Result<Option<PrivacyPublicationAuthorization>, &'static str> {
        let state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        if state.library_transition.is_some() || state.note_transitions.contains_key(note_id) {
            return Ok(None);
        }
        Ok(Some(PrivacyPublicationAuthorization {
            inner: Arc::clone(&self.inner),
            scope: PrivacyPublicationScope::Note {
                note_id: note_id.clone(),
                library_generation: state.library_publication_generation,
                note_generation: state
                    .note_publication_generations
                    .get(note_id)
                    .copied()
                    .unwrap_or(0),
            },
        }))
    }

    /// Capture publication authority for a surface that may contain data from
    /// multiple notes. Any note transition invalidates this scope.
    pub(crate) fn capture_multi_note_publication_authorization(
        &self,
    ) -> Result<Option<PrivacyPublicationAuthorization>, &'static str> {
        let state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        if state.library_transition.is_some() || !state.note_transitions.is_empty() {
            return Ok(None);
        }
        Ok(Some(PrivacyPublicationAuthorization {
            inner: Arc::clone(&self.inner),
            scope: PrivacyPublicationScope::MultiNote {
                library_generation: state.library_publication_generation,
                multi_note_generation: state.multi_note_publication_generation,
            },
        }))
    }

    #[cfg(test)]
    pub(crate) fn note_publication_generation(&self, note_id: &NoteId) -> u64 {
        self.inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .note_publication_generations
            .get(note_id)
            .copied()
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn multi_note_publication_generation(&self) -> u64 {
        self.inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .multi_note_publication_generation
    }

    /// Returns whether the note currently has an active, non-revoking Vault
    /// session. This keeps production callers on PrivacyCoordinator authority
    /// without exposing test-only key-presence helpers.
    pub(crate) fn note_session_is_active(&self, note_id: &NoteId) -> Result<bool, &'static str> {
        self.inner.vault_runtime.note_session_is_active(note_id)
    }

    #[cfg(any(test, feature = "internal-test-support"))]
    pub(crate) fn contains_note_key(&self, note_id: &NoteId) -> Result<bool, &'static str> {
        self.note_session_is_active(note_id)
    }

    #[cfg(test)]
    fn active_vault_permit_count(&self, note_id: &NoteId) -> Result<usize, &'static str> {
        self.inner.vault_runtime.active_permit_count(note_id)
    }


    fn wait_for_all_admission_clear(
        &self,
    ) -> Result<MutexGuard<'_, PrivacyAdmissionState>, &'static str> {
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        while state.library_transition.is_some() || !state.note_transitions.is_empty() {
            state = self
                .inner
                .admission_changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        Ok(state)
    }

    fn wait_for_notes_admission_clear(
        &self,
        note_ids: &[NoteId],
    ) -> Result<MutexGuard<'_, PrivacyAdmissionState>, &'static str> {
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        while state.library_transition.is_some()
            || note_ids
                .iter()
                .any(|note_id| state.note_transitions.contains_key(note_id))
        {
            state = self
                .inner
                .admission_changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        Ok(state)
    }

    fn wait_for_note_admission_clear(
        &self,
        note_id: &NoteId,
    ) -> Result<MutexGuard<'_, PrivacyAdmissionState>, &'static str> {
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        while state.library_transition.is_some() || state.note_transitions.contains_key(note_id) {
            state = self
                .inner
                .admission_changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        Ok(state)
    }

    fn wait_for_whole_library_operations_to_drain<'a>(
        &self,
        mut state: MutexGuard<'a, PrivacyAdmissionState>,
    ) -> MutexGuard<'a, PrivacyAdmissionState> {
        while state.active_whole_library_operations != 0 {
            state = self
                .inner
                .whole_library_operation_drained
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state
    }

    fn admit_note(
        &self,
        note_id: &NoteId,
        transition: NotePrivacyTransition,
    ) -> Result<PrivacyAdmissionGuard, &'static str> {
        self.admit_notes(std::slice::from_ref(note_id), transition)
    }

    fn admit_notes(
        &self,
        note_ids: &[NoteId],
        transition: NotePrivacyTransition,
    ) -> Result<PrivacyAdmissionGuard, &'static str> {
        if note_ids.is_empty() {
            return Err("privacy transition requires at least one note");
        }
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        if state.library_transition.is_some()
            || note_ids.iter().any(|note_id| state.note_transitions.contains_key(note_id))
        {
            return Err("privacy transition is already in progress");
        }
        for note_id in note_ids {
            state.note_transitions.insert(note_id.clone(), transition);
        }
        // A note transition invalidates publication only for the affected notes
        // plus aggregate/multi-note surfaces. Unrelated note-local publication
        // remains current and is never drained here.
        state.multi_note_publication_generation =
            state.multi_note_publication_generation.wrapping_add(1);
        for note_id in note_ids {
            let generation = state
                .note_publication_generations
                .entry(note_id.clone())
                .or_insert(0);
            *generation = generation.wrapping_add(1);
        }
        while state.active_multi_note_publications != 0
            || note_ids.iter().any(|note_id| {
                state
                    .active_note_publications
                    .get(note_id)
                    .copied()
                    .unwrap_or(0)
                    != 0
            })
        {
            state = self
                .inner
                .publication_drained
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state = self.wait_for_whole_library_operations_to_drain(state);
        drop(state);
        Ok(PrivacyAdmissionGuard {
            inner: Arc::clone(&self.inner),
            scope: if note_ids.len() == 1 {
                PrivacyAdmissionScope::Note(note_ids[0].clone())
            } else {
                PrivacyAdmissionScope::Notes(note_ids.to_vec())
            },
            active: true,
        })
    }

    fn admit_library(
        &self,
        transition: LibraryPrivacyTransition,
    ) -> Result<PrivacyAdmissionGuard, &'static str> {
        let mut state = self
            .inner
            .admission
            .lock()
            .map_err(|_| "privacy coordinator admission lock is poisoned")?;
        if state.library_transition.is_some() || !state.note_transitions.is_empty() {
            return Err("privacy transition is already in progress");
        }
        state.library_transition = Some(transition);
        // Whole-library replacement invalidates every publication scope and
        // drains all outstanding note-local and multi-note handoffs.
        state.library_publication_generation =
            state.library_publication_generation.wrapping_add(1);
        state.multi_note_publication_generation =
            state.multi_note_publication_generation.wrapping_add(1);
        while state.active_multi_note_publications != 0
            || state.active_note_publications.values().any(|count| *count != 0)
        {
            state = self
                .inner
                .publication_drained
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state = self.wait_for_whole_library_operations_to_drain(state);
        drop(state);
        Ok(PrivacyAdmissionGuard {
            inner: Arc::clone(&self.inner),
            scope: PrivacyAdmissionScope::Library,
            active: true,
        })
    }
}

/// Multi-note variant used by purge/rollback. It is still one coordinator-owned
/// boundary and cannot be reconstructed from public component guards.
pub(crate) struct MultiNotePrivacyMutationGuard<'a> {
    _operation_guard: MutexGuard<'a, ()>,
    _vault_transition: Option<VaultRevocationTransition>,
    _derivative_transition: Option<PlaintextDerivativeTransition>,
    _admission: PrivacyAdmissionGuard,
}

impl Drop for PrivacyAdmissionGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut state = self
            .inner
            .admission
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &self.scope {
            PrivacyAdmissionScope::Note(note_id) => {
                state.note_transitions.remove(note_id);
            }
            PrivacyAdmissionScope::Notes(note_ids) => {
                for note_id in note_ids {
                    state.note_transitions.remove(note_id);
                }
            }
            PrivacyAdmissionScope::Library => {
                state.library_transition = None;
            }
        }
        self.active = false;
        self.inner.admission_changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{Argon2idParams, VaultContext, VaultDomain, VaultService};
    use std::{sync::mpsc, thread, time::Duration};

    fn note(label: &str) -> NoteId {
        NoteId::parse(format!("note-{label}")).expect("note id")
    }

    fn install_key(coordinator: &PrivacyCoordinator, note_id: &NoteId) {
        let vault = VaultService::new();
        let key = vault.generate_key().expect("key");
        coordinator
            .insert_note_key(note_id.clone(), key)
            .expect("install key");
    }

    fn wait_for_multi_note_generation_advance(
        coordinator: &PrivacyCoordinator,
        before: u64,
        label: &str,
    ) {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while coordinator.multi_note_publication_generation() == before && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert_ne!(
            coordinator.multi_note_publication_generation(),
            before,
            "{label} must be admitted",
        );
    }

    fn wait_for_note_generation_advance(
        coordinator: &PrivacyCoordinator,
        note_id: &NoteId,
        before: u64,
        label: &str,
    ) {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while coordinator.note_publication_generation(note_id) == before
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
        assert_ne!(
            coordinator.note_publication_generation(note_id),
            before,
            "{label} must advance the affected note publication generation",
        );
    }

    #[test]
    fn same_note_transition_conflicts_fail_closed() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("same-note-conflict");
        let first = coordinator
            .begin_note_transition(&note_id, NotePrivacyTransition::Protect)
            .expect("first transition");
        assert!(
            coordinator
                .begin_note_transition(&note_id, NotePrivacyTransition::Unlock)
                .is_err(),
            "a conflicting same-note mutation must be rejected rather than queued"
        );
        drop(first);
        assert!(
            coordinator
                .begin_note_transition(&note_id, NotePrivacyTransition::Unlock)
                .is_ok()
        );
    }

    #[test]
    fn overlapping_multi_note_transition_fails_without_partial_admission() {
        let coordinator = PrivacyCoordinator::new();
        let free_note = note("multi-free");
        let busy_note = note("multi-busy");
        let busy_guard = coordinator
            .begin_note_transition(&busy_note, NotePrivacyTransition::Protect)
            .expect("busy note transition");

        assert!(
            coordinator
                .begin_note_transitions(
                    &[free_note.clone(), busy_note.clone()],
                    NotePrivacyTransition::Delete,
                )
                .is_err(),
            "an overlapping multi-note mutation must fail rather than partially reserve its free notes"
        );

        // `busy_guard` intentionally owns the coordinator's global storage
        // mutation mutex. Drop it before probing the free note; otherwise this
        // test would block on storage serialization rather than test admission
        // rollback. If the failed multi-note admission had partially reserved
        // `free_note`, the next admission would still fail after `busy_guard`
        // is gone.
        drop(busy_guard);
        let free_guard = coordinator
            .begin_note_transition(&free_note, NotePrivacyTransition::Unlock)
            .expect("free note must remain unreserved after failed multi-note admission");
        drop(free_guard);
    }

    #[test]
    fn library_and_note_transitions_are_mutually_exclusive() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("library-note-conflict");
        let note_guard = coordinator
            .begin_note_transition(&note_id, NotePrivacyTransition::Protect)
            .expect("note transition");
        assert!(
            coordinator
                .begin_library_transition(LibraryPrivacyTransition::Restore)
                .is_err()
        );
        drop(note_guard);
        let library_guard = coordinator
            .begin_library_transition(LibraryPrivacyTransition::Restore)
            .expect("library transition");
        assert!(
            coordinator
                .begin_note_transition(&note_id, NotePrivacyTransition::Unlock)
                .is_err()
        );
        drop(library_guard);
    }

    #[test]
    fn restore_is_a_same_note_conflicting_transition() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("restore-conflict");
        let restore = coordinator
            .begin_note_transition(&note_id, NotePrivacyTransition::Restore)
            .expect("restore transition");
        assert!(
            coordinator
                .begin_note_transition(&note_id, NotePrivacyTransition::Lock)
                .is_err(),
            "Lock must fail closed rather than queue behind an admitted Restore"
        );
        assert!(
            coordinator
                .begin_note_transition(&note_id, NotePrivacyTransition::Unprotect)
                .is_err(),
            "Unprotect must fail closed rather than queue behind an admitted Restore"
        );
        assert!(
            coordinator
                .begin_library_transition(LibraryPrivacyTransition::Restore)
                .is_err(),
            "whole-library Restore must exclude an admitted note Restore"
        );
        drop(restore);
    }

    #[test]
    fn protected_restore_rejects_existing_same_note_transition_without_waiting() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("protected-restore-fail-closed");
        install_key(&coordinator, &note_id);
        let active = coordinator
            .begin_note_transition(&note_id, NotePrivacyTransition::Unlock)
            .expect("active same-note transition");
        assert!(
            coordinator.begin_protected_note_restore(&note_id).is_err(),
            "protected Restore must reject a same-note conflict instead of queuing behind it"
        );
        drop(active);
        assert!(
            coordinator
                .begin_protected_note_restore(&note_id)
                .expect("Restore after conflict releases")
                .is_some()
        );
    }

    #[test]
    fn admitted_lock_blocks_new_reader_before_vault_revocation_begins() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("admission-reader-gap");
        install_key(&coordinator, &note_id);
        let authorization = coordinator
            .capture_multi_note_publication_authorization()
            .expect("capture")
            .expect("authorization");
        let publication = authorization
            .begin_publication()
            .expect("lease")
            .expect("publishable");

        let lock_worker = coordinator.clone();
        let lock_note = note_id.clone();
        let (lock_done_tx, lock_done_rx) = mpsc::channel();
        let lock_thread = thread::spawn(move || {
            let guard = lock_worker
                .begin_note_transition(&lock_note, NotePrivacyTransition::Lock)
                .expect("Lock after publication drain");
            drop(guard);
            lock_done_tx.send(()).expect("lock done");
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while authorization.is_current().expect("authorization state")
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(
            !authorization.is_current().expect("invalidated"),
            "Lock must be admitted before this test probes new reader admission"
        );

        let reader_worker = coordinator.clone();
        let reader_note = note_id.clone();
        let (reader_tx, reader_rx) = mpsc::channel();
        let reader_thread = thread::spawn(move || {
            let access = reader_worker
                .acquire_note_access(&reader_note)
                .expect("reader acquisition after transition");
            reader_tx.send(access.is_some()).expect("reader result");
        });
        assert!(
            reader_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "a reader must not enter after Lock admission while Lock is still draining publication"
        );
        assert!(lock_done_rx.try_recv().is_err());

        drop(publication);
        lock_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Lock completes after publication release");
        assert!(
            !reader_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("reader re-authorizes after Lock"),
            "the post-Lock reader must observe the removed note key"
        );
        lock_thread.join().expect("lock thread");
        reader_thread.join().expect("reader thread");
    }

    #[test]
    fn admitted_protect_blocks_new_plaintext_producer_before_derivative_drain() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("admission-producer-gap");
        let authorization = coordinator
            .capture_multi_note_publication_authorization()
            .expect("capture")
            .expect("authorization");
        let publication = authorization
            .begin_publication()
            .expect("lease")
            .expect("publishable");

        let transition_worker = coordinator.clone();
        let transition_note = note_id.clone();
        let (transition_done_tx, transition_done_rx) = mpsc::channel();
        let transition_thread = thread::spawn(move || {
            let guard = transition_worker
                .begin_note_transition(&transition_note, NotePrivacyTransition::Protect)
                .expect("Protect after publication drain");
            drop(guard);
            transition_done_tx.send(()).expect("transition done");
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while authorization.is_current().expect("authorization state")
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(!authorization.is_current().expect("invalidated"));

        let producer_worker = coordinator.clone();
        let producer_note = note_id.clone();
        let (producer_tx, producer_rx) = mpsc::channel();
        let producer_thread = thread::spawn(move || {
            let _permit = producer_worker
                .acquire_plaintext_derivative(&producer_note)
                .expect("producer re-authorized after transition");
            producer_tx.send(()).expect("producer ready");
        });
        assert!(
            producer_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "a plaintext producer must not enter after Protect admission"
        );
        assert!(transition_done_rx.try_recv().is_err());

        drop(publication);
        transition_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Protect completes after publication release");
        producer_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("producer may re-authorize only after Protect boundary releases");
        transition_thread.join().expect("transition thread");
        producer_thread.join().expect("producer thread");
    }

    #[test]
    fn derivative_cleanup_drains_existing_producer_and_blocks_new_producer() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("derivative-cleanup-boundary");
        let producer = coordinator
            .acquire_plaintext_derivative(&note_id)
            .expect("initial derivative producer");

        let cleanup_worker = coordinator.clone();
        let cleanup_note = note_id.clone();
        let (cleanup_ready_tx, cleanup_ready_rx) = mpsc::channel();
        let (release_cleanup_tx, release_cleanup_rx) = mpsc::channel();
        let cleanup_thread = thread::spawn(move || {
            let guard = cleanup_worker
                .begin_plaintext_derivative_cleanup(&cleanup_note)
                .expect("derivative cleanup after producer drain");
            cleanup_ready_tx.send(()).expect("cleanup ready");
            release_cleanup_rx.recv().expect("release cleanup");
            drop(guard);
        });

        assert!(
            cleanup_ready_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "cleanup must wait for an already-running derivative producer"
        );
        drop(producer);
        cleanup_ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("cleanup enters after producer drains");

        let producer_worker = coordinator.clone();
        let producer_note = note_id.clone();
        let (producer_ready_tx, producer_ready_rx) = mpsc::channel();
        let producer_thread = thread::spawn(move || {
            let _permit = producer_worker
                .acquire_plaintext_derivative(&producer_note)
                .expect("producer after cleanup");
            producer_ready_tx.send(()).expect("producer ready");
        });
        assert!(
            producer_ready_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "a new derivative producer must remain blocked until cleanup releases"
        );

        release_cleanup_tx.send(()).expect("release cleanup guard");
        producer_ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("producer enters after cleanup releases");
        cleanup_thread.join().expect("cleanup thread");
        producer_thread.join().expect("producer thread");
    }

    #[test]
    fn compound_plaintext_operation_finishes_after_lock_admission_without_circular_wait() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("compound-lock-linearization");
        install_key(&coordinator, &note_id);
        let operation = coordinator
            .acquire_plaintext_operation(&note_id)
            .expect("compound operation authorization");
        assert!(operation.access().is_some(), "protected operation retains Vault access");
        let before = coordinator.note_publication_generation(&note_id);

        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock after compound authorization");
            drop(guard);
            done_tx.send(()).expect("Lock done");
        });

        wait_for_note_generation_advance(&coordinator, &note_id, before, "Lock");

        let mutation = coordinator
            .begin_plaintext_operation_mutation(&operation)
            .expect("compound mutation check")
            .expect("pre-admission compound operation remains authorized to finish");
        assert!(
            done_rx.try_recv().is_err(),
            "Lock must still wait for the complete compound plaintext operation"
        );
        drop(mutation);
        assert!(
            done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "releasing storage alone must not release the derivative/Vault lifetime"
        );
        drop(operation);
        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("Lock completes after compound authorization releases");
        handle.join().expect("Lock thread");
    }

    #[test]
    fn multi_note_plaintext_operation_blocks_only_included_transitions() {
        let coordinator = PrivacyCoordinator::new();
        let note_a = note("multi-plaintext-a");
        let note_b = note("multi-plaintext-b");
        let note_c = note("multi-plaintext-c");
        let operation = coordinator
            .acquire_multi_note_plaintext_operation(&[note_a.clone(), note_b.clone()])
            .expect("multi-note plaintext operation");
        let before_b = coordinator.note_publication_generation(&note_b);

        let b_worker = coordinator.clone();
        let b_note = note_b.clone();
        let (b_done_tx, b_done_rx) = mpsc::channel();
        let b_handle = thread::spawn(move || {
            let guard = b_worker
                .begin_note_transition(&b_note, NotePrivacyTransition::Protect)
                .expect("Protect B after multi-note plaintext operation");
            drop(guard);
            b_done_tx.send(()).expect("Protect B done");
        });
        wait_for_note_generation_advance(&coordinator, &note_b, before_b, "Protect B");
        assert!(
            b_done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "included B transition must wait for the multi-note plaintext lifetime",
        );

        let c_worker = coordinator.clone();
        let c_note = note_c.clone();
        let (c_done_tx, c_done_rx) = mpsc::channel();
        let c_handle = thread::spawn(move || {
            let guard = c_worker
                .begin_note_transition(&c_note, NotePrivacyTransition::Protect)
                .expect("unrelated Protect C");
            drop(guard);
            c_done_tx.send(()).expect("Protect C done");
        });
        c_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("unrelated C transition remains concurrent");
        c_handle.join().expect("Protect C thread");

        drop(operation);
        b_done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("Protect B completes after multi-note plaintext lifetime releases");
        b_handle.join().expect("Protect B thread");
    }

    #[test]
    fn ordinary_compound_plaintext_operation_finishes_before_admitted_protect() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("compound-protect-linearization");
        let operation = coordinator
            .acquire_plaintext_operation(&note_id)
            .expect("ordinary compound authorization");
        assert!(operation.access().is_none(), "ordinary note has no Vault reader");
        let before = coordinator.note_publication_generation(&note_id);

        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Protect)
                .expect("Protect after ordinary compound authorization");
            drop(guard);
            done_tx.send(()).expect("Protect done");
        });

        wait_for_note_generation_advance(&coordinator, &note_id, before, "Protect");
        let mutation = coordinator
            .begin_plaintext_operation_mutation(&operation)
            .expect("ordinary compound mutation")
            .expect("pre-admission ordinary operation remains authorized to finish");
        drop(mutation);
        assert!(done_rx.try_recv().is_err(), "Protect still waits for derivative lifetime");
        drop(operation);
        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("Protect completes after compound authorization releases");
        handle.join().expect("Protect thread");
    }

    #[test]
    fn multi_protected_mutation_acquires_entire_reader_set_before_transition_can_admit() {
        let coordinator = PrivacyCoordinator::new();
        let first = note("multi-linearized-a");
        let second = note("multi-linearized-b");
        install_key(&coordinator, &first);
        install_key(&coordinator, &second);

        // Hold storage so the worker is forced to pause after acquiring the
        // complete protected-note reader set while still inside admission.
        let storage = coordinator.lock_storage().expect("hold storage");
        let worker = coordinator.clone();
        let worker_first = first.clone();
        let worker_second = second.clone();
        let (mutation_tx, mutation_rx) = mpsc::channel();
        let mutation_thread = thread::spawn(move || {
            let guard = worker
                .begin_protected_notes_mutation(&[worker_first, worker_second])
                .expect("multi-note mutation")
                .expect("both notes unlocked");
            mutation_tx.send(()).expect("mutation acquired");
            thread::sleep(Duration::from_millis(100));
            drop(guard);
        });

        // Wait until the worker has captured the *complete* reader set. It is
        // then blocked only on storage while still holding admission. A library
        // transition must not be able to admit between reader A and reader B.
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while (coordinator.active_vault_permit_count(&first).expect("first permit count") != 1
            || coordinator.active_vault_permit_count(&second).expect("second permit count") != 1)
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert_eq!(
            coordinator.active_vault_permit_count(&first).expect("first permit count"),
            1,
            "first reader must be acquired before testing transition admission",
        );
        assert_eq!(
            coordinator.active_vault_permit_count(&second).expect("second permit count"),
            1,
            "second reader must be acquired in the same admission critical section",
        );
        assert!(
            coordinator.inner.admission.try_lock().is_err(),
            "set-wise reader acquisition must retain admission while waiting for storage",
        );
        let transition_worker = coordinator.clone();
        let (attempt_tx, attempt_rx) = mpsc::channel();
        let (restore_tx, restore_rx) = mpsc::channel();
        let restore_thread = thread::spawn(move || {
            attempt_tx.send(()).expect("Restore attempt started");
            let guard = transition_worker
                .begin_library_transition(LibraryPrivacyTransition::Restore)
                .expect("library Restore after multi-note mutation");
            drop(guard);
            restore_tx.send(()).expect("Restore done");
        });
        attempt_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Restore thread reaches admission attempt");
        assert!(
            restore_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "library Restore must not complete while set-wise acquisition owns admission",
        );
        assert!(
            coordinator.inner.admission.try_lock().is_err(),
            "library Restore cannot take admission before the complete reader set reaches storage",
        );

        drop(storage);
        mutation_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("multi-note mutation acquires storage");
        restore_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Restore completes after complete reader set releases");
        mutation_thread.join().expect("mutation thread");
        restore_thread.join().expect("Restore thread");
    }

    #[test]
    fn protected_note_mutation_guard_owns_reader_and_storage_lifetime() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("protected-mutation-boundary");
        install_key(&coordinator, &note_id);
        let mutation = coordinator
            .begin_protected_note_mutation(&note_id)
            .expect("protected mutation acquisition")
            .expect("unlocked protected note");

        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock after protected mutation");
            drop(guard);
            done_tx.send(()).expect("Lock done");
        });

        assert!(
            done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Lock must wait while the canonical protected mutation guard retains Vault and storage authorization"
        );
        drop(mutation);
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Lock completes after protected mutation releases");
        handle.join().expect("Lock thread");
        assert!(!coordinator.contains_note_key(&note_id).expect("key state"));
    }

    #[test]
    fn protected_restore_guard_owns_restore_admission_and_retained_reader_lifetime() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("protected-restore-boundary");
        install_key(&coordinator, &note_id);
        let restore = coordinator
            .begin_protected_note_restore(&note_id)
            .expect("protected restore acquisition")
            .expect("unlocked protected note");

        assert!(
            coordinator
                .begin_note_transition(&note_id, NotePrivacyTransition::Lock)
                .is_err(),
            "Lock must fail closed while Restore admission is held"
        );

        let retained = restore.into_retained_access();
        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock after Restore transition release");
            drop(guard);
            done_tx.send(()).expect("Lock done");
        });
        assert!(
            done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Lock must wait for the exact retained Restore reader after Restore admission releases"
        );
        drop(retained);
        done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Lock completes after retained Restore reader releases");
        handle.join().expect("Lock thread");
    }

    #[test]
    fn retained_mutation_fails_closed_when_lock_admitted_first() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("retained-mutation-lock-race");
        install_key(&coordinator, &note_id);
        let access = coordinator
            .acquire_note_access(&note_id)
            .expect("reader")
            .expect("unlocked note");
        let before = coordinator.multi_note_publication_generation();

        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock");
            drop(guard);
            done_tx.send(()).expect("Lock done");
        });

        wait_for_multi_note_generation_advance(&coordinator, before, "Lock");
        assert!(
            coordinator
                .begin_retained_protected_note_mutation(&access)
                .expect("retained mutation check")
                .is_none(),
            "retained mutation must fail closed once its Vault permit has been revoked"
        );
        assert!(done_rx.try_recv().is_err(), "Lock still waits for retained reader");
        drop(access);
        done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("Lock completes after retained reader releases");
        handle.join().expect("Lock thread");
    }

    #[test]
    fn multi_protected_mutation_failure_releases_partial_authorization() {
        let coordinator = PrivacyCoordinator::new();
        let unlocked = note("protected-mutation-unlocked");
        let locked = note("protected-mutation-locked");
        install_key(&coordinator, &unlocked);

        assert!(
            coordinator
                .begin_protected_notes_mutation(&[unlocked.clone(), locked])
                .expect("multi protected mutation acquisition")
                .is_none(),
            "a multi-note protected mutation must fail before storage mutation if any target is locked"
        );

        let guard = coordinator
            .begin_note_transition(&unlocked, NotePrivacyTransition::Lock)
            .expect("partial Vault authorization must have been released");
        drop(guard);
    }

    #[test]
    fn lock_waits_for_existing_reader_before_storage_mutation() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("reader-lock");
        install_key(&coordinator, &note_id);
        let access = coordinator
            .acquire_note_access(&note_id)
            .expect("access")
            .expect("unlocked");
        let permit = access.into_permit();

        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            started_tx.send(()).expect("started");
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("lock transition");
            ready_tx.send(()).expect("ready");
            drop(guard);
        });
        started_rx.recv().expect("started");
        assert!(ready_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(permit);
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("lock completed after reader drain");
        thread.join().expect("thread");
        assert!(!coordinator.contains_note_key(&note_id).expect("key state"));
    }

    #[test]
    fn lock_invalidates_publication_and_reader_authorization_before_reader_drain_completes() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("reader-publication-lock");
        install_key(&coordinator, &note_id);
        let access = coordinator
            .acquire_note_access(&note_id)
            .expect("access")
            .expect("unlocked");
        let permit = access.into_permit();
        let publication = coordinator
            .capture_multi_note_publication_authorization()
            .expect("publication capture")
            .expect("no active transition");

        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("lock transition");
            ready_tx.send(()).expect("lock ready");
            drop(guard);
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while publication.is_current().expect("publication state")
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(
            !publication.is_current().expect("publication invalidated"),
            "Lock admission must invalidate the UI publication authorization before reader drain"
        );

        while coordinator
            .read_permit_is_publishable(&permit)
            .expect("reader publication state")
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(
            !coordinator
                .read_permit_is_publishable(&permit)
                .expect("reader invalidated"),
            "Lock must revoke the retained Vault reader before waiting for it to drain"
        );
        assert!(
            ready_rx.try_recv().is_err(),
            "Lock must still be waiting while the retained reader permit is alive"
        );

        drop(permit);
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Lock completes after reader release");
        handle.join().expect("lock worker");
    }

    #[test]
    fn protect_waits_for_plaintext_producer_and_blocks_new_producer() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("producer-protect");
        let permit = coordinator
            .acquire_plaintext_derivative(&note_id)
            .expect("producer permit");
        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            started_tx.send(()).expect("started");
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Protect)
                .expect("protect transition");
            ready_tx.send(()).expect("ready");
            release_rx.recv().expect("release");
            drop(guard);
        });
        started_rx.recv().expect("started");
        assert!(ready_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(permit);
        ready_rx.recv_timeout(Duration::from_secs(2)).expect("protect ready");

        let blocked = coordinator.clone();
        let blocked_note = note_id.clone();
        let (producer_ready_tx, producer_ready_rx) = mpsc::channel();
        let producer = thread::spawn(move || {
            let _permit = blocked
                .acquire_plaintext_derivative(&blocked_note)
                .expect("post-transition producer");
            producer_ready_tx.send(()).expect("producer ready");
        });
        assert!(producer_ready_rx.recv_timeout(Duration::from_millis(50)).is_err());
        release_tx.send(()).expect("release transition");
        producer_ready_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("producer reauthorized after protect boundary");
        thread.join().expect("transition thread");
        producer.join().expect("producer thread");
    }

    #[test]
    fn lock_b_leaves_note_a_publication_valid_and_undrained() {
        let coordinator = PrivacyCoordinator::new();
        let note_a = note("publication-unrelated-a");
        let note_b = note("publication-unrelated-b");
        install_key(&coordinator, &note_b);

        let authorization_a = coordinator
            .capture_note_publication_authorization(&note_a)
            .expect("capture A")
            .expect("A publishable");
        let lease_a = authorization_a
            .begin_publication()
            .expect("A lease capture")
            .expect("A lease publishable");
        let a_generation = coordinator.note_publication_generation(&note_a);
        let b_generation = coordinator.note_publication_generation(&note_b);

        let worker = coordinator.clone();
        let worker_note = note_b.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock B");
            acquired_tx.send(()).expect("Lock B acquired");
            release_rx.recv().expect("release Lock B");
            drop(guard);
        });

        wait_for_note_generation_advance(&coordinator, &note_b, b_generation, "Lock B");
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Lock B must not wait for A publication");

        assert!(
            authorization_a.is_current().expect("A authorization state"),
            "Lock B must not invalidate A's note-local authorization",
        );
        assert_eq!(
            coordinator.note_publication_generation(&note_a),
            a_generation,
            "Lock B must not advance A's note-local generation",
        );
        let concurrent_a = authorization_a
            .begin_publication()
            .expect("concurrent A publication check")
            .expect("A remains publishable while B is locked");
        drop(concurrent_a);
        assert!(
            coordinator
                .capture_note_publication_authorization(&note_a)
                .expect("fresh A capture")
                .is_some(),
            "fresh A note-local authorization must remain available during Lock B",
        );
        assert!(
            coordinator
                .capture_multi_note_publication_authorization()
                .expect("aggregate capture during Lock B")
                .is_none(),
            "aggregate publication must remain fail-closed during Lock B",
        );

        release_tx.send(()).expect("release Lock B");
        handle.join().expect("Lock B thread");
        drop(lease_a);
    }

    #[test]
    fn lock_b_invalidates_and_drains_note_b_publication() {
        let coordinator = PrivacyCoordinator::new();
        let note_b = note("publication-same-b");
        install_key(&coordinator, &note_b);

        let authorization_b = coordinator
            .capture_note_publication_authorization(&note_b)
            .expect("capture B")
            .expect("B publishable");
        let lease_b = authorization_b
            .begin_publication()
            .expect("B lease capture")
            .expect("B lease publishable");
        let before = coordinator.note_publication_generation(&note_b);

        let worker = coordinator.clone();
        let worker_note = note_b.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock B after B publication drain");
            acquired_tx.send(()).expect("Lock B acquired");
            drop(guard);
        });

        wait_for_note_generation_advance(&coordinator, &note_b, before, "Lock B");
        assert!(
            !authorization_b.is_current().expect("B authorization invalidated"),
            "Lock B must invalidate B's pre-admission authorization before draining",
        );
        assert!(
            coordinator
                .capture_note_publication_authorization(&note_b)
                .expect("fresh B capture during Lock")
                .is_none(),
            "Lock B must not mint fresh B authorization while admitted",
        );
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Lock B must wait for an already-started B publication lease",
        );

        drop(lease_b);
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Lock B advances after B publication drains");
        handle.join().expect("Lock B thread");
    }

    #[test]
    fn lock_b_invalidates_and_drains_multi_note_publication() {
        let coordinator = PrivacyCoordinator::new();
        let note_b = note("publication-aggregate-b");
        install_key(&coordinator, &note_b);

        let aggregate = coordinator
            .capture_multi_note_publication_authorization()
            .expect("capture aggregate")
            .expect("aggregate publishable");
        let aggregate_lease = aggregate
            .begin_publication()
            .expect("aggregate lease capture")
            .expect("aggregate lease publishable");
        let before = coordinator.multi_note_publication_generation();

        let worker = coordinator.clone();
        let worker_note = note_b.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock B after aggregate publication drain");
            acquired_tx.send(()).expect("Lock B acquired");
            drop(guard);
        });

        wait_for_multi_note_generation_advance(&coordinator, before, "Lock B");
        assert!(
            !aggregate.is_current().expect("aggregate invalidated"),
            "Lock B must invalidate pre-admission multi-note authorization",
        );
        assert!(
            coordinator
                .capture_multi_note_publication_authorization()
                .expect("fresh aggregate capture")
                .is_none(),
            "Lock B must not mint aggregate authorization while admitted",
        );
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Lock B must drain an already-started multi-note publication",
        );

        drop(aggregate_lease);
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Lock B advances after aggregate publication drains");
        handle.join().expect("Lock B thread");
    }

    #[test]
    fn restore_invalidates_pre_restore_publication_and_allows_fresh_post_restore_authorization() {
        let coordinator = PrivacyCoordinator::new();
        let note_a = note("revision-restore-unrelated-a");
        let note_b = note("revision-restore-target-b");

        let authorization_a = coordinator
            .capture_note_publication_authorization(&note_a)
            .expect("capture A")
            .expect("A publishable");
        let authorization_b = coordinator
            .capture_note_publication_authorization(&note_b)
            .expect("capture B")
            .expect("B publishable");
        let aggregate = coordinator
            .capture_multi_note_publication_authorization()
            .expect("capture aggregate")
            .expect("aggregate publishable");

        let restore = coordinator
            .begin_note_transition(&note_b, NotePrivacyTransition::Restore)
            .expect("note-level Restore transition");

        assert!(
            authorization_a.is_current().expect("A remains current"),
            "note-level Restore B must not invalidate unrelated note A",
        );
        assert!(
            !authorization_b.is_current().expect("B invalidated"),
            "note-level Restore B must invalidate B's pre-Restore publication",
        );
        assert!(
            !aggregate.is_current().expect("aggregate invalidated"),
            "note-level Restore B must invalidate aggregate publication",
        );
        assert!(
            coordinator
                .capture_note_publication_authorization(&note_b)
                .expect("capture B during Restore")
                .is_none(),
            "B cannot mint fresh authorization while Restore is admitted",
        );
        assert!(
            coordinator
                .capture_multi_note_publication_authorization()
                .expect("capture aggregate during Restore")
                .is_none(),
            "aggregate publication is fail-closed while Restore is admitted",
        );
        assert!(
            coordinator
                .capture_note_publication_authorization(&note_a)
                .expect("capture A during Restore")
                .is_some(),
            "unrelated A note-local publication remains available",
        );

        drop(restore);
        let fresh_b = coordinator
            .capture_note_publication_authorization(&note_b)
            .expect("fresh B capture")
            .expect("B publishable after Restore");
        let fresh_aggregate = coordinator
            .capture_multi_note_publication_authorization()
            .expect("fresh aggregate capture")
            .expect("aggregate publishable after Restore");
        assert!(fresh_b.is_current().expect("fresh B current"));
        assert!(fresh_aggregate.is_current().expect("fresh aggregate current"));
        assert!(
            !authorization_b.is_current().expect("old B stays stale"),
            "fresh post-Restore authorization must never revive pre-Restore B work",
        );
        assert!(
            !aggregate.is_current().expect("old aggregate stays stale"),
            "fresh post-Restore authorization must never revive pre-Restore aggregate work",
        );
    }

    #[test]
    fn library_restore_invalidates_and_drains_every_publication_scope() {
        let coordinator = PrivacyCoordinator::new();
        let note_a = note("restore-publication-a");
        let note_b = note("restore-publication-b");

        let authorization_a = coordinator
            .capture_note_publication_authorization(&note_a)
            .expect("capture A")
            .expect("A publishable");
        let authorization_b = coordinator
            .capture_note_publication_authorization(&note_b)
            .expect("capture B")
            .expect("B publishable");
        let aggregate = coordinator
            .capture_multi_note_publication_authorization()
            .expect("capture aggregate")
            .expect("aggregate publishable");
        let lease_a = authorization_a
            .begin_publication()
            .expect("A lease")
            .expect("A lease publishable");
        let lease_b = authorization_b
            .begin_publication()
            .expect("B lease")
            .expect("B lease publishable");
        let aggregate_lease = aggregate
            .begin_publication()
            .expect("aggregate lease")
            .expect("aggregate lease publishable");

        let worker = coordinator.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_library_transition(LibraryPrivacyTransition::Restore)
                .expect("library Restore after publication drain");
            acquired_tx.send(()).expect("Restore acquired");
            drop(guard);
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while (authorization_a.is_current().expect("A state")
            || authorization_b.is_current().expect("B state")
            || aggregate.is_current().expect("aggregate state"))
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(!authorization_a.is_current().expect("A invalidated"));
        assert!(!authorization_b.is_current().expect("B invalidated"));
        assert!(!aggregate.is_current().expect("aggregate invalidated"));
        assert!(
            coordinator
                .capture_note_publication_authorization(&note_a)
                .expect("A capture during Restore")
                .is_none(),
        );
        assert!(
            coordinator
                .capture_multi_note_publication_authorization()
                .expect("aggregate capture during Restore")
                .is_none(),
        );
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Restore must wait for all active publication scopes",
        );

        drop(lease_a);
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Restore must still wait for B and aggregate publication",
        );
        drop(aggregate_lease);
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Restore must still wait for B publication",
        );
        drop(lease_b);
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Restore advances only after every publication scope drains");
        handle.join().expect("Restore thread");
    }

    #[test]
    fn whole_library_operation_serializes_against_note_privacy_transition() {
        let coordinator = PrivacyCoordinator::new();
        let note_b = note("library-operation-lock-b");
        install_key(&coordinator, &note_b);
        let library_operation = coordinator
            .begin_whole_library_operation()
            .expect("library operation capture")
            .expect("library operation available");
        let before = coordinator.note_publication_generation(&note_b);

        let worker = coordinator.clone();
        let worker_note = note_b.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_note_transition(&worker_note, NotePrivacyTransition::Lock)
                .expect("Lock B after library operation drains");
            acquired_tx.send(()).expect("Lock B acquired");
            drop(guard);
        });

        wait_for_note_generation_advance(&coordinator, &note_b, before, "Lock B");
        assert!(
            coordinator
                .begin_whole_library_operation()
                .expect("library operation during Lock")
                .is_none(),
            "an admitted Lock must block new whole-library operations",
        );
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Lock B must wait for an already-started whole-library operation",
        );

        drop(library_operation);
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Lock B advances after whole-library operation releases");
        handle.join().expect("Lock B thread");
    }

    #[test]
    fn whole_library_operation_serializes_against_protected_note_restore() {
        let coordinator = PrivacyCoordinator::new();
        let note_id = note("library-operation-protected-restore");
        install_key(&coordinator, &note_id);
        let library_operation = coordinator
            .begin_whole_library_operation()
            .expect("library operation capture")
            .expect("library operation available");
        let before = coordinator.note_publication_generation(&note_id);

        let worker = coordinator.clone();
        let worker_note = note_id.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_protected_note_restore(&worker_note)
                .expect("protected Restore after library operation drains")
                .expect("protected note remains unlocked");
            acquired_tx.send(()).expect("protected Restore acquired");
            drop(guard);
        });

        wait_for_note_generation_advance(&coordinator, &note_id, before, "protected Restore");
        assert!(
            coordinator
                .begin_whole_library_operation()
                .expect("library operation during protected Restore")
                .is_none(),
            "an admitted protected Restore must block new whole-library operations",
        );
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "protected Restore must wait for an already-started whole-library operation",
        );

        drop(library_operation);
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("protected Restore advances after whole-library operation releases");
        handle.join().expect("protected Restore thread");
    }

    #[test]
    fn whole_library_operation_serializes_against_library_restore() {
        let coordinator = PrivacyCoordinator::new();
        let library_operation = coordinator
            .begin_whole_library_operation()
            .expect("library operation capture")
            .expect("library operation available");
        let aggregate = coordinator
            .capture_multi_note_publication_authorization()
            .expect("aggregate capture")
            .expect("aggregate publishable");

        let worker = coordinator.clone();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let guard = worker
                .begin_library_transition(LibraryPrivacyTransition::Restore)
                .expect("Restore after library operation drains");
            acquired_tx.send(()).expect("Restore acquired");
            drop(guard);
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while aggregate.is_current().expect("aggregate state")
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(
            !aggregate.is_current().expect("aggregate invalidated"),
            "Restore must be admitted before waiting for the earlier library operation",
        );
        assert!(
            coordinator
                .begin_whole_library_operation()
                .expect("library operation during Restore")
                .is_none(),
            "an admitted Restore must block new whole-library operations",
        );
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "Restore must wait for the already-started whole-library operation",
        );

        drop(library_operation);
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Restore advances after whole-library operation releases");
        handle.join().expect("Restore thread");
    }

    // Keep crypto API coverage used by the coordinator module linked under the
    // same security feature set; this catches accidental cfg drift.
    #[test]
    fn coordinator_security_context_types_remain_available() {
        let _ = Argon2idParams::V1_INTERACTIVE;
        let _ = VaultContext::new(VaultDomain::Note, b"coordinator-test");
    }
}

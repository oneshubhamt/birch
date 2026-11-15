use crate::{
    application::errors::DataAccessError,
    autosave::AutosaveController,
    domain::{Note, NoteId},
    storage::VaultStore,
    vault_profile::{VaultProfileSpan, VaultProfileStage},
};

/// Application workflow boundary for selective Rustnote encryption.
///
/// Password bytes cross this boundary only for the duration of the requested
/// operation. Storage owns cryptography and persistence; autosave cleanup keeps
/// the already-known emergency journal from retaining a fresh copy at the E2
/// transition boundary. The broader forensic leak audit remains E3.
#[derive(Clone, Debug)]
pub(crate) struct ProtectNoteOutcome {
    /// `None` means the post-commit status refresh failed. Encryption itself
    /// remains successful; callers should disclose the uncertainty instead of
    /// reporting a false encryption failure after the canonical commit.
    pub(crate) pending_backup_privacy_resets: Option<usize>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct VaultProtectionService {
    storage: VaultStore,
    autosave: AutosaveController,
}

impl VaultProtectionService {
    pub(crate) fn new(storage: VaultStore, autosave: AutosaveController) -> Self {
        Self { storage, autosave }
    }

    pub(crate) fn protect_note(
        &self,
        note_id: &NoteId,
        password: &[u8],
        public_label: &str,
    ) -> Result<ProtectNoteOutcome, DataAccessError> {
        self.block_autosave(note_id)?;
        match self.storage.protect_note(note_id, password, public_label) {
            Ok(_) => {
                // Protect retains the newly created note key, so the canonical
                // post-transition state is protected *and unlocked*. Re-open
                // autosave only after the transition has fully committed; the
                // emergency journal independently rejects protected plaintext.
                self.unblock_autosave_profiled(note_id)?;
                Ok(ProtectNoteOutcome {
                    pending_backup_privacy_resets: self
                        .storage
                        .pending_backup_privacy_reset_count()
                        .ok(),
                })
            }
            Err(error) => {
                // A storage operation can fail after its canonical transaction
                // committed (for example, during privacy cleanup). Release only
                // when authoritative storage says no durable transition remains.
                self.release_autosave_barrier_if_transition_complete(note_id);
                Err(DataAccessError::from(error))
            }
        }
    }

    pub(crate) fn unlock_note(
        &self,
        note_id: &NoteId,
        password: &[u8],
    ) -> Result<Note, DataAccessError> {
        // Freeze any stale pre-unlock autosave work while the privacy transition
        // runs. Once the note key is installed and canonical granular state is
        // authenticated, unlocked protected notes re-enter the ordinary editing
        // pipeline. The emergency journal independently refuses protected
        // plaintext, so enabling autosave does not create a disk plaintext copy.
        self.block_autosave(note_id)?;
        match self.storage.unlock_note(note_id, password) {
            Ok(note) => {
                self.unblock_autosave_profiled(note_id)?;
                Ok(note)
            }
            Err(error) => {
                self.release_autosave_barrier_if_transition_complete(note_id);
                Err(DataAccessError::from(error))
            }
        }
    }

    pub(crate) fn lock_note(&self, note_id: &NoteId) -> Result<Note, DataAccessError> {
        self.block_autosave(note_id)?;
        let result = self
            .storage
            .lock_note(note_id)
            .map_err(DataAccessError::from);
        // The worker barrier is transition-scoped, not a permanent lock-state flag.
        // Once Lock finishes, authoritative storage/session state decides whether
        // autosave mutation is admissible; a locked note remains blocked because
        // its Vault key is absent, while navigation/checkpoint code can move on.
        match result {
            Ok(note) => {
                self.unblock_autosave_profiled(note_id)?;
                Ok(note)
            }
            Err(error) => {
                self.release_autosave_barrier_if_transition_complete(note_id);
                Err(error)
            }
        }
    }

    pub(crate) fn remove_note_protection(
        &self,
        note_id: &NoteId,
        password: &[u8],
    ) -> Result<Note, DataAccessError> {
        self.block_autosave(note_id)?;
        match self.storage.remove_note_protection(note_id, password) {
            Ok(note) => {
                self.unblock_autosave_profiled(note_id)?;
                Ok(note)
            }
            Err(error) => {
                // Removal can likewise report a post-commit filesystem cleanup
                // error. Keep the worker fence aligned with durable transition
                // state rather than the surface API result.
                self.release_autosave_barrier_if_transition_complete(note_id);
                Err(DataAccessError::from(error))
            }
        }
    }

    fn block_autosave(&self, note_id: &NoteId) -> Result<(), DataAccessError> {
        let profile = VaultProfileSpan::stage(VaultProfileStage::AutosaveBarrier);
        let result = self
            .autosave
            .block_note_for_vault(note_id)
            .map_err(autosave_barrier_error);
        drop(profile);
        result
    }

    fn unblock_autosave_profiled(&self, note_id: &NoteId) -> Result<(), DataAccessError> {
        let profile = VaultProfileSpan::stage(VaultProfileStage::AutosaveRelease);
        let result = self
            .autosave
            .unblock_note_after_vault(note_id)
            .map_err(autosave_barrier_error);
        drop(profile);
        result
    }

    fn release_autosave_barrier_if_transition_complete(&self, note_id: &NoteId) {
        if matches!(
            self.storage.requires_autosave_transition_barrier(note_id),
            Ok(false)
        ) {
            let _ = self.autosave.unblock_note_after_vault(note_id);
        }
    }
}

fn autosave_barrier_error(error: crate::autosave::AutosaveError) -> DataAccessError {
    DataAccessError::Storage(crate::storage::StorageError::Validation(format!(
        "could not establish the encrypted-note autosave privacy barrier: {error}"
    )))
}

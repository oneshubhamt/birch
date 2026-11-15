use std::{
    collections::{HashMap, HashSet},
    fmt, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::domain::NoteId;
use crate::security::VaultKey;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Process-local state for scopes the user has explicitly unlocked.
///
/// A protected operation must hold a `VaultReadPermit` for the complete lifetime
/// of any decrypted resource it can use. Locking first revokes new permits, then
/// waits for existing permits to drain before dropping the key and removing
/// materialized plaintext. This gives `Lock` one authoritative revocation boundary
/// instead of relying on endpoint-specific rechecks.
pub(crate) struct VaultRuntime {
    inner: Arc<VaultRuntimeInner>,
}

struct VaultRuntimeInner {
    state: Mutex<VaultState>,
    idle: Condvar,
    temp_owner: Mutex<Option<crate::native::temp_owner::SensitiveTempOwner>>,
}

#[derive(Default)]
struct VaultState {
    notes: HashMap<NoteId, VaultNoteSession>,
    materialized_attachments: HashMap<(NoteId, String), PathBuf>,
    transient_plaintext: HashMap<NoteId, HashSet<PathBuf>>,
    note_revocations: HashSet<NoteId>,
    library_revoking: bool,
    next_generation: u64,
}

struct VaultNoteSession {
    key: Arc<VaultKey>,
    generation: u64,
    revoking: bool,
    active_permits: usize,
}

#[derive(Clone)]
pub(crate) struct VaultNoteAccess {
    key: Arc<VaultKey>,
    permit: VaultReadPermit,
}

/// Exclusive revocation intent retained across the storage mutation that
/// follows a Vault drain.
///
/// The runtime publishes revocation and waits for existing readers *before*
/// callers take Storage's operation lock. Keeping this guard alive then blocks
/// a concurrent Unlock from repopulating the session in the small interval
/// between the drain and the serialized database mutation. This establishes a
/// single lock order: Vault readers drain first, storage mutation second.
pub(crate) struct VaultRevocationTransition {
    runtime: Arc<VaultRuntimeInner>,
    scope: VaultRevocationScope,
    active: bool,
}

enum VaultRevocationScope {
    Note(NoteId),
    Notes(Vec<NoteId>),
    Library,
}

#[derive(Clone)]
pub(crate) struct VaultReadPermit {
    lease: Arc<VaultReadLease>,
}

/// A Rustnote-owned ephemeral plaintext file derived from protected state.
///
/// The guard shares the underlying Vault read lease, so Lock cannot complete
/// while the derivative is being generated or served. Successful cleanup
/// unregisters the path. If deletion fails, the path deliberately remains in
/// VaultRuntime so Lock performs a second fail-closed cleanup before returning.
pub(crate) struct VaultPlaintextPath {
    permit: VaultReadPermit,
    path: Option<PathBuf>,
}

impl fmt::Debug for VaultPlaintextPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultPlaintextPath([REDACTED])")
    }
}

struct VaultReadLease {
    runtime: Arc<VaultRuntimeInner>,
    note_id: NoteId,
    generation: u64,
}

impl fmt::Debug for VaultReadPermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultReadPermit([REDACTED])")
    }
}

impl fmt::Debug for VaultNoteAccess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultNoteAccess([REDACTED])")
    }
}

impl fmt::Debug for VaultRevocationTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultRevocationTransition([REDACTED])")
    }
}

impl VaultNoteAccess {
    pub(crate) fn key(&self) -> &VaultKey {
        self.key.as_ref()
    }

    pub(crate) fn permit(&self) -> &VaultReadPermit {
        &self.permit
    }

    pub(crate) fn into_permit(self) -> VaultReadPermit {
        self.permit
    }
}

impl VaultReadPermit {
    pub(crate) fn note_id(&self) -> &NoteId {
        &self.lease.note_id
    }

    /// Validate this exact unlocked-note session immediately before plaintext
    /// publication or native handoff.
    pub(crate) fn is_publishable(&self) -> Result<bool, &'static str> {
        let state = self
            .lease
            .runtime
            .state
            .lock()
            .map_err(|_| "vault runtime state lock is poisoned")?;
        let Some(session) = state.notes.get(&self.lease.note_id) else {
            return Ok(false);
        };
        Ok(!session.revoking && self.belongs_to(&self.lease.note_id, session.generation))
    }

    fn belongs_to(&self, note_id: &NoteId, generation: u64) -> bool {
        self.lease.note_id == *note_id && self.lease.generation == generation
    }

    pub(crate) fn track_plaintext_path(
        &self,
        path: PathBuf,
    ) -> Result<VaultPlaintextPath, &'static str> {
        let mut state = self
            .lease
            .runtime
            .state
            .lock()
            .map_err(|_| "vault runtime state lock is poisoned")?;
        let Some(session) = state.notes.get(&self.lease.note_id) else {
            return Err("vault note session disappeared while tracking plaintext");
        };
        if session.revoking || !self.belongs_to(&self.lease.note_id, session.generation) {
            return Err("vault note is being locked");
        }
        state
            .transient_plaintext
            .entry(self.lease.note_id.clone())
            .or_default()
            .insert(path.clone());
        Ok(VaultPlaintextPath {
            permit: self.clone(),
            path: Some(path),
        })
    }
}

impl VaultPlaintextPath {
    /// Promote an already-tracked transient plaintext path to the canonical
    /// materialized-attachment registry without ever creating an ownership gap.
    pub(crate) fn promote_materialized_attachment(
        &mut self,
        note_id: &NoteId,
        attachment_id: String,
    ) -> Result<PathBuf, &'static str> {
        let Some(path) = self.path.as_ref().cloned() else {
            return Err("vault plaintext path is no longer tracked");
        };
        if self.permit.lease.note_id != *note_id {
            return Err("vault plaintext path belongs to a different note");
        }

        let mut state = self
            .permit
            .lease
            .runtime
            .state
            .lock()
            .map_err(|_| "vault runtime state lock is poisoned")?;
        let Some(session) = state.notes.get(note_id) else {
            return Err("vault note session disappeared while materializing attachment");
        };
        if session.revoking {
            return Err("vault note is being locked");
        }
        if !self.permit.belongs_to(note_id, session.generation) {
            return Err("vault read permit no longer belongs to this note session");
        }

        let materialized_key = (note_id.clone(), attachment_id);
        if let Some(existing) = state
            .materialized_attachments
            .get(&materialized_key)
            .cloned()
        {
            match super::plaintext::managed_path_is_present(&existing) {
                Ok(true) => {
                    // Another authorized reader won the materialization race. Keep its
                    // canonical path and securely discard our still-tracked duplicate.
                    drop(state);
                    self.remove_now()
                        .map_err(|_| "could not remove redundant vault plaintext")?;
                    return Ok(existing);
                }
                Ok(false) => {}
                Err(_) => {
                    return Err("could not prove prior vault materialization is absent");
                }
            }
        }

        let remove_transient_entry = {
            let Some(paths) = state.transient_plaintext.get_mut(note_id) else {
                return Err("vault plaintext path lost runtime ownership");
            };
            if !paths.remove(&path) {
                return Err("vault plaintext path lost runtime ownership");
            }
            paths.is_empty()
        };
        if remove_transient_entry {
            state.transient_plaintext.remove(note_id);
        }
        state
            .materialized_attachments
            .insert(materialized_key, path.clone());
        self.path = None;
        Ok(path)
    }

    pub(crate) fn remove_now(&mut self) -> io::Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        remove_owned_plaintext_path(path)?;
        self.unregister();
        self.path = None;
        Ok(())
    }

    fn unregister(&self) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        let Ok(mut state) = self.permit.lease.runtime.state.lock() else {
            return;
        };
        let remove_entry = if let Some(paths) = state
            .transient_plaintext
            .get_mut(&self.permit.lease.note_id)
        {
            paths.remove(path);
            paths.is_empty()
        } else {
            false
        };
        if remove_entry {
            state.transient_plaintext.remove(&self.permit.lease.note_id);
        }
    }
}

impl Drop for VaultPlaintextPath {
    fn drop(&mut self) {
        if self.path.is_some() {
            let _ = self.remove_now();
        }
    }
}

impl Drop for VaultReadLease {
    fn drop(&mut self) {
        let Ok(mut state) = self.runtime.state.lock() else {
            return;
        };
        if let Some(session) = state.notes.get_mut(&self.note_id)
            && session.generation == self.generation
            && session.active_permits > 0
        {
            session.active_permits -= 1;
            if session.active_permits == 0 {
                self.runtime.idle.notify_all();
            }
        }
    }
}

impl Drop for VaultRevocationTransition {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Ok(mut state) = self.runtime.state.lock() else {
            return;
        };
        match &self.scope {
            VaultRevocationScope::Note(note_id) => {
                state.note_revocations.remove(note_id);
            }
            VaultRevocationScope::Notes(note_ids) => {
                for note_id in note_ids {
                    state.note_revocations.remove(note_id);
                }
            }
            VaultRevocationScope::Library => {
                state.library_revoking = false;
            }
        }
        self.active = false;
        self.runtime.idle.notify_all();
    }
}

impl Default for VaultRuntime {
    fn default() -> Self {
        Self {
            inner: Arc::new(VaultRuntimeInner {
                state: Mutex::new(VaultState::default()),
                idle: Condvar::new(),
                temp_owner: Mutex::new(None),
            }),
        }
    }
}

impl fmt::Debug for VaultRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultRuntime([REDACTED])")
    }
}

impl VaultRuntime {
    /// Remove abandoned decrypted attachment material before a library is opened.
    ///
    /// This is intentionally fallible and called from the storage constructor:
    /// silently ignoring a cleanup failure would allow Rustnote to continue while
    /// plaintext from a crashed process remains inside an app-managed namespace.
    pub(crate) fn cleanup_stale_plaintext() -> crate::native::NativeResult<()> {
        let vault_root =
            crate::native::private_temp::rustnote_temp_directory("vault").map_err(|error| {
                crate::native::NativeError::from(format!(
                    "Could not prepare Vault temporary cleanup: {error}"
                ))
            })?;
        crate::native::temp_owner::cleanup_session_directories(
            &vault_root,
            "vault temporary material",
        )?;
        Ok(())
    }

    fn state(&self) -> Result<MutexGuard<'_, VaultState>, &'static str> {
        self.inner
            .state
            .lock()
            .map_err(|_| "vault runtime state lock is poisoned")
    }

    pub(crate) fn insert_note_key(
        &self,
        note_id: NoteId,
        key: VaultKey,
    ) -> Result<(), &'static str> {
        let mut state = self.state()?;
        if state.library_revoking || state.note_revocations.contains(&note_id) {
            return Err("vault note is being locked");
        }
        if let Some(session) = state.notes.get(&note_id) {
            if session.revoking {
                return Err("vault note is being locked");
            }
            // A correct unlock of an already-unlocked note derives the same note
            // key. Keep the existing session so active permits and generation stay
            // coherent; the newly derived key is zeroized when it drops here.
            return Ok(());
        }
        state.next_generation = state.next_generation.wrapping_add(1).max(1);
        let generation = state.next_generation;
        state.notes.insert(
            note_id,
            VaultNoteSession {
                key: Arc::new(key),
                generation,
                revoking: false,
                active_permits: 0,
            },
        );
        Ok(())
    }

    /// Revoke a note session synchronously.
    ///
    /// Once `revoking` is published, new read permits fail. Existing readers are
    /// allowed to finish. Registered plaintext remains owned by the runtime until
    /// every cleanup succeeds; only then are the cleanup records and note key
    /// removed. A failed Lock therefore stays revoking and is safely retryable.
    #[cfg(test)]
    pub(crate) fn remove_note_key(&self, note_id: &NoteId) -> Result<bool, &'static str> {
        let (removed, transition) = self.begin_note_revocation(note_id)?;
        drop(transition);
        Ok(removed)
    }

    /// Publish note revocation, drain every active read and retire tracked
    /// plaintext without holding Storage's operation lock. The returned guard
    /// must remain alive until the caller's serialized mutation is complete so
    /// Unlock cannot repopulate the session between those phases.
    pub(crate) fn begin_note_revocation(
        &self,
        note_id: &NoteId,
    ) -> Result<(bool, VaultRevocationTransition), &'static str> {
        let transition = self.reserve_note_revocation(note_id)?;
        let removed = self.remove_note_key_with(note_id, remove_owned_plaintext_path)?;
        Ok((removed, transition))
    }

    /// Publish revocation for the complete note set in one critical section
    /// before waiting for any existing reader. This prevents a later target from
    /// admitting a fresh protected read while an earlier target is still draining.
    pub(crate) fn begin_note_revocations(
        &self,
        note_ids: &[NoteId],
    ) -> Result<(usize, VaultRevocationTransition), &'static str> {
        let transition = self.reserve_note_revocations(note_ids)?;
        let note_ids = match &transition.scope {
            VaultRevocationScope::Note(note_id) => vec![note_id.clone()],
            VaultRevocationScope::Notes(note_ids) => note_ids.clone(),
            VaultRevocationScope::Library => {
                return Err("multi-note revocation unexpectedly reserved library scope");
            }
        };

        let mut removed = 0_usize;
        for note_id in note_ids {
            if self.remove_note_key_with(&note_id, remove_owned_plaintext_path)? {
                removed = removed.saturating_add(1);
            }
        }
        Ok((removed, transition))
    }

    /// Revoke every unlocked note before a whole-library replacement.
    ///
    /// The global guard blocks all Unlock attempts until the replacement has
    /// finished, while the drain itself happens before Storage's operation lock
    /// is acquired. This avoids waiting for a permit-bearing UI operation while
    /// holding the lock that operation may still need to finish.
    pub(crate) fn begin_library_revocation(
        &self,
    ) -> Result<(usize, VaultRevocationTransition), &'static str> {
        let transition = self.reserve_library_revocation()?;
        let note_ids = {
            let mut state = self.state()?;
            for session in state.notes.values_mut() {
                session.revoking = true;
            }
            while state
                .notes
                .values()
                .any(|session| session.active_permits > 0)
            {
                state = self
                    .inner
                    .idle
                    .wait(state)
                    .map_err(|_| "vault runtime state lock is poisoned")?;
            }
            state.notes.keys().cloned().collect::<Vec<_>>()
        };

        let mut removed = 0_usize;
        for note_id in note_ids {
            if self.remove_note_key_with(&note_id, remove_owned_plaintext_path)? {
                removed = removed.saturating_add(1);
            }
        }
        Ok((removed, transition))
    }

    fn reserve_note_revocation(
        &self,
        note_id: &NoteId,
    ) -> Result<VaultRevocationTransition, &'static str> {
        self.reserve_note_revocations(std::slice::from_ref(note_id))
    }

    fn reserve_note_revocations(
        &self,
        note_ids: &[NoteId],
    ) -> Result<VaultRevocationTransition, &'static str> {
        let mut ids = note_ids.to_vec();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.dedup_by(|left, right| left.as_str() == right.as_str());
        if ids.is_empty() {
            return Err("vault revocation requires at least one note");
        }

        let mut state = self.state()?;
        // Conflicting privacy mutations are rejected rather than queued. Admit
        // the complete set atomically so an overlap never leaves partial Vault
        // revocation state. Different-note transitions remain independent.
        if state.library_revoking
            || ids
                .iter()
                .any(|note_id| state.note_revocations.contains(note_id))
        {
            return Err("vault privacy transition is already in progress");
        }
        state.note_revocations.extend(ids.iter().cloned());
        for note_id in &ids {
            if let Some(session) = state.notes.get_mut(note_id) {
                session.revoking = true;
            }
        }

        let scope = if ids.len() == 1 {
            VaultRevocationScope::Note(ids[0].clone())
        } else {
            VaultRevocationScope::Notes(ids)
        };
        Ok(VaultRevocationTransition {
            runtime: Arc::clone(&self.inner),
            scope,
            active: true,
        })
    }

    fn reserve_library_revocation(&self) -> Result<VaultRevocationTransition, &'static str> {
        let mut state = self.state()?;
        // Whole-library replacement is mutually exclusive with every note-level
        // revocation. Reject an overlap instead of queueing it, so a privacy
        // mutation that was already attempted cannot wake after another Lock or
        // replacement reports success and publish state from the old boundary.
        if state.library_revoking || !state.note_revocations.is_empty() {
            return Err("vault privacy transition is already in progress");
        }
        state.library_revoking = true;
        Ok(VaultRevocationTransition {
            runtime: Arc::clone(&self.inner),
            scope: VaultRevocationScope::Library,
            active: true,
        })
    }

    fn remove_note_key_with<F>(
        &self,
        note_id: &NoteId,
        mut remove_path: F,
    ) -> Result<bool, &'static str>
    where
        F: FnMut(&Path) -> io::Result<()>,
    {
        let had_session = {
            let mut state = self.state()?;
            let had_session = state.notes.contains_key(note_id);
            if let Some(session) = state.notes.get_mut(note_id) {
                session.revoking = true;
            }

            loop {
                let active = state
                    .notes
                    .get(note_id)
                    .map(|session| session.active_permits)
                    .unwrap_or(0);
                if active == 0 {
                    break;
                }
                state = self
                    .inner
                    .idle
                    .wait(state)
                    .map_err(|_| "vault runtime state lock is poisoned")?;
            }
            had_session
        };

        // Snapshot without unregistering. If any deletion fails, every cleanup
        // obligation and the revoking session/key remain intact for a later retry.
        let paths = {
            let state = self.state()?;
            plaintext_paths(&state, note_id)
        };
        for path in &paths {
            remove_path(path).map_err(|_| "could not remove decrypted vault plaintext")?;
        }

        // Revocation prevents new permits/registrations, so after successful
        // deletion it is safe to forget the now-satisfied cleanup obligations.
        let mut state = self.state()?;
        let removed = state.notes.remove(note_id).is_some();
        forget_plaintext_paths(&mut state, note_id);
        Ok(had_session || removed)
    }

    /// Returns whether this note currently has an active, non-revoking Vault
    /// session. This is the production semantic used by encrypted autosave
    /// admission; callers do not receive or inspect the key itself.
    pub(crate) fn note_session_is_active(&self, note_id: &NoteId) -> Result<bool, &'static str> {
        Ok(self
            .state()?
            .notes
            .get(note_id)
            .is_some_and(|session| !session.revoking))
    }

    #[cfg(test)]
    pub(crate) fn contains_note_key(&self, note_id: &NoteId) -> Result<bool, &'static str> {
        self.note_session_is_active(note_id)
    }

    #[cfg(test)]
    pub(crate) fn active_permit_count(&self, note_id: &NoteId) -> Result<usize, &'static str> {
        Ok(self
            .state()?
            .notes
            .get(note_id)
            .map(|session| session.active_permits)
            .unwrap_or(0))
    }

    pub(crate) fn acquire_note_access(
        &self,
        note_id: &NoteId,
    ) -> Result<Option<VaultNoteAccess>, &'static str> {
        let mut state = self.state()?;
        let Some(session) = state.notes.get_mut(note_id) else {
            return Ok(None);
        };
        if session.revoking {
            return Ok(None);
        }
        session.active_permits = session
            .active_permits
            .checked_add(1)
            .ok_or("vault read permit count overflowed")?;
        let key = Arc::clone(&session.key);
        let generation = session.generation;
        Ok(Some(VaultNoteAccess {
            key,
            permit: VaultReadPermit {
                lease: Arc::new(VaultReadLease {
                    runtime: Arc::clone(&self.inner),
                    note_id: note_id.clone(),
                    generation,
                }),
            },
        }))
    }

    /// Final publication gate for an owned value derived from protected state.
    ///
    /// Callers must keep the permit alive while decrypting *and* while deriving
    /// the value they intend to publish. If Lock has begun in the meantime the
    /// derived value must be discarded before the permit is released, so Lock
    /// cannot complete and then be followed by a stale protected-read result.
    pub(crate) fn read_permit_is_publishable(
        &self,
        permit: &VaultReadPermit,
    ) -> Result<bool, &'static str> {
        permit.is_publishable()
    }

    pub(crate) fn materialized_attachment(
        &self,
        permit: &VaultReadPermit,
        note_id: &NoteId,
        attachment_id: &str,
    ) -> Result<Option<PathBuf>, &'static str> {
        let state = self.state()?;
        let Some(session) = state.notes.get(note_id) else {
            return Ok(None);
        };
        if !permit.belongs_to(note_id, session.generation) {
            return Ok(None);
        }
        let Some(path) = state
            .materialized_attachments
            .get(&(note_id.clone(), attachment_id.to_string()))
            .cloned()
        else {
            return Ok(None);
        };
        match super::plaintext::managed_path_is_present(&path) {
            Ok(true) => Ok(Some(path)),
            Ok(false) => Ok(None),
            Err(_) => Err("could not prove vault materialization is absent"),
        }
    }

    pub(crate) fn allocate_attachment_path(&self) -> io::Result<PathBuf> {
        let mut owner = self
            .inner
            .temp_owner
            .lock()
            .map_err(|_| io::Error::other("vault temporary owner lock is poisoned"))?;
        if owner.is_none() {
            let vault_root = crate::native::private_temp::rustnote_temp_directory("vault")?;
            *owner = Some(
                crate::native::temp_owner::SensitiveTempOwner::acquire(&vault_root, "session-")
                    .map_err(|error| io::Error::other(error.to_string()))?,
            );
        }
        let temp_root = owner
            .as_ref()
            .ok_or_else(|| io::Error::other("vault temporary owner was not initialized"))?
            .root();
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        Ok(temp_root.join(format!("attachment-{sequence}.tmp")))
    }
}

fn plaintext_paths(state: &VaultState, note_id: &NoteId) -> Vec<PathBuf> {
    let mut paths = state
        .materialized_attachments
        .iter()
        .filter(|((candidate, _), _)| candidate == note_id)
        .map(|(_, path)| path.clone())
        .collect::<Vec<_>>();
    if let Some(transient) = state.transient_plaintext.get(note_id) {
        paths.extend(transient.iter().cloned());
    }
    paths.sort();
    paths.dedup();
    paths
}

fn forget_plaintext_paths(state: &mut VaultState, note_id: &NoteId) {
    state
        .materialized_attachments
        .retain(|(candidate, _), _| candidate != note_id);
    state.transient_plaintext.remove(note_id);
}

fn remove_owned_plaintext_path(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            crate::native::privacy::remove_tree(path)
        }
        Ok(_) => crate::native::privacy::remove_file(path).map(|_| ()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn is_vault_runtime_path(path: &Path) -> bool {
    let mut saw_vault_namespace = false;
    for ancestor in path.ancestors() {
        let Some(name) = ancestor.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name == "vault" {
            saw_vault_namespace = true;
            continue;
        }
        if saw_vault_namespace && name.starts_with("session-v2-") {
            return true;
        }
    }
    // In the v2 layout the session is a descendant of the `vault` namespace,
    // so also recognize it directly when walking from the materialized file.
    path.ancestors().any(|ancestor| {
        ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with("session-v2-"))
            && ancestor.parent().is_some_and(|parent| {
                parent.file_name().and_then(|value| value.to_str()) == Some("vault")
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    fn key() -> VaultKey {
        crate::security::VaultService::new()
            .generate_key()
            .expect("generate vault key")
    }

    #[test]
    fn tracked_materialization_cannot_promote_after_revocation_begins() {
        let runtime = Arc::new(VaultRuntime::default());
        let note_id = NoteId::parse("note-vault-late-materialization").expect("note id");
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("insert key");
        let access = runtime
            .acquire_note_access(&note_id)
            .expect("access")
            .expect("unlocked access");
        let path = runtime.allocate_attachment_path().expect("temp path");
        let mut tracked = access
            .permit()
            .track_plaintext_path(path.clone())
            .expect("track before plaintext creation");
        crate::native::private_temp::create_private_file(&path)
            .expect("create temp")
            .sync_all()
            .expect("sync temp");

        let (started_tx, started_rx) = mpsc::channel();
        let runtime_for_lock = Arc::clone(&runtime);
        let note_for_lock = note_id.clone();
        let lock_thread = thread::spawn(move || {
            started_tx.send(()).expect("signal lock start");
            runtime_for_lock.remove_note_key(&note_for_lock)
        });
        started_rx.recv().expect("lock started");
        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !runtime.contains_note_key(&note_id).expect("contains key") {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should publish revocation promptly"
        );

        assert!(
            tracked
                .promote_materialized_attachment(&note_id, "attachment-late".to_string())
                .is_err(),
            "a decrypt finishing after revocation starts must not publish new plaintext"
        );
        tracked
            .remove_now()
            .expect("rejected plaintext remains owned and removable");
        drop(tracked);
        drop(access);
        assert!(lock_thread.join().expect("join lock").expect("lock result"));
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn ambiguous_materialized_attachment_presence_fails_closed_without_replacement() {
        use std::os::unix::fs::symlink;

        let runtime = VaultRuntime::default();
        let note_id = NoteId::parse("note-vault-ambiguous-materialization").expect("note id");
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("insert key");
        let access = runtime
            .acquire_note_access(&note_id)
            .expect("access")
            .expect("unlocked access");

        let root = std::env::temp_dir().join(format!(
            "rustnote-vault-ambiguous-materialization-{}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&root).expect("test root");
        let loop_parent = root.join("loop");
        symlink("loop", &loop_parent).expect("symlink loop");
        let registered = loop_parent.join("materialized.bin");
        {
            let mut state = runtime.state().expect("runtime state");
            state.materialized_attachments.insert(
                (note_id.clone(), "attachment-ambiguous".to_string()),
                registered.clone(),
            );
        }

        let candidate = root.join("candidate.bin");
        std::fs::write(&candidate, b"candidate plaintext").expect("candidate plaintext");
        let mut tracked = access
            .permit()
            .track_plaintext_path(candidate.clone())
            .expect("track candidate");

        assert!(
            runtime
                .materialized_attachment(access.permit(), &note_id, "attachment-ambiguous",)
                .is_err(),
            "ambiguous metadata must not be reported as absence"
        );
        assert!(
            tracked
                .promote_materialized_attachment(&note_id, "attachment-ambiguous".to_string(),)
                .is_err(),
            "ambiguous prior materialization must block replacement"
        );
        assert!(
            candidate.is_file(),
            "rejected candidate must remain tracked for cleanup"
        );
        {
            let state = runtime.state().expect("runtime state after rejection");
            assert_eq!(
                state
                    .materialized_attachments
                    .get(&(note_id.clone(), "attachment-ambiguous".to_string())),
                Some(&registered),
                "registry authority must remain on the ambiguous predecessor"
            );
        }

        std::fs::remove_file(&loop_parent).expect("remove symlink loop");
        std::fs::create_dir(&loop_parent).expect("repair parent");
        std::fs::write(&registered, b"canonical plaintext").expect("repair canonical plaintext");

        assert_eq!(
            runtime
                .materialized_attachment(access.permit(), &note_id, "attachment-ambiguous",)
                .expect("lookup after repair"),
            Some(registered.clone())
        );
        let reused = tracked
            .promote_materialized_attachment(&note_id, "attachment-ambiguous".to_string())
            .expect("retry promotion after repair");
        assert_eq!(reused, registered);
        assert!(
            !candidate.exists(),
            "retry must discard the redundant candidate"
        );

        drop(tracked);
        drop(access);
        runtime.remove_note_key(&note_id).expect("Lock");
        assert!(
            !registered.exists(),
            "Lock must remove repaired canonical plaintext"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_materialization_reuses_canonical_path_without_orphaning_duplicate() {
        let runtime = VaultRuntime::default();
        let note_id = NoteId::parse("note-vault-concurrent-materialization").expect("note id");
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("insert key");
        let access = runtime
            .acquire_note_access(&note_id)
            .expect("access")
            .expect("unlocked access");
        let first = std::env::temp_dir().join(format!(
            "rustnote-vault-concurrent-first-{}",
            std::process::id()
        ));
        let second = std::env::temp_dir().join(format!(
            "rustnote-vault-concurrent-second-{}",
            std::process::id()
        ));
        std::fs::write(&first, b"first plaintext").expect("write first");
        std::fs::write(&second, b"second plaintext").expect("write second");
        let mut first_guard = access
            .permit()
            .track_plaintext_path(first.clone())
            .expect("track first");
        let mut second_guard = access
            .permit()
            .track_plaintext_path(second.clone())
            .expect("track second");

        let canonical = first_guard
            .promote_materialized_attachment(&note_id, "attachment-race".to_string())
            .expect("publish first");
        let reused = second_guard
            .promote_materialized_attachment(&note_id, "attachment-race".to_string())
            .expect("reuse first");
        assert_eq!(canonical, first);
        assert_eq!(reused, first);
        assert!(
            first.exists(),
            "canonical materialization must remain published"
        );
        assert!(
            !second.exists(),
            "redundant materialization must be removed"
        );

        drop(first_guard);
        drop(second_guard);
        drop(access);
        runtime.remove_note_key(&note_id).expect("Lock");
        assert!(
            !first.exists(),
            "Lock must remove the canonical materialization"
        );
    }

    #[test]
    fn failed_lock_cleanup_retains_session_and_plaintext_for_retry() {
        let runtime = VaultRuntime::default();
        let note_id = NoteId::parse("note-vault-lock-cleanup-retry").expect("note id");
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("insert key");
        let access = runtime
            .acquire_note_access(&note_id)
            .expect("access")
            .expect("unlocked access");
        let path = std::env::temp_dir().join(format!(
            "rustnote-vault-lock-cleanup-retry-{}",
            std::process::id()
        ));
        std::fs::write(&path, b"vault plaintext").expect("write plaintext");
        let mut tracked = access
            .permit()
            .track_plaintext_path(path.clone())
            .expect("track plaintext");
        tracked
            .promote_materialized_attachment(&note_id, "attachment-retry".to_string())
            .expect("promote materialized plaintext");
        drop(tracked);
        drop(access);

        let failure = runtime.remove_note_key_with(&note_id, |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected cleanup failure",
            ))
        });
        assert!(
            failure.is_err(),
            "Lock must fail when plaintext cleanup fails"
        );
        assert!(
            path.exists(),
            "failed cleanup must leave the plaintext for retry"
        );
        {
            let state = runtime.state().expect("runtime state");
            let session = state
                .notes
                .get(&note_id)
                .expect("revoking session retained");
            assert!(
                session.revoking,
                "failed Lock must remain in revocation state"
            );
            assert!(
                plaintext_paths(&state, &note_id).contains(&path),
                "failed Lock must retain its plaintext cleanup obligation"
            );
        }

        assert!(
            runtime.remove_note_key(&note_id).expect("retry Lock"),
            "retry must complete the retained session revocation"
        );
        assert!(!path.exists(), "retry Lock must remove retained plaintext");
        let state = runtime.state().expect("runtime state after retry");
        assert!(!state.notes.contains_key(&note_id));
        assert!(plaintext_paths(&state, &note_id).is_empty());
    }

    #[test]
    fn note_revocation_transition_blocks_reunlock_until_storage_boundary_releases() {
        let runtime = VaultRuntime::default();
        let note_id = NoteId::parse("note-vault-revocation-transition").expect("note id");
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("insert key");

        let (removed, transition) = runtime
            .begin_note_revocation(&note_id)
            .expect("begin note revocation");
        assert!(removed, "revocation should remove the existing session");
        assert!(
            runtime.insert_note_key(note_id.clone(), key()).is_err(),
            "Unlock must remain blocked while the serialized storage transition is active"
        );

        drop(transition);
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("Unlock may resume after storage transition releases");
        assert!(runtime.remove_note_key(&note_id).expect("cleanup lock"));
    }

    #[test]
    fn conflicting_note_revocation_is_rejected_while_first_storage_transition_is_active() {
        let runtime = VaultRuntime::default();
        let note_id = NoteId::parse("note-vault-exclusive-revocation").expect("note id");
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("insert key");

        let (_removed, first) = runtime
            .begin_note_revocation(&note_id)
            .expect("first revocation");
        assert!(
            runtime.begin_note_revocation(&note_id).is_err(),
            "same-note privacy mutation must fail closed rather than queue behind an active boundary"
        );
        drop(first);
    }

    #[test]
    fn multi_note_revocation_blocks_all_target_readers_before_first_drain() {
        let runtime = Arc::new(VaultRuntime::default());
        let first_note = NoteId::parse("note-vault-multi-revocation-a").expect("first note id");
        let second_note = NoteId::parse("note-vault-multi-revocation-b").expect("second note id");
        runtime
            .insert_note_key(first_note.clone(), key())
            .expect("insert first key");
        runtime
            .insert_note_key(second_note.clone(), key())
            .expect("insert second key");
        let first_permit = runtime
            .acquire_note_access(&first_note)
            .expect("first access")
            .expect("first unlocked access")
            .into_permit();

        let runtime_for_transition = Arc::clone(&runtime);
        let first_for_transition = first_note.clone();
        let second_for_transition = second_note.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let transition_thread = thread::spawn(move || {
            started_tx.send(()).expect("transition started");
            let result = runtime_for_transition
                .begin_note_revocations(&[first_for_transition, second_for_transition]);
            done_tx.send(result).expect("transition result");
        });
        started_rx.recv().expect("transition start signal");

        let mut second_revocation_visible = false;
        for _ in 0..2_000 {
            if !runtime
                .contains_note_key(&second_note)
                .expect("second key visibility")
            {
                second_revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            second_revocation_visible,
            "all multi-note targets must publish revocation before any reader drain completes"
        );
        assert!(
            runtime
                .acquire_note_access(&second_note)
                .expect("second access check")
                .is_none(),
            "later target must reject new readers while the first target still drains"
        );
        assert!(
            done_rx.try_recv().is_err(),
            "multi-note revocation must still be waiting for the active first reader"
        );

        drop(first_permit);
        let (removed, guard) = done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("revocation completion")
            .expect("multi-note revocation");
        assert_eq!(removed, 2);
        drop(guard);
        transition_thread.join().expect("transition thread");
    }

    #[test]
    fn library_and_note_revocations_reject_overlapping_privacy_mutations() {
        let runtime = VaultRuntime::default();
        let first_note = NoteId::parse("note-vault-library-exclusive-a").expect("first note id");
        let second_note = NoteId::parse("note-vault-library-exclusive-b").expect("second note id");
        runtime
            .insert_note_key(first_note.clone(), key())
            .expect("insert first key");
        runtime
            .insert_note_key(second_note.clone(), key())
            .expect("insert second key");

        let (_removed, note_transition) = runtime
            .begin_note_revocation(&first_note)
            .expect("note transition");
        assert!(
            runtime.begin_library_revocation().is_err(),
            "whole-library replacement must fail closed while a note privacy mutation is active"
        );
        drop(note_transition);

        let (_removed, library_transition) = runtime
            .begin_library_revocation()
            .expect("library transition");
        assert!(
            runtime.begin_note_revocation(&second_note).is_err(),
            "note privacy mutation must fail closed while whole-library replacement is active"
        );
        drop(library_transition);
    }

    #[test]
    fn library_revocation_transition_blocks_all_unlocks_until_replacement_finishes() {
        let runtime = VaultRuntime::default();
        let first = NoteId::parse("note-vault-library-revocation-a").expect("first note id");
        let second = NoteId::parse("note-vault-library-revocation-b").expect("second note id");
        runtime
            .insert_note_key(first.clone(), key())
            .expect("insert first");
        runtime
            .insert_note_key(second.clone(), key())
            .expect("insert second");

        let (removed, transition) = runtime
            .begin_library_revocation()
            .expect("begin library revocation");
        assert_eq!(removed, 2);
        assert!(
            runtime.insert_note_key(first.clone(), key()).is_err(),
            "old-library Unlock must stay blocked during replacement"
        );
        let third = NoteId::parse("note-vault-library-revocation-c").expect("third note id");
        assert!(
            runtime.insert_note_key(third.clone(), key()).is_err(),
            "library revocation must also block notes that were not previously unlocked"
        );

        drop(transition);
        runtime
            .insert_note_key(third.clone(), key())
            .expect("Unlock may resume after library replacement releases");
        assert!(runtime.remove_note_key(&third).expect("cleanup lock"));
    }

    #[test]
    fn lock_waits_for_active_read_permit_and_rejects_new_reads_after_revocation() {
        let runtime = Arc::new(VaultRuntime::default());
        let note_id = NoteId::parse("note-vault-revocation").expect("note id");
        runtime
            .insert_note_key(note_id.clone(), key())
            .expect("insert key");
        let access = runtime
            .acquire_note_access(&note_id)
            .expect("access")
            .expect("unlocked access");
        let permit = access.into_permit();

        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let runtime_for_lock = Arc::clone(&runtime);
        let note_for_lock = note_id.clone();
        let lock_thread = thread::spawn(move || {
            started_tx.send(()).expect("signal lock start");
            let result = runtime_for_lock.remove_note_key(&note_for_lock);
            done_tx.send(result).expect("signal lock finish");
        });

        started_rx.recv().expect("lock started");
        let mut revocation_visible = false;
        for _ in 0..2_000 {
            if !runtime.contains_note_key(&note_id).expect("contains key") {
                revocation_visible = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(
            revocation_visible,
            "Lock should publish revocation promptly"
        );
        assert!(
            runtime
                .acquire_note_access(&note_id)
                .expect("new access check")
                .is_none(),
            "revocation must stop new protected reads before Lock completes"
        );
        assert!(
            done_rx.try_recv().is_err(),
            "Lock must wait while an authorized protected read is still active"
        );

        drop(permit);
        assert!(
            done_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("lock completion")
                .expect("lock result"),
            "Lock should remove the unlocked note session"
        );
        lock_thread.join().expect("join lock thread");
        assert!(
            runtime
                .acquire_note_access(&note_id)
                .expect("post-lock access check")
                .is_none(),
            "no protected read permit may survive completed Lock"
        );
    }
}

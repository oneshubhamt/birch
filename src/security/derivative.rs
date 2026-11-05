//! Coordination for plaintext attachment use during ordinary -> Vault transitions.
//!
//! Persistent derivatives live outside SQLite (renditions, media compatibility,
//! staging and desktop handoffs). A note-protection transaction must therefore
//! stop *new* plaintext producers, wait for already-authorized producers to
//! finish, scrub their outputs, and only then reopen the boundary. The gate is
//! note-scoped so callers can re-authorize attachment state after a transition
//! without coupling native/cache code to Vault internals.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

use crate::domain::NoteId;

#[derive(Clone, Default)]
pub(super) struct PlaintextDerivativeGate {
    inner: Arc<PlaintextDerivativeGateInner>,
}

#[derive(Default)]
struct PlaintextDerivativeGateInner {
    state: Mutex<PlaintextDerivativeState>,
    changed: Condvar,
}

#[derive(Default)]
struct PlaintextDerivativeState {
    active: HashMap<NoteId, usize>,
    transitioning: HashSet<NoteId>,
    library_transitioning: bool,
}

/// Owned permit retained for the complete lifetime of a plaintext read or
/// derivative operation that began while the note was not transitioning.
///
/// The permit deliberately owns the shared gate rather than borrowing Storage,
/// allowing it to cross worker-thread and protocol request boundaries safely.
#[derive(Clone)]
pub(crate) struct PlaintextDerivativePermit {
    lease: Arc<PlaintextDerivativeLease>,
}

struct PlaintextDerivativeLease {
    gate: Arc<PlaintextDerivativeGateInner>,
    note_id: NoteId,
}

/// Exclusive note transition. While alive, no new derivative permit for the
/// note can be acquired and all permits that existed before construction have
/// already drained.
pub(super) struct PlaintextDerivativeTransition {
    gate: Arc<PlaintextDerivativeGateInner>,
    scope: PlaintextDerivativeTransitionScope,
    active: bool,
}

#[derive(Debug)]
enum PlaintextDerivativeTransitionScope {
    Note(NoteId),
    Notes(Vec<NoteId>),
    Library,
}

impl fmt::Debug for PlaintextDerivativeGate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaintextDerivativeGate")
    }
}

impl fmt::Debug for PlaintextDerivativePermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaintextDerivativePermit")
    }
}

impl PlaintextDerivativePermit {
    pub(crate) fn note_id(&self) -> &NoteId {
        &self.lease.note_id
    }
}

impl fmt::Debug for PlaintextDerivativeTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaintextDerivativeTransition")
    }
}

impl PlaintextDerivativeGate {
    fn state(&self) -> Result<MutexGuard<'_, PlaintextDerivativeState>, &'static str> {
        self.inner
            .state
            .lock()
            .map_err(|_| "plaintext derivative gate is poisoned")
    }

    pub(crate) fn acquire(
        &self,
        note_id: &NoteId,
    ) -> Result<PlaintextDerivativePermit, &'static str> {
        self.acquire_notes(std::slice::from_ref(note_id))?
            .into_iter()
            .next()
            .ok_or("plaintext derivative permit acquisition returned no permit")
    }

    /// Atomically acquire derivative permits for a known note set. The complete
    /// set is checked and incremented under one gate lock so a privacy transition
    /// cannot split a multi-note plaintext operation between notes.
    pub(crate) fn acquire_notes(
        &self,
        note_ids: &[NoteId],
    ) -> Result<Vec<PlaintextDerivativePermit>, &'static str> {
        let mut ids = note_ids.to_vec();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.dedup_by(|left, right| left.as_str() == right.as_str());
        if ids.is_empty() {
            return Err("plaintext derivative permit set requires at least one note");
        }

        let mut state = self.state()?;
        while state.library_transitioning
            || ids.iter().any(|note_id| state.transitioning.contains(note_id))
        {
            state = self
                .inner
                .changed
                .wait(state)
                .map_err(|_| "plaintext derivative gate is poisoned")?;
        }
        if ids.iter().any(|note_id| {
            state.active.get(note_id).copied().unwrap_or(0) == usize::MAX
        }) {
            return Err("plaintext derivative permit count overflowed");
        }
        for note_id in &ids {
            *state.active.entry(note_id.clone()).or_default() += 1;
        }
        drop(state);

        Ok(ids
            .into_iter()
            .map(|note_id| PlaintextDerivativePermit {
                lease: Arc::new(PlaintextDerivativeLease {
                    gate: Arc::clone(&self.inner),
                    note_id,
                }),
            })
            .collect())
    }

    pub(super) fn begin_transition(
        &self,
        note_id: &NoteId,
    ) -> Result<PlaintextDerivativeTransition, &'static str> {
        self.begin_transitions(std::slice::from_ref(note_id))
    }

    /// Atomically freeze every requested note before waiting for any existing
    /// producer to drain. This matters for purge/rollback: sequential gating
    /// would leave later notes able to start new plaintext work while the first
    /// note was still draining.
    pub(super) fn begin_note_transitions(
        &self,
        note_ids: &[NoteId],
    ) -> Result<PlaintextDerivativeTransition, &'static str> {
        self.begin_transitions(note_ids)
    }

    fn begin_transitions(
        &self,
        note_ids: &[NoteId],
    ) -> Result<PlaintextDerivativeTransition, &'static str> {
        let mut ids = note_ids.to_vec();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.dedup_by(|left, right| left.as_str() == right.as_str());
        if ids.is_empty() {
            return Err("plaintext derivative transition requires at least one note");
        }

        let mut state = self.state()?;
        // Note transitions and whole-library replacement share one gate. Wait
        // only for an internal gate transition that already owns one of these
        // scopes; coordinator-level conflicting privacy mutations are rejected
        // earlier by PrivacyCoordinator admission.
        while state.library_transitioning
            || ids
                .iter()
                .any(|note_id| state.transitioning.contains(note_id))
        {
            state = self
                .inner
                .changed
                .wait(state)
                .map_err(|_| "plaintext derivative gate is poisoned")?;
        }

        // Freeze the complete set in one critical section before waiting for a
        // single producer. New work for every affected note is blocked now.
        state.transitioning.extend(ids.iter().cloned());

        loop {
            if ids.iter().all(|note_id| {
                state.active.get(note_id).copied().unwrap_or_default() == 0
            }) {
                break;
            }
            state = match self.inner.changed.wait(state) {
                Ok(state) => state,
                Err(poisoned) => {
                    let mut recovered = poisoned.into_inner();
                    for note_id in &ids {
                        recovered.transitioning.remove(note_id);
                    }
                    self.inner.changed.notify_all();
                    return Err("plaintext derivative gate is poisoned");
                }
            };
        }

        let scope = if ids.len() == 1 {
            PlaintextDerivativeTransitionScope::Note(ids[0].clone())
        } else {
            PlaintextDerivativeTransitionScope::Notes(ids)
        };
        Ok(PlaintextDerivativeTransition {
            gate: Arc::clone(&self.inner),
            scope,
            active: true,
        })
    }

    #[cfg(test)]
    fn is_transitioning(&self, note_id: &NoteId) -> bool {
        self.state()
            .map(|state| state.transitioning.contains(note_id))
            .unwrap_or(false)
    }

    /// Stop every Rustnote plaintext derivative producer while a complete
    /// library database is replaced. New permits are blocked before existing
    /// permits drain, so no `.incoming`/image/import producer can publish an
    /// ownership row into the outgoing database after the restore safety
    /// snapshot has passed its zero-pending-work barrier.
    pub(super) fn begin_library_transition(
        &self,
    ) -> Result<PlaintextDerivativeTransition, &'static str> {
        let mut state = self.state()?;
        while state.library_transitioning || !state.transitioning.is_empty() {
            state = self
                .inner
                .changed
                .wait(state)
                .map_err(|_| "plaintext derivative gate is poisoned")?;
        }
        state.library_transitioning = true;

        loop {
            if state.active.values().all(|count| *count == 0) {
                break;
            }
            state = match self.inner.changed.wait(state) {
                Ok(state) => state,
                Err(poisoned) => {
                    let mut recovered = poisoned.into_inner();
                    recovered.library_transitioning = false;
                    self.inner.changed.notify_all();
                    return Err("plaintext derivative gate is poisoned");
                }
            };
        }

        Ok(PlaintextDerivativeTransition {
            gate: Arc::clone(&self.inner),
            scope: PlaintextDerivativeTransitionScope::Library,
            active: true,
        })
    }
}

impl Drop for PlaintextDerivativeLease {
    fn drop(&mut self) {
        let Ok(mut state) = self.gate.state.lock() else {
            return;
        };
        if let Some(active) = state.active.get_mut(&self.note_id) {
            *active = active.saturating_sub(1);
            if *active == 0 {
                state.active.remove(&self.note_id);
                self.gate.changed.notify_all();
            }
        }
    }
}

impl Drop for PlaintextDerivativeTransition {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Ok(mut state) = self.gate.state.lock() else {
            return;
        };
        match &self.scope {
            PlaintextDerivativeTransitionScope::Note(note_id) => {
                state.transitioning.remove(note_id);
            }
            PlaintextDerivativeTransitionScope::Notes(note_ids) => {
                for note_id in note_ids {
                    state.transitioning.remove(note_id);
                }
            }
            PlaintextDerivativeTransitionScope::Library => {
                state.library_transitioning = false;
            }
        }
        self.active = false;
        self.gate.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    fn note_id(label: &str) -> NoteId {
        NoteId::parse(format!("note-{label}")).expect("note id")
    }

    #[test]
    fn transition_waits_for_existing_permit_and_blocks_new_permits() {
        let gate = PlaintextDerivativeGate::default();
        let note_id = note_id("derivative-transition");
        let first = gate.acquire(&note_id).expect("first permit");

        let gate_for_transition = gate.clone();
        let note_for_transition = note_id.clone();
        let (transition_started_tx, transition_started_rx) = mpsc::channel();
        let (transition_ready_tx, transition_ready_rx) = mpsc::channel();
        let (release_transition_tx, release_transition_rx) = mpsc::channel();
        let transition_thread = thread::spawn(move || {
            transition_started_tx.send(()).expect("transition started");
            let transition = gate_for_transition
                .begin_transition(&note_for_transition)
                .expect("transition");
            transition_ready_tx.send(()).expect("transition ready");
            release_transition_rx.recv().expect("release transition");
            drop(transition);
        });

        transition_started_rx.recv().expect("transition start signal");
        assert!(transition_ready_rx.recv_timeout(Duration::from_millis(30)).is_err());
        drop(first);
        transition_ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("transition should drain first permit");

        let gate_for_second = gate.clone();
        let note_for_second = note_id.clone();
        let (second_ready_tx, second_ready_rx) = mpsc::channel();
        let second_thread = thread::spawn(move || {
            let permit = gate_for_second.acquire(&note_for_second).expect("second permit");
            second_ready_tx.send(()).expect("second permit ready");
            drop(permit);
        });
        assert!(second_ready_rx.recv_timeout(Duration::from_millis(30)).is_err());

        release_transition_tx.send(()).expect("release transition");
        second_ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("second permit should re-open after transition");
        transition_thread.join().expect("transition thread");
        second_thread.join().expect("second thread");
    }


    #[test]
    fn multi_note_transition_freezes_all_targets_before_waiting_for_first_drain() {
        let gate = PlaintextDerivativeGate::default();
        let first_note = note_id("multi-transition-a");
        let second_note = note_id("multi-transition-b");
        let first = gate.acquire(&first_note).expect("first producer");

        let gate_for_transition = gate.clone();
        let first_for_transition = first_note.clone();
        let second_for_transition = second_note.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let transition_thread = thread::spawn(move || {
            let transition = gate_for_transition
                .begin_note_transitions(&[first_for_transition, second_for_transition])
                .expect("multi-note transition");
            ready_tx.send(()).expect("transition ready");
            drop(transition);
        });

        for _ in 0..2_000 {
            if gate.is_transitioning(&first_note) {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(gate.is_transitioning(&first_note), "first target should be frozen");
        assert!(
            gate.is_transitioning(&second_note),
            "all multi-note targets must freeze atomically before any producer drain"
        );
        assert!(
            ready_rx.recv_timeout(Duration::from_millis(30)).is_err(),
            "transition must still wait for the active first-note producer"
        );

        let gate_for_second = gate.clone();
        let second_for_acquire = second_note.clone();
        let (producer_tx, producer_rx) = mpsc::channel();
        let producer_thread = thread::spawn(move || {
            let permit = gate_for_second
                .acquire(&second_for_acquire)
                .expect("second-note producer");
            producer_tx.send(()).expect("producer acquired");
            drop(permit);
        });
        assert!(
            producer_rx.recv_timeout(Duration::from_millis(30)).is_err(),
            "later target must not admit a new producer while an earlier target drains"
        );

        drop(first);
        ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("transition should complete after first producer drains");
        producer_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("producer should resume after transition releases");
        transition_thread.join().expect("transition thread");
        producer_thread.join().expect("producer thread");
    }

    #[test]
    fn library_transition_drains_all_notes_and_blocks_new_producers() {
        let gate = PlaintextDerivativeGate::default();
        let first_note = note_id("library-transition-a");
        let second_note = note_id("library-transition-b");
        let first = gate.acquire(&first_note).expect("first permit");
        let second = gate.acquire(&second_note).expect("second permit");

        let gate_for_transition = gate.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let transition_thread = thread::spawn(move || {
            let transition = gate_for_transition
                .begin_library_transition()
                .expect("library transition");
            ready_tx.send(()).expect("library transition ready");
            release_rx.recv().expect("release library transition");
            drop(transition);
        });

        assert!(ready_rx.recv_timeout(Duration::from_millis(30)).is_err());
        drop(first);
        assert!(ready_rx.recv_timeout(Duration::from_millis(30)).is_err());
        drop(second);
        ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("library transition should drain all producers");

        let third_note = note_id("library-transition-c");
        let gate_for_new = gate.clone();
        let (new_ready_tx, new_ready_rx) = mpsc::channel();
        let new_thread = thread::spawn(move || {
            let permit = gate_for_new.acquire(&third_note).expect("new permit");
            new_ready_tx.send(()).expect("new permit ready");
            drop(permit);
        });
        assert!(new_ready_rx.recv_timeout(Duration::from_millis(30)).is_err());

        release_tx.send(()).expect("release transition");
        new_ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("producer should resume after replacement");
        transition_thread.join().expect("transition thread");
        new_thread.join().expect("producer thread");
    }
}

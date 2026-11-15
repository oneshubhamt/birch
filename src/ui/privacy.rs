use std::collections::HashMap;

use dioxus::prelude::*;

use crate::{
    application::privacy::{
        PrivacyPublicationAuthorization, PrivacyPublicationLease as ApplicationPublicationLease,
        PrivacyPublicationService,
    },
    domain::NoteId,
};

/// UI-local immediate revocation state. Coordinator authorization remains the
/// authoritative transition boundary; this state closes the user-action ->
/// coordinator-admission interval without globally revoking unrelated notes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct UiPrivacyState {
    library_generation: u64,
    multi_note_generation: u64,
    note_generations: HashMap<NoteId, u64>,
    library_revocations: u32,
    multi_note_revocations: u32,
    note_revocations: HashMap<NoteId, u32>,
}

impl UiPrivacyState {
    fn note_generation(&self, note_id: &NoteId) -> u64 {
        self.note_generations.get(note_id).copied().unwrap_or(0)
    }

    fn note_revocation_active(&self, note_id: &NoteId) -> bool {
        self.library_revocations != 0
            || self.note_revocations.get(note_id).copied().unwrap_or(0) != 0
    }

    fn multi_note_revocation_active(&self) -> bool {
        self.library_revocations != 0
            || self.multi_note_revocations != 0
            || !self.note_revocations.is_empty()
    }

    fn open_note_revocation(&mut self, note_id: &NoteId) {
        self.multi_note_generation = self.multi_note_generation.wrapping_add(1);
        let generation = self.note_generations.entry(note_id.clone()).or_insert(0);
        *generation = generation.wrapping_add(1);
        let count = self.note_revocations.entry(note_id.clone()).or_insert(0);
        *count = count.saturating_add(1);
    }

    fn close_note_revocation(&mut self, note_id: &NoteId) {
        if let Some(count) = self.note_revocations.get_mut(note_id) {
            debug_assert!(*count > 0);
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.note_revocations.remove(note_id);
            }
        }
    }

    fn open_multi_note_revocation(&mut self) {
        self.multi_note_generation = self.multi_note_generation.wrapping_add(1);
        self.multi_note_revocations = self.multi_note_revocations.saturating_add(1);
    }

    fn close_multi_note_revocation(&mut self) {
        debug_assert!(self.multi_note_revocations > 0);
        self.multi_note_revocations = self.multi_note_revocations.saturating_sub(1);
    }

    fn open_library_revocation(&mut self) {
        self.library_generation = self.library_generation.wrapping_add(1);
        self.multi_note_generation = self.multi_note_generation.wrapping_add(1);
        self.library_revocations = self.library_revocations.saturating_add(1);
    }

    fn close_library_revocation(&mut self) {
        debug_assert!(self.library_revocations > 0);
        self.library_revocations = self.library_revocations.saturating_sub(1);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct UiPrivacyEpoch {
    signal: Signal<UiPrivacyState>,
}

#[derive(Clone, Debug)]
enum UiPublicationScope {
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

#[derive(Clone, Debug)]
enum UiRevocationScope {
    Note(NoteId),
    MultiNote,
    Library,
}

pub(super) struct UiPrivacyRevocationGuard {
    epoch: UiPrivacyEpoch,
    scope: UiRevocationScope,
    active: bool,
}

/// Explicit authorization for publishing protected-derived information.
///
/// Privacy scope is orthogonal to UI lifecycle guards. Note-local publication
/// never carries the notes-list generation merely because a sidebar exists.
#[derive(Clone)]
pub(super) struct PrivacyPublicationToken {
    authorization: PrivacyPublicationAuthorization,
    privacy_epoch: UiPrivacyEpoch,
    ui_scope: UiPublicationScope,
    lifecycle_generations: Vec<(Signal<u64>, u64)>,
}

#[derive(Clone)]
pub(super) struct NotePrivacyPublicationScope {
    privacy_epoch: UiPrivacyEpoch,
    privacy: PrivacyPublicationService,
    note_id: NoteId,
    ui_library_generation: u64,
    ui_note_generation: u64,
    lifecycle_generations: Vec<(Signal<u64>, u64)>,
}

impl std::fmt::Debug for PrivacyPublicationToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PrivacyPublicationToken([REDACTED])")
    }
}

/// Hook-level reactive view of one note's privacy state. The memo subscribes
/// to the canonical privacy signal, but components that read the memo are
/// notified only when this note's derived tuple changes. `use_reactive!` keeps
/// the non-reactive NoteId prop in the hook dependency set.
pub(super) fn use_note_privacy(epoch: UiPrivacyEpoch, note_id: NoteId) -> Memo<(u64, u64, bool)> {
    use_memo(use_reactive!(|note_id| epoch.observe_note(&note_id)))
}

impl UiPrivacyEpoch {
    pub(super) const fn new(signal: Signal<UiPrivacyState>) -> Self {
        Self { signal }
    }

    fn state(self) -> UiPrivacyState {
        self.signal.peek().clone()
    }

    /// Reactive dependency for an aggregate/multi-note surface.
    pub(super) fn observe_multi_note(self) -> (u64, u64, bool) {
        let state = self.signal.read();
        (
            state.library_generation,
            state.multi_note_generation,
            state.multi_note_revocation_active(),
        )
    }

    /// Reactive dependency for one note-local surface. This returns only values
    /// whose meaning belongs to this note (plus the whole-library generation).
    pub(super) fn observe_note(self, note_id: &NoteId) -> (u64, u64, bool) {
        let state = self.signal.read();
        (
            state.library_generation,
            state.note_generation(note_id),
            state.note_revocation_active(note_id),
        )
    }

    fn note_is_current(
        self,
        note_id: &NoteId,
        library_generation: u64,
        note_generation: u64,
    ) -> bool {
        let state = self.state();
        !state.note_revocation_active(note_id)
            && state.library_generation == library_generation
            && state.note_generation(note_id) == note_generation
    }

    fn multi_note_is_current(self, library_generation: u64, multi_note_generation: u64) -> bool {
        let state = self.state();
        !state.multi_note_revocation_active()
            && state.library_generation == library_generation
            && state.multi_note_generation == multi_note_generation
    }

    pub(super) fn begin_note_ui_revocation(self, note_id: NoteId) -> UiPrivacyRevocationGuard {
        let mut signal = self.signal;
        {
            let mut state = signal.write();
            state.open_note_revocation(&note_id);
        }
        UiPrivacyRevocationGuard {
            epoch: self,
            scope: UiRevocationScope::Note(note_id),
            active: true,
        }
    }

    pub(super) fn begin_multi_note_ui_revocation(self) -> UiPrivacyRevocationGuard {
        let mut signal = self.signal;
        {
            let mut state = signal.write();
            state.open_multi_note_revocation();
        }
        UiPrivacyRevocationGuard {
            epoch: self,
            scope: UiRevocationScope::MultiNote,
            active: true,
        }
    }

    pub(super) fn begin_library_ui_revocation(self) -> UiPrivacyRevocationGuard {
        let mut signal = self.signal;
        {
            let mut state = signal.write();
            state.open_library_revocation();
        }
        UiPrivacyRevocationGuard {
            epoch: self,
            scope: UiRevocationScope::Library,
            active: true,
        }
    }

    pub(super) fn note_publication_scope(
        self,
        privacy: PrivacyPublicationService,
        note_id: NoteId,
        lifecycle_generation: Signal<u64>,
    ) -> NotePrivacyPublicationScope {
        let state = self.state();
        NotePrivacyPublicationScope {
            privacy_epoch: self,
            privacy,
            note_id: note_id.clone(),
            ui_library_generation: state.library_generation,
            ui_note_generation: state.note_generation(&note_id),
            lifecycle_generations: vec![(lifecycle_generation, *lifecycle_generation.peek())],
        }
    }

    pub(super) fn capture_note_publication(
        self,
        privacy: &PrivacyPublicationService,
        note_id: &NoteId,
        lifecycle_generation: Signal<u64>,
    ) -> Option<PrivacyPublicationToken> {
        self.note_publication_scope(privacy.clone(), note_id.clone(), lifecycle_generation)
            .capture()
    }

    pub(super) fn capture_note_publication_without_lifecycle(
        self,
        privacy: &PrivacyPublicationService,
        note_id: &NoteId,
    ) -> Option<PrivacyPublicationToken> {
        let state = self.state();
        if state.note_revocation_active(note_id) {
            return None;
        }
        let authorization = privacy.capture_note(note_id).ok().flatten()?;
        let token = PrivacyPublicationToken {
            authorization,
            privacy_epoch: self,
            ui_scope: UiPublicationScope::Note {
                note_id: note_id.clone(),
                library_generation: state.library_generation,
                note_generation: state.note_generation(note_id),
            },
            lifecycle_generations: Vec::new(),
        };
        token.is_publishable().then_some(token)
    }

    pub(super) fn capture_multi_note_publication(
        self,
        privacy: &PrivacyPublicationService,
        list_generation: Signal<u64>,
    ) -> Option<PrivacyPublicationToken> {
        let state = self.state();
        if state.multi_note_revocation_active() {
            return None;
        }
        let authorization = privacy.capture_multi_note().ok().flatten()?;
        let token = PrivacyPublicationToken {
            authorization,
            privacy_epoch: self,
            ui_scope: UiPublicationScope::MultiNote {
                library_generation: state.library_generation,
                multi_note_generation: state.multi_note_generation,
            },
            lifecycle_generations: vec![(list_generation, *list_generation.peek())],
        };
        token.is_publishable().then_some(token)
    }

    pub(super) fn capture_multi_note_publication_with_lifecycle(
        self,
        privacy: &PrivacyPublicationService,
        list_generation: Signal<u64>,
        lifecycle_generation: Signal<u64>,
    ) -> Option<PrivacyPublicationToken> {
        let mut token = self.capture_multi_note_publication(privacy, list_generation)?;
        token
            .lifecycle_generations
            .push((lifecycle_generation, *lifecycle_generation.peek()));
        token.is_publishable().then_some(token)
    }
}

impl NotePrivacyPublicationScope {
    pub(super) fn capture(&self) -> Option<PrivacyPublicationToken> {
        if !self.privacy_epoch.note_is_current(
            &self.note_id,
            self.ui_library_generation,
            self.ui_note_generation,
        ) {
            return None;
        }
        let authorization = self.privacy.capture_note(&self.note_id).ok().flatten()?;
        let token = PrivacyPublicationToken {
            authorization,
            privacy_epoch: self.privacy_epoch,
            ui_scope: UiPublicationScope::Note {
                note_id: self.note_id.clone(),
                library_generation: self.ui_library_generation,
                note_generation: self.ui_note_generation,
            },
            lifecycle_generations: self.lifecycle_generations.clone(),
        };
        token.is_publishable().then_some(token)
    }
}

// Generations advance when revocation begins. Releasing a guard only closes
// the fail-closed window; it must not create a second generation change that
// unnecessarily invalidates freshly reconstructed post-transition UI.
impl Drop for UiPrivacyRevocationGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut signal = self.epoch.signal;
        let mut state = signal.write();
        match &self.scope {
            UiRevocationScope::Note(note_id) => state.close_note_revocation(note_id),
            UiRevocationScope::MultiNote => state.close_multi_note_revocation(),
            UiRevocationScope::Library => state.close_library_revocation(),
        }
        drop(state);
        self.active = false;
    }
}

impl std::fmt::Debug for UiPrivacyRevocationGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("UiPrivacyRevocationGuard([REDACTED])")
    }
}

impl PrivacyPublicationToken {
    fn local_generations_current(&self) -> bool {
        let privacy_current = match &self.ui_scope {
            UiPublicationScope::Note {
                note_id,
                library_generation,
                note_generation,
            } => self
                .privacy_epoch
                .note_is_current(note_id, *library_generation, *note_generation),
            UiPublicationScope::MultiNote {
                library_generation,
                multi_note_generation,
            } => self
                .privacy_epoch
                .multi_note_is_current(*library_generation, *multi_note_generation),
        };
        privacy_current
            && self
                .lifecycle_generations
                .iter()
                .all(|(signal, generation)| *signal.peek() == *generation)
    }

    pub(super) fn is_publishable(&self) -> bool {
        self.local_generations_current() && self.authorization.is_current()
    }

    pub(super) fn observe_publishable(&self) -> bool {
        let state = self.privacy_epoch.signal.read();
        let privacy_current = match &self.ui_scope {
            UiPublicationScope::Note {
                note_id,
                library_generation,
                note_generation,
            } => {
                !state.note_revocation_active(note_id)
                    && state.library_generation == *library_generation
                    && state.note_generation(note_id) == *note_generation
            }
            UiPublicationScope::MultiNote {
                library_generation,
                multi_note_generation,
            } => {
                !state.multi_note_revocation_active()
                    && state.library_generation == *library_generation
                    && state.multi_note_generation == *multi_note_generation
            }
        };
        drop(state);
        privacy_current
            && self
                .lifecycle_generations
                .iter()
                .all(|(signal, generation)| *signal.read() == *generation)
            && self.authorization.is_current()
    }

    pub(super) fn begin_publication(&self) -> Option<PrivacyPublicationLease> {
        if !self.local_generations_current() {
            return None;
        }
        let coordinator = self.authorization.begin_publication()?;
        if !self.local_generations_current() {
            drop(coordinator);
            return None;
        }
        Some(PrivacyPublicationLease {
            _coordinator: coordinator,
        })
    }

    pub(super) fn publish(&self, handoff: impl FnOnce()) -> bool {
        let Some(_publication_lease) = self.begin_publication() else {
            return false;
        };
        handoff();
        true
    }
}

pub(super) struct PrivacyPublicationLease {
    _coordinator: ApplicationPublicationLease,
}

impl std::fmt::Debug for PrivacyPublicationLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PrivacyPublicationLease([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(label: &str) -> NoteId {
        NoteId::parse(format!("note-{label}")).expect("note id")
    }

    #[test]
    fn note_revocation_changes_only_the_note_and_multi_note_generations() {
        let note_a = note("ui-scope-a");
        let note_b = note("ui-scope-b");
        let mut state = UiPrivacyState::default();
        let library_before = state.library_generation;
        let multi_before = state.multi_note_generation;
        let a_before = state.note_generation(&note_a);
        let b_before = state.note_generation(&note_b);

        state.open_note_revocation(&note_b);

        assert_eq!(state.library_generation, library_before);
        assert_eq!(state.note_generation(&note_a), a_before);
        assert_eq!(state.note_generation(&note_b), b_before.wrapping_add(1));
        assert_eq!(state.multi_note_generation, multi_before.wrapping_add(1));
        assert!(!state.note_revocation_active(&note_a));
        assert!(state.note_revocation_active(&note_b));
        assert!(state.multi_note_revocation_active());

        let b_generation_after_begin = state.note_generation(&note_b);
        let multi_generation_after_begin = state.multi_note_generation;
        state.close_note_revocation(&note_b);

        assert!(!state.note_revocation_active(&note_b));
        assert!(!state.multi_note_revocation_active());
        assert_eq!(state.note_generation(&note_b), b_generation_after_begin);
        assert_eq!(state.multi_note_generation, multi_generation_after_begin);
    }

    #[test]
    fn multi_note_revocation_changes_only_the_multi_note_generation() {
        let note_a = note("ui-multi-a");
        let mut state = UiPrivacyState::default();
        state.note_generations.insert(note_a.clone(), 5);
        let library_before = state.library_generation;
        let multi_before = state.multi_note_generation;
        let note_before = state.note_generation(&note_a);

        state.open_multi_note_revocation();

        assert_eq!(state.library_generation, library_before);
        assert_eq!(state.note_generation(&note_a), note_before);
        assert_eq!(state.multi_note_generation, multi_before.wrapping_add(1));
        assert!(!state.note_revocation_active(&note_a));
        assert!(state.multi_note_revocation_active());

        let multi_after_begin = state.multi_note_generation;
        state.close_multi_note_revocation();
        assert!(!state.multi_note_revocation_active());
        assert_eq!(state.multi_note_generation, multi_after_begin);
    }

    #[test]
    fn overlapping_note_revocations_remain_fail_closed_until_last_guard_releases() {
        let note_b = note("ui-overlap-b");
        let mut state = UiPrivacyState::default();

        state.open_note_revocation(&note_b);
        state.open_note_revocation(&note_b);
        assert!(state.note_revocation_active(&note_b));
        assert_eq!(state.note_revocations.get(&note_b).copied(), Some(2));

        state.close_note_revocation(&note_b);
        assert!(state.note_revocation_active(&note_b));
        assert_eq!(state.note_revocations.get(&note_b).copied(), Some(1));

        state.close_note_revocation(&note_b);
        assert!(!state.note_revocation_active(&note_b));
        assert!(!state.note_revocations.contains_key(&note_b));
    }

    #[test]
    fn library_revocation_invalidates_every_ui_scope_without_per_note_iteration() {
        let note_a = note("ui-library-a");
        let note_b = note("ui-library-b");
        let mut state = UiPrivacyState::default();
        state.note_generations.insert(note_a.clone(), 7);
        state.note_generations.insert(note_b.clone(), 11);
        let a_before = state.note_generation(&note_a);
        let b_before = state.note_generation(&note_b);
        let library_before = state.library_generation;
        let multi_before = state.multi_note_generation;

        state.open_library_revocation();

        assert_eq!(state.library_generation, library_before.wrapping_add(1));
        assert_eq!(state.multi_note_generation, multi_before.wrapping_add(1));
        assert_eq!(state.note_generation(&note_a), a_before);
        assert_eq!(state.note_generation(&note_b), b_before);
        assert!(state.note_revocation_active(&note_a));
        assert!(state.note_revocation_active(&note_b));
        assert!(state.multi_note_revocation_active());

        let library_after_begin = state.library_generation;
        let multi_after_begin = state.multi_note_generation;
        state.close_library_revocation();

        assert!(!state.note_revocation_active(&note_a));
        assert!(!state.note_revocation_active(&note_b));
        assert!(!state.multi_note_revocation_active());
        assert_eq!(state.library_generation, library_after_begin);
        assert_eq!(state.multi_note_generation, multi_after_begin);
    }
}

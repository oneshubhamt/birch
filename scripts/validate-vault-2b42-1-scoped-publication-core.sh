#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path


def read(path):
    return Path(path).read_text(errors='ignore')


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f"{label} missing: {marker}")


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f"{label} contains obsolete/forbidden marker: {marker}")


def segment(text, start, end):
    i = text.find(start)
    if i < 0:
        raise SystemExit(f"missing segment start: {start}")
    j = text.find(end, i + len(start))
    if j < 0:
        raise SystemExit(f"missing segment end after {start}: {end}")
    return text[i:j]

coord = read('src/security/privacy_coordinator.rs')
app = read('src/application/privacy.rs')
caps = read('src/storage/capabilities.rs')
ui = read('src/ui/privacy.rs')

# Coordinator state is explicitly scoped.
require(coord, [
    'library_publication_generation: u64',
    'multi_note_publication_generation: u64',
    'note_publication_generations: HashMap<NoteId, u64>',
    'active_multi_note_publications: usize',
    'active_note_publications: HashMap<NoteId, usize>',
    'enum PrivacyPublicationScope',
    'Note {',
    'MultiNote {',
    'enum PrivacyPublicationLeaseScope',
    'Note(NoteId)',
    'MultiNote',
], 'scoped coordinator publication state')

note_capture = segment(
    coord,
    'pub(crate) fn capture_note_publication_authorization(',
    'pub(crate) fn capture_multi_note_publication_authorization(',
)
require(note_capture, [
    'state.library_transition.is_some() || state.note_transitions.contains_key(note_id)',
    'library_generation: state.library_publication_generation',
    'note_publication_generations',
], 'note-local capture')
forbid(note_capture, [
    '!state.note_transitions.is_empty()',
    'active_multi_note_publications',
], 'note-local capture must not depend on unrelated notes')

multi_capture = segment(
    coord,
    'pub(crate) fn capture_multi_note_publication_authorization(',
    '#[cfg(test)]\n    pub(crate) fn note_publication_generation',
)
require(multi_capture, [
    'state.library_transition.is_some() || !state.note_transitions.is_empty()',
    'library_generation: state.library_publication_generation',
    'multi_note_generation: state.multi_note_publication_generation',
], 'multi-note capture')

admit_notes = segment(coord, 'fn admit_notes(', 'fn admit_library(')
require(admit_notes, [
    'state.multi_note_publication_generation =',
    'note_publication_generations',
    'state.active_multi_note_publications != 0',
    'active_note_publications',
    '.get(note_id)',
], 'note transition admission/drain')
forbid(admit_notes, [
    'state.active_note_publications.values().any',
    'state.library_publication_generation =',
], 'note transition must not drain/invalidate unrelated note-local publication')

admit_library = segment(coord, 'fn admit_library(', '/// Multi-note variant used by purge/rollback.')
require(admit_library, [
    'state.library_publication_generation =',
    'state.multi_note_publication_generation =',
    'state.active_multi_note_publications != 0',
    'state.active_note_publications.values().any',
], 'library transition invalidation/drain')

restore = segment(
    coord,
    'pub(crate) fn begin_protected_note_restore(',
    'pub(crate) fn begin_retained_protected_note_mutation(',
)
require(restore, [
    'state.multi_note_publication_generation =',
    'note_publication_generations',
    'state.active_multi_note_publications != 0',
    'active_note_publications',
    '.get(note_id)',
], 'protected revision Restore scoped admission')
forbid(restore, ['state.library_publication_generation ='], 'note-level Restore')

# Application/storage APIs expose only explicit note vs multi-note capture.
require(app, [
    'pub(crate) fn capture_note(',
    'pub(crate) fn capture_multi_note(',
], 'application publication API')
require(caps, [
    'impl PrivacyPublicationStore',
    'pub(crate) fn capture_note(',
    'pub(crate) fn capture_multi_note(',
], 'storage publication API')
forbid(app, [
    'pub(crate) fn capture(&self)',
    'capture_library_publication',
], 'application publication API')
forbid(caps, ['capture_publication_authorization()'], 'storage publication API')

# UI immediate revocation mirrors the same scopes and mutates atomically.
require(ui, [
    'library_generation: u64',
    'multi_note_generation: u64',
    'note_generations: HashMap<NoteId, u64>',
    'library_revocations: u32',
    'multi_note_revocations: u32',
    'note_revocations: HashMap<NoteId, u32>',
    'fn open_note_revocation(&mut self, note_id: &NoteId)',
    'fn open_multi_note_revocation(&mut self)',
    'fn open_library_revocation(&mut self)',
    'pub(super) fn begin_note_ui_revocation(',
    'pub(super) fn begin_multi_note_ui_revocation(',
    'pub(super) fn begin_library_ui_revocation(',
    'let mut state = signal.write();',
], 'UI scoped privacy state')

ui_note_begin = segment(ui, 'fn open_note_revocation(&mut self, note_id: &NoteId)', 'fn close_note_revocation(')
require(ui_note_begin, [
    'self.multi_note_generation = self.multi_note_generation.wrapping_add(1)',
    'self.note_generations.entry(note_id.clone())',
], 'UI note revocation generation')
forbid(ui_note_begin, ['self.library_generation ='], 'UI note revocation')

ui_library_begin = segment(ui, 'fn open_library_revocation(&mut self)', 'fn close_library_revocation(')
require(ui_library_begin, [
    'self.library_generation = self.library_generation.wrapping_add(1)',
    'self.multi_note_generation = self.multi_note_generation.wrapping_add(1)',
], 'UI library revocation generation')

ui_drop = segment(ui, 'impl Drop for UiPrivacyRevocationGuard', 'impl std::fmt::Debug for UiPrivacyRevocationGuard')
require(ui_drop, [
    'state.close_note_revocation(note_id)',
    'state.close_multi_note_revocation()',
    'state.close_library_revocation()',
], 'UI revocation release')
forbid(ui_drop, ['wrapping_add(1)'], 'UI revocation release must not advance generations twice')

# Direct adversarial tests required by 2b.42.1.
require(coord, [
    'fn lock_b_leaves_note_a_publication_valid_and_undrained()',
    'fn lock_b_invalidates_and_drains_note_b_publication()',
    'fn lock_b_invalidates_and_drains_multi_note_publication()',
    'fn library_restore_invalidates_and_drains_every_publication_scope()',
    'fn compound_plaintext_operation_finishes_after_lock_admission_without_circular_wait()',
    'fn ordinary_compound_plaintext_operation_finishes_before_admitted_protect()',
], '2b.42.1 adversarial coordinator tests')
require(ui, [
    'fn note_revocation_changes_only_the_note_and_multi_note_generations()',
    'fn multi_note_revocation_changes_only_the_multi_note_generation()',
    'fn overlapping_note_revocations_remain_fail_closed_until_last_guard_releases()',
    'fn library_revocation_invalidates_every_ui_scope_without_per_note_iteration()',
], '2b.42.1 UI state tests')

# The old global publication model must not survive in the authority layers.
for text, label in ((coord, 'coordinator'), (app, 'application privacy'), (caps, 'storage publication')):
    forbid(text, [
        'active_publications: usize',
        'privacy_generation: AtomicU64',
        'capture_library_publication',
        'capture_publication_authorization(',
    ], label)

print('Vault 2b.42.1 scoped publication core static contract passed.')
PY

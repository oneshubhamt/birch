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
            raise SystemExit(f"{label} contains obsolete marker: {marker}")

coord = read('src/security/privacy_coordinator.rs')
app = read('src/application/privacy.rs')
ui = read('src/ui/privacy.rs')

# Storage/application authorization exposes explicit note-local vs multi-note scopes.
require(coord, [
    'enum PrivacyPublicationScope',
    'Note {',
    'MultiNote {',
    'enum PrivacyPublicationLeaseScope',
    'Note(NoteId)',
    'MultiNote',
    'pub(crate) fn capture_note_publication_authorization(',
    'pub(crate) fn capture_multi_note_publication_authorization(',
], 'coordinator publication scopes')
require(app, [
    'pub(crate) struct PrivacyPublicationService',
    'pub(crate) struct PrivacyPublicationAuthorization',
    'pub(crate) fn capture_note(',
    'pub(crate) fn capture_multi_note(',
    'pub(crate) fn begin_publication(&self) -> Option<PrivacyPublicationLease>',
], 'application publication boundary')

# UI lifetime authority mirrors those scopes and keeps revocation local.
require(ui, [
    'pub(super) struct UiPrivacyState',
    'library_generation: u64',
    'multi_note_generation: u64',
    'note_generations: HashMap<NoteId, u64>',
    'library_revocations: u32',
    'multi_note_revocations: u32',
    'note_revocations: HashMap<NoteId, u32>',
    'pub(super) fn begin_note_ui_revocation(',
    'pub(super) fn begin_multi_note_ui_revocation(',
    'pub(super) fn begin_library_ui_revocation(',
    'pub(super) fn capture_note_publication(',
    'pub(super) fn capture_multi_note_publication(',
    'pub(super) fn begin_publication(&self) -> Option<PrivacyPublicationLease>',
    'pub(super) fn publish(&self, handoff: impl FnOnce()) -> bool',
], 'UI scoped publication token')

# Regression tests prove unrelated note-local publication survives Lock B while
# affected and aggregate publication is revoked/drained.
require(coord, [
    'lock_b_leaves_note_a_publication_valid_and_undrained',
    'lock_b_invalidates_and_drains_note_b_publication',
    'lock_b_invalidates_and_drains_multi_note_publication',
    'library_restore_invalidates_and_drains_every_publication_scope',
], 'scoped publication regressions')
require(ui, [
    'note_revocation_changes_only_the_note_and_multi_note_generations',
    'multi_note_revocation_changes_only_the_multi_note_generation',
    'overlapping_note_revocations_remain_fail_closed_until_last_guard_releases',
    'library_revocation_invalidates_every_ui_scope_without_per_note_iteration',
], 'UI scoped revocation regressions')

print('Vault 2b.3 publication-token regression guard passed (current scoped model).')
PY

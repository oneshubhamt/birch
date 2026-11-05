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
derivative = read('src/security/derivative.rs')
runtime = read('src/security/runtime.rs')

# Canonical coordinator remains the single privacy-transition authority.
require(coord, [
    'pub(crate) struct PrivacyCoordinator',
    'admission: Mutex<PrivacyAdmissionState>',
    'admission_changed: Condvar',
    'publication_drained: Condvar',
    'whole_library_operation_drained: Condvar',
    'operation_lock: Mutex<()>',
    'plaintext_derivatives: PlaintextDerivativeGate',
    'vault_runtime: Arc<VaultRuntime>',
    'pub(crate) fn begin_note_transition',
    'pub(crate) fn begin_note_transitions',
    'pub(crate) fn begin_library_transition',
    'pub(crate) fn begin_protected_note_restore',
    'pub(crate) fn begin_protected_note_mutation',
    'pub(crate) fn begin_protected_notes_mutation',
], 'PrivacyCoordinator authority')

# Admission remains fail-closed and note-scoped; whole-library conflicts stay exclusive.
require(coord, [
    'same_note_transition_conflicts_fail_closed',
    'overlapping_multi_note_transition_fails_without_partial_admission',
    'library_and_note_transitions_are_mutually_exclusive',
    'protected_restore_rejects_existing_same_note_transition_without_waiting',
    'admitted_lock_blocks_new_reader_before_vault_revocation_begins',
    'admitted_protect_blocks_new_plaintext_producer_before_derivative_drain',
    'lock_waits_for_existing_reader_before_storage_mutation',
    'protect_waits_for_plaintext_producer_and_blocks_new_producer',
    'compound_plaintext_operation_finishes_after_lock_admission_without_circular_wait',
    'ordinary_compound_plaintext_operation_finishes_before_admitted_protect',
], 'admission/ordering regressions')

# 2b.42 scoped publication is now part of the coordinator contract; the old
# one-global-generation/one-global-counter model must not return.
require(coord, [
    'library_publication_generation: u64',
    'multi_note_publication_generation: u64',
    'note_publication_generations: HashMap<NoteId, u64>',
    'active_multi_note_publications: usize',
    'active_note_publications: HashMap<NoteId, usize>',
    'pub(crate) fn capture_note_publication_authorization(',
    'pub(crate) fn capture_multi_note_publication_authorization(',
], 'scoped publication authority')
# Callers still cannot reconstruct the old component-level choreography.
production = '\n'.join(
    path.read_text(errors='ignore')
    for path in Path('src').rglob('*.rs')
    if not path.as_posix().startswith('src/security/')
)
forbid(production, [
    'begin_note_revocation(',
    'begin_note_revocations(',
    'begin_library_revocation(',
    'begin_plaintext_derivative_transition(',
    'begin_library_plaintext_derivative_transition(',
], 'production privacy callers')

require(derivative, ['pub(super) struct PlaintextDerivativeGate'], 'plaintext derivative gate')
require(runtime, ['pub(crate) struct VaultRuntime'], 'Vault runtime')

print('Vault 2b.2 privacy-authority regression guard passed (current scoped model).')
PY

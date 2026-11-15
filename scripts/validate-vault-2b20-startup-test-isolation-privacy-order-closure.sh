#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
root = Path('.')
app_paths = (root/'src/app_paths.rs').read_text()
output = (root/'src/native/output.rs').read_text()
locations = (root/'src/storage/locations.rs').read_text()
portability = (root/'src/storage/portability.rs').read_text()
journal = (root/'src/autosave/emergency_journal.rs').read_text()
coordinator = (root/'src/security/privacy_coordinator.rs').read_text()
protection = (root/'src/storage/protection.rs').read_text()

prod_cfg = '#[cfg(not(any(test, feature = "internal-test-support")))]'
test_cfg = '#[cfg(any(test, feature = "internal-test-support"))]'
for marker in (
    f'{prod_cfg}\npub(crate) fn configuration_directory() -> PathBuf',
    f'{test_cfg}\npub(crate) fn configuration_directory() -> PathBuf',
    'rustnote-test-config-{}-{nonce}',
    'TEST_CONFIGURATION_DIRECTORY',
):
    assert marker in app_paths, marker
for marker in (
    f'{prod_cfg}\nconst OUTPUT_JOURNAL_NAMESPACE: &str = "output-transfers";',
    f'{prod_cfg}\nfn journal_root() -> io::Result<PathBuf>',
    f'{test_cfg}\nfn journal_root() -> io::Result<PathBuf>',
    'rustnote-output-journal-test-{}',
):
    assert marker in output, marker

for marker in (
    'ensure_library_identity_with_history_at',
    'historical_library_id_at',
    'pristine_bootstrap_library_directory',
    'write_library_identity',
    'multiple library identities for one data directory',
    'deleted_library_recovers_historical_identity_for_the_same_data_path',
    'failed_bootstrap_identity_is_reconciled_only_while_library_is_pristine',
    'non_pristine_library_identity_is_never_rewritten_from_history',
    'automatic backup folder is already associated with another Rustnote library',
):
    assert marker in locations, marker
assert 'write_backup_location_history_at' in locations
assert 'pending_privacy_reset_directories' in locations
assert 'remove_file(&backup_location_history_path' not in locations

rollback = portability[portability.index('fn rollback_portability_import'):]
assert 'if transition_note_ids.is_empty() {' in rollback
assert 'None\n        } else {\n            Some(self.begin_multi_note_privacy_transition(' in rollback

success_test = journal[journal.index('fn protected_recovery_cleanup_removes_known_plaintext_record'):]
assert 'journal_plaintext_namespace(&root)' in success_test
assert 'let journal = owner.root().to_path_buf();' in success_test
assert 'let record = journal.join("note-record.json");' in success_test

single = coordinator[coordinator.index('pub(crate) fn begin_note_transition'):coordinator.index('pub(crate) fn begin_note_transitions')]
assert single.index('let admission = self.admit_note') < single.index('vault_runtime.begin_note_revocation(note_id)?') < single.index('plaintext_derivatives.begin_transition(note_id)?') < single.index('let operation_guard = self.lock_storage()?;')
multi = coordinator[coordinator.index('pub(crate) fn begin_note_transitions'):coordinator.index('pub(crate) fn begin_library_transition')]
assert multi.index('let admission = self.admit_notes') < multi.index('vault_runtime.begin_note_revocations(&ids)?') < multi.index('plaintext_derivatives.begin_note_transitions(&ids)?') < multi.index('let operation_guard = self.lock_storage()?;')
library = coordinator[coordinator.index('pub(crate) fn begin_library_transition'):coordinator.index('pub(crate) fn insert_note_key')]
assert library.index('plaintext_derivatives.begin_library_transition()?') < library.index('vault_runtime.begin_library_revocation()?')
retained = coordinator[coordinator.index('pub(crate) fn begin_retained_protected_note_mutation'):coordinator.index('pub(crate) fn begin_protected_note_mutation')]
for marker in (
    '.admission',
    'state.library_transition.is_some() || state.note_transitions.contains_key(note_id)',
    'read_permit_is_publishable(access.permit())?',
    'let operation_guard = self.lock_storage()?;',
    'drop(state);',
):
    assert marker in retained, marker
assert retained.index('.admission') < retained.index('state.library_transition.is_some()') < retained.index('let operation_guard = self.lock_storage()?;') < retained.index('drop(state);')


scrub = protection[protection.index('fn scrub_managed_plaintext_residue_after_hashes'):protection.index('pub(crate) fn protected_note_ids_for_privacy_recovery')]
for marker in (
    'self.discard_image_rendition_cache_strict(plaintext_hash)',
    'crate::media_compat::discard_cache_strict(&media_cache, plaintext_hash)',
    'self.discard_orphaned_quarantine_for_hashes_strict(&plaintext_hashes)',
    'discard_staged_derivatives_for_note(note_id)',
    'discard_handoffs_for_source_keys',
    'reset_backup_repository_for_vault_unlocked()',
    'scrub_sqlite_plaintext_residue_unlocked()',
):
    assert marker in scrub, marker
assert 'fn protected_note_scrubs_rustnote_managed_plaintext_residue()' in protection
print('Rustnote 2b.20 startup identity/test isolation/privacy ordering closure guard passed.')
PY

bash scripts/validate-vault-2b19-test-target-dead-code-closure.sh

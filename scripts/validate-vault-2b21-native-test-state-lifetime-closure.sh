#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
case "$BUILD_ID" in
  2b.21|2b.22|2b.23|2b.24|2b.25|2b.26|2b.27|2b.28|2b.29|2b.30|2b.31|2b.32|2b.33|2b.34|2b.35|2b.36|2b.37|2b.38|2b.39|2b.40|2b.41|2b.43) ;;
  *) echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1 ;;
esac

for required in NATIVE_COMPILE_2B21_PLAN.md NATIVE_COMPILE_2B21_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B21.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.21 closure file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path
root = Path('.')
journal = (root/'src/autosave/emergency_journal.rs').read_text()
coordinator = (root/'src/security/privacy_coordinator.rs').read_text()
protection = (root/'src/storage/protection.rs').read_text()
ownership = (root/'scripts/validate-security-e0-e3-ownership-2a16.sh').read_text()

journal_test = journal[journal.index('fn protected_recovery_cleanup_removes_known_plaintext_record'):]
for marker in (
    'journal_plaintext_namespace(&root)',
    'ensure_private_directory(&journal).expect("create managed journal root")',
    'std::fs::write(&record, b"plaintext recovery residue")',
):
    assert marker in journal_test, marker
assert journal_test.index('journal_plaintext_namespace(&root)') < journal_test.index('ensure_private_directory(&journal)') < journal_test.index('std::fs::write(&record')

multi_test = coordinator[coordinator.index('fn overlapping_multi_note_transition_fails_without_partial_admission'):coordinator.index('fn library_and_note_transitions_are_mutually_exclusive')]
for marker in (
    '.begin_note_transitions(',
    'drop(busy_guard);',
    '.begin_note_transition(&free_note, NotePrivacyTransition::Unlock)',
):
    assert marker in multi_test, marker
assert multi_test.index('.begin_note_transitions(') < multi_test.index('drop(busy_guard);') < multi_test.index('.begin_note_transition(&free_note')

ready = protection[protection.index('fn ensure_note_protection_ready'):protection.index('fn reconcile_protected_attachment_index')]
for marker in (
    'attachment_ingest_jobs WHERE note_id = ?1',
    'attachment_operations WHERE note_id = ?1',
    'managed_plaintext_staging WHERE note_id = ?1',
):
    assert marker in ready, marker
assert 'managed_plaintext_staging WHERE note_id IS NULL' not in ready

restore = protection[protection.index('fn protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables'):protection.index('fn protected_restore_normalization_reuses_retained_authorization_without_lock_cycle')]
for marker in (
    'let (restored, authorization) = storage',
    'drop(authorization);',
):
    assert marker in restore, marker
compact_restore = ''.join(restore.split())
lock_marker = 'storage.lock_protected_note(&note.id)'
assert lock_marker in compact_restore, lock_marker
assert compact_restore.index('drop(authorization);') < compact_restore.index(lock_marker)

for test_name, completion in (
    ('fn ordinary_attachment_command_resource_blocks_encrypt_until_command_lifetime_ends()', 'expect("protect completion")'),
    ('fn ordinary_note_export_resource_blocks_encrypt_until_native_lifetime_ends()', 'expect("protect completion")'),
    ('fn protect_waits_for_active_derivative_then_scrubs_cache_and_stage()', 'expect("protection completion")'),
):
    start = protection.index(test_name)
    tail = protection[start:]
    end = tail.index('\n    #[test]', 10) if '\n    #[test]' in tail[10:] else len(tail)
    body = tail[:end]
    assert 'recv_timeout(Duration::from_secs(15))' in body, test_name
    assert completion in body, test_name

assert "Protect must not block on unrelated library-scoped staging" in ownership
assert "managed_plaintext_staging WHERE note_id IS NULL" in ownership
print('Rustnote 2b.21 native test state/lifetime closure guard passed.')
PY

bash scripts/validate-vault-2b20-startup-test-isolation-privacy-order-closure.sh

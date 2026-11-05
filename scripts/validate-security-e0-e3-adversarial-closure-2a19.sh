#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# 2a.19 predates the scoped 2b.42 privacy architecture. Keep its adversarial
# intent while delegating publication/action/library boundaries to the current
# canonical guards rather than asserting obsolete global-token choreography.
bash scripts/validate-vault-2b42-1-scoped-publication-core.sh
bash scripts/validate-vault-2b42-5-vault-action-controller.sh
bash scripts/validate-vault-2b42-6-library-wide-operations.sh
bash scripts/validate-vault-2b42-8-no-vacuum-forensic.sh

python3 - <<'PY'
from pathlib import Path


def read(path):
    return Path(path).read_text(errors='ignore')


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f'{label} missing: {marker}')


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f'{label} retained obsolete marker: {marker}')

coord = read('src/security/privacy_coordinator.rs')
plaintext = read('src/storage/plaintext_staging.rs')
ingest = read('src/storage/attachment_ingest.rs')
attachments = read('src/storage/attachments.rs')
notes = read('src/storage/notes.rs')
protection = read('src/storage/protection.rs')
repository = read('src/storage/backup_repository.rs')
autosave = read('src/autosave/emergency_journal.rs')
ui_privacy = read('src/ui/privacy.rs')

require(coord, [
    'same_note_transition_conflicts_fail_closed',
    'library_and_note_transitions_are_mutually_exclusive',
    'compound_plaintext_operation_finishes_after_lock_admission_without_circular_wait',
    'multi_note_plaintext_operation_blocks_only_included_transitions',
    'lock_waits_for_existing_reader_before_storage_mutation',
    'protect_waits_for_plaintext_producer_and_blocks_new_producer',
    'whole_library_operation_serializes_against_note_privacy_transition',
    'whole_library_operation_serializes_against_library_restore',
], '2a.19 coordinator adversarial coverage')

require(ingest, [
    'reserve_attachment_owned_plaintext',
    'cleanup_owned_plaintext_after_error',
], 'attachment ingest ownership')
require(plaintext, [
    'recover_managed_plaintext_staging_unlocked',
], 'startup plaintext recovery')
require(attachments, [
    'discard_orphaned_quarantine_for_hashes_strict',
], 'attachment quarantine cleanup')
require(notes, [
    'permanent_delete_waits_for_plaintext_producer_then_scrubs_late_staging',
], 'hard-delete race regression')
require(protection, [
    'protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot',
    'no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups',
], 'SQLite/FTS/backup residue regressions')
require(repository, [
    'offline_historical_repository_keeps_privacy_obligation_until_reconnected',
    'interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear',
], 'backup interruption regressions')
require(autosave, [
    'startup_discards_corrupt_protected_records_before_quarantine',
    'startup_fails_closed_when_protected_root_record_cannot_be_removed',
], 'emergency journal residue regressions')

for path in Path('src').rglob('*.rs'):
    source = path.read_text(errors='ignore')
    if not path.as_posix().startswith('src/storage/'):
        for marker in ('rusqlite::Connection', 'Connection::open(', 'Connection::open_with_flags('):
            if marker in source:
                raise SystemExit(f'direct SQLite connection escaped storage boundary: {path}: {marker}')

print('E0-E3 2a.19 adversarial closure guard passed (current scoped model).')
PY

#!/bin/zsh
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2a.16" || "$BUILD_ID" == "2a.17" || "$BUILD_ID" == "2a.18" || "$BUILD_ID" == "2a.19" || "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

python3 - <<'PY'
from pathlib import Path

build_id = Path('build-id.txt').read_text().strip()

def text(path): return Path(path).read_text(errors='ignore')
def require(haystack, markers, label):
    for marker in markers:
        if marker not in haystack:
            raise SystemExit(f"{label} missing: {marker}")
def forbid(haystack, markers, label):
    for marker in markers:
        if marker in haystack:
            raise SystemExit(f"{label} retained forbidden marker: {marker}")
def ordered(haystack, markers, label):
    pos=-1
    for marker in markers:
        nxt=haystack.find(marker, pos+1)
        if nxt < 0:
            raise SystemExit(f"{label} missing/order violation: {marker}")
        pos=nxt

schema=text('src/storage/schema.rs'); init=text('src/storage/initialization.rs')
plaintext=text('src/security/plaintext.rs'); owner=text('src/storage/plaintext_staging.rs')
ingest=text('src/storage/attachment_ingest.rs'); attachments=text('src/storage/attachments.rs')
staging=text('src/native/staging.rs'); portability=text('src/storage/portability.rs')
bundle=text('src/storage/portable_bundle.rs'); protection=text('src/storage/protection.rs')
notes=text('src/storage/notes.rs'); backups=text('src/storage/backups.rs')
backup_repository=text('src/storage/backup_repository.rs'); main=text('src/main.rs')

if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(schema, [
        'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;',
        'CREATE TABLE managed_plaintext_staging',
        "'attachment_incoming', 'portability_import', 'portability_blob'",
        'transfer_destination  TEXT', "transfer_state        TEXT CHECK(transfer_state IS NULL OR transfer_state = 'prepared')",
        'CREATE TABLE vault_objects', 'CREATE TABLE vault_note_roots', 'CREATE TABLE vault_blob_refs',
        'PRAGMA user_version = 21;',
    ], 'durable plaintext ledger')
    if 'const SCHEMA_' in schema or 'protected_notes' in schema or 'protected_note_blobs' in schema:
        raise SystemExit('2b.5 retained obsolete pre-release schema compatibility')
elif build_id == '2b.4':
    require(schema, [
        'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;',
        'CREATE TABLE managed_plaintext_staging',
        "'attachment_incoming', 'portability_import', 'portability_blob'",
        'transfer_destination  TEXT', "transfer_state        TEXT CHECK(transfer_state IS NULL OR transfer_state = 'prepared')",
        'pub(super) const SCHEMA_19_TO_20', 'pub(super) const SCHEMA_20_TO_21',
        'CREATE TABLE vault_objects', 'CREATE TABLE vault_note_roots', 'CREATE TABLE vault_blob_refs',
        'DROP TABLE protected_notes;', 'PRAGMA user_version = 21;',
    ], 'durable plaintext ledger')
else:
    require(schema, [
        'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 20;',
        'CREATE TABLE managed_plaintext_staging',
        "'attachment_incoming', 'portability_import', 'portability_blob'",
        'transfer_destination  TEXT', "transfer_state        TEXT CHECK(transfer_state IS NULL OR transfer_state = 'prepared')",
        'pub(super) const SCHEMA_19_TO_20', 'PRAGMA user_version = 20;',
    ], 'durable plaintext ledger')

startup_markers = [
    'self.initialize_schema(&mut connection)?;',
    'self.recover_managed_plaintext_staging_unlocked()?;',
]
if build_id not in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    startup_markers.append('self.recover_legacy_plaintext_staging_unlocked()?;')
startup_markers.extend([
    'self.cleanup_untracked_incoming_files(&self.list_attachment_operations()?)?;',
    'self.recover_vault_operations_unlocked()?;',
])
ordered(init, startup_markers, 'startup ownership recovery')
if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') and 'recover_legacy_plaintext_staging_unlocked' in init:
    raise SystemExit('2b.5 must not retain obsolete pre-ledger plaintext staging recovery')

require(plaintext, [
    'pub(crate) struct OwnedPlaintext', 'PlaintextLifecycle::Reserved',
    'PlaintextCleanupAuthority::DurableManaged', 'PlaintextCleanupAuthority::VaultSession',
    'PlaintextCleanupAuthority::ProcessPrivate', 'reserve_vault_session',
    'process_private(', 'copy_from(', 'copy_and_hash_from(', 'create_file(',
    'rename_process_private(', 'rename_durable_to_preowned(', 'remove_file(',
    'failed_process_private_rename_retains_predecessor_ownership',
    'failed_vault_operation_rename_retains_predecessor_ownership',
    'process_private_delete_failure_keeps_retiring_state_and_owner',
], 'typed plaintext authority')
# The typed delete moves to Retiring before filesystem deletion; durable owner retirement is a separate guarded step.
remove=plaintext[plaintext.index('pub(crate) fn remove_file'):]
ordered(remove, ['self.lifecycle = PlaintextLifecycle::Retiring;', 'remove_managed_file(&self.path)?;'], 'typed delete transition')
require(plaintext, ['durable plaintext file must be removed before owner retirement'], 'durable delete-before-retire guard')
require(plaintext, [
    'pub(crate) fn establish(',
    'let root_nonempty = match fs::symlink_metadata(&root)',
    'if root_nonempty {',
    'durable plaintext namespace marker is missing for a nonempty managed root',
    'durable_namespace_establish_refuses_posthoc_adoption_of_nonempty_root',
], 'fail-closed durable namespace establishment')
forbid(plaintext, [
    'adopt_existing_with_proof',
    'allow_nonempty_adoption',
], 'obsolete durable namespace adoption')
forbid(backup_repository, [
    'InlineRepositoryOwner',
    'inline_owner',
    'adopt_existing_with_proof',
], 'obsolete backup repository adoption')

owner_markers = [
    'reserve_attachment_owned_plaintext', 'reserve_portability_owned_plaintext',
    'reserve_portability_blob_owned_plaintext', 'transfer_owned_plaintext_stage',
    'complete_owned_plaintext_transfer', 'rename_owned_restore_plaintext_to_canonical',
    'complete_owned_restore_plaintext_rename', 'recover_managed_plaintext_staging_unlocked',
    'failed_plaintext_cleanup_keeps_durable_owner_for_retry',
    'failed_durable_filesystem_rename_keeps_predecessor_and_prepared_transfer_owner',
    'restart_completes_prepared_transfer_when_canonical_bytes_are_missing',
    'restart_recovers_durable_rename_after_filesystem_transfer_before_owner_retirement',
    'prepared_transfer_pins_successor_owner_until_predecessor_is_reconciled',
    'corrupt_durable_transfer_record_fails_closed_without_forgetting_plaintext',
]
if build_id not in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    owner_markers.append('recover_legacy_plaintext_staging_unlocked')
require(owner, owner_markers, 'durable plaintext owner')
if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') and 'recover_legacy_plaintext_staging_unlocked' in owner:
    raise SystemExit('2b.5 durable owner still contains obsolete pre-ledger recovery')
ordered(owner[owner.index('pub(super) fn rename_owned_restore_plaintext_to_canonical'):], [
    'plaintext_transfer_successor_is_owned',
    "SET transfer_destination = ?1, transfer_state = 'prepared'",
    'owned.rename_durable_to_preowned(&destination)?;',
], 'recoverable durable rename preparation')
complete_restore = owner[owner.index('pub(super) fn complete_owned_library_blob_rename'):]
if build_id in ('2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    ordered(complete_restore, [
        'owned.ensure_path_absent()?;', 'plaintext_transfer_successor_is_owned',
        'transaction_with_behavior(TransactionBehavior::Immediate)', 'owned.ensure_path_absent()?;',
        'DELETE FROM managed_plaintext_staging WHERE id = ?1',
        'retire_transferred_durable_owner()?', 'transaction.commit()?;',
    ], 'predecessor retirement after successor proof')
    if 'source_path.exists()' in complete_restore:
        raise SystemExit('2b.6 predecessor retirement regressed to ambiguous Path::exists()')
else:
    ordered(complete_restore, [
        'if source_path.exists()', 'plaintext_transfer_successor_is_owned',
        'DELETE FROM managed_plaintext_staging WHERE id = ?1',
        'retire_transferred_durable_owner()',
    ], 'predecessor retirement after successor proof')

ordered(ingest[ingest.index('pub(crate) fn prepare_attachment_import_profiled'):], [
    'reserve_attachment_owned_plaintext(note_id, &staging_relative_path)?',
    'owned_plaintext.copy_from(source)',
    'insert_attachment_ingest_job(&job, &mut owned_plaintext)',
], 'async attachment pre-creation ownership')
ordered(ingest[ingest.index('fn insert_attachment_ingest_job'):], [
    'INSERT INTO attachment_ingest_jobs(', 'transfer_owned_plaintext_stage(&transaction, owned_plaintext)?;',
    'transaction.commit()?;', 'complete_owned_plaintext_transfer(owned_plaintext)?;',
], 'async attachment owner transfer')
if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    ordered(ingest[ingest.index('fn finalize_attachment_ingest_job'):], [
        'acquire_plaintext_operation_for_note(&job.note_id)?;', 'sha256_file(&staging_path)?;',
        'finish_staged_attachment_import_profiled(',
    ], 'attachment finalization compound-authorization lifetime')
else:
    ordered(ingest[ingest.index('fn finalize_attachment_ingest_job'):], [
        'acquire_plaintext_derivative_for_note(&job.note_id)?;', 'sha256_file(&staging_path)?;',
        'finish_staged_attachment_import_profiled(',
    ], 'attachment finalization permit lifetime')
if 'let _ = crate::native::privacy::remove_file' in ingest or 'let _ = fs::remove_file' in ingest:
    raise SystemExit('attachment ingest contains best-effort plaintext deletion')

ordered(attachments[attachments.index('fn add_attachment_file_internal'):], [
    'reserve_attachment_owned_plaintext(note_id, &staging_relative_path)?',
    'owned_plaintext.copy_and_hash_from(source)', 'finish_staged_attachment_import(',
], 'direct attachment pre-creation ownership')
ordered(attachments[attachments.index('pub(super) fn prepare_attachment_add'):], [
    'insert_attachment_operation(', 'transfer_owned_plaintext_stage(&transaction, owned_plaintext)?;',
    'transaction.commit()?;', 'complete_owned_plaintext_transfer(owned_plaintext)?;',
], 'direct attachment owner transfer')
require(attachments, ['pending_managed_attachment_staging_paths()?', 'attachment incoming staging contains an unexpected non-file entry'], 'incoming cleanup ledger awareness')

claim=staging[staging.index('fn claim_registered_staged_image_file'):staging.index('fn discard_staged_image_file')]
ordered(claim, [
    'let mut stages = lock_derivative_stage_registry();', 'OwnedPlaintext::process_private(',
    '.rename_process_private(claimed.clone())', 'stage.path = claimed.clone();',
], 'image claim typed ownership transfer')
require(staging, ['image_claim_updates_registered_ownership_to_claimed_path', 'process_session_root(&root, "archive restore staging")'], 'process-private ownership regressions')
require(main, ['cleanup_stale_archive_restore_stages()'], 'startup archive restore cleanup')

require(backups, [
    'allocate_restore_stage_directory("restore-stage")?', 'allocate_restore_stage_directory("restore-rollback")?',
    'reserve_restore_blob_temporary_owned_plaintext(sha256)?',
    'rename_owned_restore_plaintext_to_canonical(', 'complete_owned_restore_plaintext_rename(',
    'discard_restore_stage_directory(&staged.directory)',
], 'complete archive restore ownership')
require(backup_repository, ['archive_restore_process_root()', 'discard_restore_stage_directory(&staged.directory)'], 'repository restore staging ownership')

ordered(portability[portability.index('fn import_planned_bundle'):], [
    'self.lock_operations()?;', 'reserve_portability_owned_plaintext(&staged_name)?',
    'owned_plaintext.copy_and_hash_from(source)?', 'import_portable_bundle_matching_staged(owned_plaintext.path(), expected_semantic_sha256)',
    'discard_owned_plaintext(&mut owned_plaintext)',
], 'bundle archive ownership')
materialize=bundle[bundle.index('fn materialize_bundle_blobs'):bundle.index('fn import_verified_bundle')]
ordered(materialize, ['reserve_portability_blob_owned_plaintext(&relative)?', 'owned_plaintext.create_file()'], 'bundle blob pre-creation ownership')
require(bundle, ['referenced_blob_hashes', 'is not referenced by any imported attachment or revision', 'manifest_rejects_surplus_unreferenced_blob'], 'bundle rejects ownerless blobs')
ordered(bundle[bundle.index('fn import_verified_bundle'):], [
    'insert_bundle_attachments(', 'insert_bundle_revisions(', 'transfer_owned_plaintext_stage(&transaction, &mut blob.owned_plaintext)?;',
    'transaction.commit()?;', 'complete_owned_plaintext_transfer(&mut blob.owned_plaintext)?;',
], 'bundle blob canonical owner transfer')

rollback=portability[portability.index('fn rollback_portability_import'):]
ordered(rollback, [
    'begin_multi_note_privacy_transition(', 'NotePrivacyTransition::Delete',
    'retire_note_plaintext_before_delete_unlocked(note_id)?;',
    'ensure_portability_blob_plaintext_stage_in_transaction(', 'transaction.commit()?;',
], 'import rollback canonical transition + ownership')
require(portability, ['undo_import_keeps_blob_cleanup_owned_until_unlink_succeeds', 'plaintext blob cleanup remains durably owned'], 'portability undo ownership regression')

protect=protection[protection.index('pub(crate) fn protect_note_with_password'):]
ordered(protect, ['begin_note_privacy_transition(', 'NotePrivacyTransition::Protect', 'discard_managed_plaintext_staging_for_note(note_id)?;', 'ensure_note_protection_ready(&connection, note_id)?;'], 'Protect staging retirement')
require(protection, ['SELECT COUNT(*) FROM managed_plaintext_staging WHERE note_id = ?1'], 'Protect note-scoped ledger barrier')
forbid(protection, ['(SELECT COUNT(*) FROM managed_plaintext_staging WHERE note_id IS NULL)'], 'Protect must not block on unrelated library-scoped staging')
require(backups, ['SELECT COUNT(*) FROM managed_plaintext_staging'], 'snapshot ledger barrier')

permanent=notes[notes.index('pub fn permanently_delete_trashed_note'):notes.index('pub(super) fn hard_delete_note_unlocked')]
ordered(permanent, ['begin_note_privacy_transition(', 'NotePrivacyTransition::Delete', 'self.hard_delete_note_unlocked(note_id)'], 'permanent-delete coordinator boundary')
retire=notes[notes.index('pub(super) fn retire_note_plaintext_before_delete_unlocked'):notes.index('pub(super) fn hard_delete_note_unlocked')]
ordered(retire, ['SELECT id FROM attachments WHERE note_id = ?1 ORDER BY id', 'discard_managed_plaintext_staging_for_note(note_id)?;', 'discard_attachment_ingests_for_note(note_id)?;', 'discard_attachment_operation_staging_for_note(note_id)?;', 'discard_staged_derivatives_for_note(note_id.as_str())', 'discard_staged_derivatives_for_attachments(&attachment_ids)'], 'complete note plaintext retirement helper')
if '.list_attachments(note_id)?' in retire:
    raise SystemExit('note deletion retirement re-enters public protected attachment access')
hard=notes[notes.index('pub(super) fn hard_delete_note_unlocked'):]
ordered(hard, ['retire_note_plaintext_before_delete_unlocked(note_id)?;', 'DELETE FROM notes_fts'], 'hard-delete file-before-owner-row cleanup')
purge=notes[notes.index('pub fn purge_expired_notes'):notes.index('pub fn permanently_delete_trashed_note')]
ordered(purge, ['begin_multi_note_privacy_transition(', 'NotePrivacyTransition::Delete', 'still_expired', 'retire_note_plaintext_before_delete_unlocked(note_id)?;', 'DELETE FROM attachment_operations', 'DELETE FROM notes WHERE id IN'], 'Trash purge transition/revalidation/file-before-owner-row cleanup')
require(notes, ['permanent_delete_waits_for_plaintext_producer_then_scrubs_late_staging'], 'delete transition regression')
require(init, ['let purged_expired_notes = self.purge_expired_notes()?;'], 'maintenance lock-order split')

PY

echo "E0-E3 2a.16 canonical plaintext ownership guard passed."

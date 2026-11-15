#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
case "$BUILD_ID" in 2b.8|2b.9|2b.10|2b.11|2b.12|2b.13|2b.14|2b.15|2b.16|2b.17|2b.18|2b.19|2b.20|2b.21|2b.22|2b.23|2b.24|2b.25|2b.26|2b.27|2b.28|2b.29|2b.30|2b.31|2b.32|2b.33|2b.34|2b.35|2b.36|2b.37|2b.38|2b.39|2b.40|2b.41|2b.43) ;; *) echo "Expected build 2b.8+" >&2; exit 1 ;; esac

for required in \
  src/app_paths.rs \
  src/native/output.rs \
  src/security/plaintext.rs \
  src/autosave/emergency_journal.rs \
  src/native/rendition.rs \
  src/media_compat.rs; do
  [[ -f "$required" ]] || { echo "Missing 2b.8 closure file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path


def read(path):
    return Path(path).read_text(errors='ignore')


def require(source, markers, label):
    for marker in markers:
        if marker not in source:
            raise SystemExit(f'{label} missing: {marker}')


def ordered(source, markers, label):
    cursor = 0
    for marker in markers:
        pos = source.find(marker, cursor)
        if pos < 0:
            raise SystemExit(f'{label} missing/out of order: {marker}')
        cursor = pos + len(marker)


def function_segment(source, signature, next_signatures=()):
    start = source.find(signature)
    if start < 0:
        raise SystemExit(f'missing function for static audit: {signature}')
    ends = [source.find(next_sig, start + len(signature)) for next_sig in next_signatures]
    ends = [end for end in ends if end >= 0]
    return source[start:min(ends) if ends else len(source)]

owned = read('src/security/plaintext.rs')
output = read('src/native/output.rs')
native_mod = read('src/native/mod.rs')
main = read('src/main.rs')
atomic = read('src/storage/atomic_output.rs')
attachments = read('src/storage/attachments.rs')
repository = read('src/storage/backup_repository.rs')
backups = read('src/storage/backups.rs')
backup_support = read('src/storage/backup_support.rs')
portability = read('src/storage/portability.rs')
init = read('src/storage/initialization.rs')
journal = read('src/autosave/emergency_journal.rs')
rendition = read('src/native/rendition.rs')
media = read('src/media_compat.rs')
protection = read('src/storage/protection.rs')
coordinator = read('src/security/privacy_coordinator.rs')
history = read('src/storage/history.rs')
private_temp = read('src/native/private_temp.rs')

# O1-O4: one durable output transaction authority, owned-before-plaintext and
# recoverable across replacement boundaries.
require(output, [
    'pub(crate) struct ExternalFileOutputTransaction',
    'pub(crate) struct ExternalDirectoryOutputTransaction',
    'crate::app_paths::durable_configuration_directory()?.join(OUTPUT_JOURNAL_NAMESPACE)',
    'OUTPUT_LEDGER_LOCK_FILE', 'lock_output_ledger(&root)',
    'ensure_destination_available_locked(&root, &record.destination)',
    'OwnedPlaintext::durable_external_output_reserved',
    'capture_external_predecessor', 'publish_external_output',
    'recover_stale_output_transactions',
    'try_lock()', 'fs::TryLockError::WouldBlock',
    'file.lock()?', 'file.sync_all()?', 'sync_directory(&root)?',
    'OUTPUT_RECORD_CHECKSUM_DOMAIN', 'validate_record_checksum(record)?',
    'output_parent_available(&record)',
    'preserve_for_recovery',
], 'external output transaction authority')
if 'rustnote_temp_directory(OUTPUT_JOURNAL_NAMESPACE)' in output:
    raise SystemExit('output recovery ledger regressed to OS-temporary storage')

file_begin = function_segment(output, 'pub(crate) fn begin(destination: &Path)', ('pub(crate) fn temporary_path',))
ordered(file_begin, [
    'let (journal_path, journal_lock) = persist_record(&record)?;',
    'OwnedPlaintext::durable_external_output_reserved(',
    'OwnedPlaintext::durable_external_output_reserved(previous',
], 'external file ownership reservation')
require(output, ['.rustnote-output-{operation_id}.dir.tmp'], 'authority-derived directory output staging')
directory_commit = function_segment(output, 'pub(crate) fn commit(mut self) -> io::Result<PathBuf>', ('fn finish_transaction',))
ordered(directory_commit, [
    'if path_exists(&self.record.destination)?',
    'self.temporary',
    '.publish_external_output(&self.record.destination)?',
], 'directory output external-race exclusion')
file_commit = function_segment(output, 'pub(crate) fn commit(mut self) -> io::Result<()>', ('fn finish_transaction',))
ordered(file_commit, [
    'self.predecessor.is_none() && path_exists(&self.record.destination)?',
    'predecessor.capture_external_predecessor(&self.record.destination)?;',
    'sync_directory(parent_of(&self.record.destination)?)',
    'self.temporary.publish_external_output(&self.record.destination)?;',
    'sync_directory(parent_of(&self.record.destination)?)',
    'predecessor.remove_file()',
    'sync_directory(parent_of(&self.record.destination)?)',
    'self.finish_transaction()',
], 'external file crash-durable commit order')

persist = function_segment(output, 'fn persist_record', ('fn read_record',))
ordered(persist, [
    'lock_output_ledger(&root)?',
    'ensure_destination_available_locked(&root, &record.destination)?',
    'create_private_file(&path)?', 'file.lock()?', 'file.write_all(&bytes)?',
    'file.sync_all()?', 'sync_directory(&root)?',
], 'output durable-record publication')
recover = function_segment(output, 'pub fn recover_stale_output_transactions', ('fn reconcile_record_state',))
ordered(recover, ['lock_output_ledger(&root)', 'try_lock()', 'WouldBlock) => continue', 'read_record(&path)', 'output_parent_available(&record)', 'reconcile_record_state', 'retire_journal_locked'], 'output live-owner/offline-parent recovery exclusion')

checksum = function_segment(output, 'fn record_checksum', ('fn validate_record_checksum',))
require(checksum, ['OutputRecordChecksum', 'Sha256::new()', 'OUTPUT_RECORD_CHECKSUM_DOMAIN'], 'output recovery checksum')
availability = function_segment(output, 'fn ensure_destination_available_locked', ('fn validate_output_parent',))
require(availability, ['read_record(&path)?', 'record.destination == destination', 'io::ErrorKind::WouldBlock'], 'same-destination output exclusion')

# Raw rename/copy of external output staging must remain centralized in
# OwnedPlaintext, not reappear in the transaction coordinator itself.
production_output = output.split('#[cfg(test)]', 1)[0]
for forbidden in ('fs::rename(', 'fs::copy('):
    if forbidden in production_output:
        raise SystemExit(f'external output coordinator bypasses OwnedPlaintext: {forbidden}')

require(owned, [
    'PlaintextScope::ExternalOutput',
    'PlaintextCleanupAuthority::DurableExternalOutput',
    'pub(crate) fn durable_external_output_reserved',
    'pub(crate) fn durable_external_output_existing',
    'pub(crate) fn capture_external_predecessor',
    'pub(crate) fn publish_external_output',
    'pub(crate) fn create_external_output_directory',
    'let canonical_root = fs::canonicalize(&self.root)?;',
    'let canonical_parent = fs::canonicalize(parent)?;',
    'durable plaintext namespace marker is missing or invalid',
    'managed plaintext parent escapes durable namespace',
], 'OwnedPlaintext external-output lifecycle')

# O5/O11: callers delegate to one authority; the old age/name based markdown
# scavenger is gone rather than becoming a second cleanup engine.
require(native_mod, ['output::copy_file_atomically(source, destination)'], 'native Save-As delegation')
require(atomic, [
    'ExternalFileOutputTransaction', 'ExternalDirectoryOutputTransaction',
    'transaction.create_file()?', 'self.transaction.commit()?',
], 'storage atomic output delegation')
require(attachments, [
    'ExternalFileOutputTransaction::begin(destination)?',
    'crate::native::output::copy_file_atomically(source, destination)?',
], 'storage atomic replace/copy delegation')
require(repository, ['ExternalFileOutputTransaction::begin(destination)?', 'transaction.copy_and_hash_from(source)?'], 'verified repository copy delegation')
require(backups, ['ExternalFileOutputTransaction::begin(destination)?', 'reserve_process_private_snapshot()'], 'archive output/snapshot ownership')
if 'cleanup_stale_markdown_exports' in backup_support or 'cleanup_stale_markdown_exports' in backups:
    raise SystemExit('obsolete markdown temp scavenger remains beside output transaction authority')

# O6: emergency journal is a durable namespace, marker outside root, and raw
# journal create/rename/delete no longer forms a second plaintext lifecycle.
require(journal, [
    'JOURNAL_OWNER_MARKER', 'JOURNAL_OWNER_IDENTITY',
    'DurablePlaintextNamespace::establish(',
    'data_directory.join(JOURNAL_OWNER_MARKER)',
    'namespace.reserve(path.to_path_buf())', 'owned.create_file()',
    'rename_durable_namespace(destination.to_path_buf())',
    'namespace.adopt_existing(path.to_path_buf())', 'owned.remove_file()',
    'rename_durable_namespace(destination.clone())',
], 'emergency journal typed ownership')
write_json = function_segment(journal, 'fn write_json_file', ('fn replace_temporary_file',))
ordered(write_json, ['remove_file_if_present(path)?', 'namespace.reserve(path.to_path_buf())', 'owned.create_file()', 'file.sync_all()'], 'journal owner-before-materialization')
production_journal = journal.split('#[cfg(test)]', 1)[0]
for forbidden in ('fs::rename(', 'OpenOptions::new()'):
    if forbidden in production_journal:
        raise SystemExit(f'emergency journal retains raw plaintext lifecycle primitive: {forbidden}')

# O7: persistent image/media derivatives reserve namespace ownership before the
# external converter/remuxer can materialize bytes, then publish through typed rename.
require(rendition, [
    'pub fn create_persistent_compatible_image_rendition',
    'pub(crate) fn persistent_rendition_owner',
    'owner.reserve(temporary.clone())',
    'render_image_to_png(&source, &temporary',
    'temporary_owner.verify_materialized()',
    'temporary_owner.rename_durable_namespace(destination.to_path_buf())',
], 'persistent image rendition ownership')
require(media, [
    'fn media_plaintext_namespace', 'DurablePlaintextNamespace::establish(',
    'let mut temporary_owner = owner.reserve(temporary.clone())?;',
    'remux_matroska(&resource.path, &temporary, profile)',
    'temporary_owner.verify_materialized()',
    'temporary_owner.rename_durable_namespace(destination.clone())',
    'fn ensure_cache_marker(marker: &Path)',
    'let mut owned = owner.reserve(marker.to_path_buf())?;',
    'let mut file = owned.create_file()?;',
    'fn remove_cache_marker(marker: &Path)',
    'let mut owned = owner.adopt_existing(path.to_path_buf())?;',
    'owned.remove_file()',
], 'persistent media derivative ownership')
production_media = media.split('#[cfg(test)]', 1)[0]
if 'fs::write(&marker' in production_media or 'fs::remove_file(&invalid_marker)' in production_media:
    raise SystemExit('media invalidation marker bypasses durable cache ownership')

# O8: a failed derived cleanup cannot happen after the final canonical plaintext
# retry anchor was moved/retired. Enforce order in each destructive path.
quarantine = function_segment(attachments, 'pub(super) fn quarantine_blob_if_unreferenced', ('pub(super) fn discard_canonical_blob_if_unreferenced_strict',))
ordered(quarantine, ['discard_image_rendition_cache_strict(sha256)?', 'discard_cache_strict(', 'self.move_blob_to_quarantine(&path)?'], 'attachment quarantine derivative-first order')
hard_delete = function_segment(attachments, 'pub(super) fn discard_canonical_blob_if_unreferenced_strict', ('pub(super) fn discard_image_rendition_cache_strict',))
ordered(hard_delete, ['discard_image_rendition_cache_strict(sha256)?', 'discard_cache_strict(', 'self.discard_owned_plaintext(&mut owned)?'], 'attachment hard-delete derivative-first order')
orphan_sweep = function_segment(attachments, 'pub(super) fn quarantine_orphan_blobs_unlocked', ('pub(super) fn discard_orphaned_quarantine_for_hashes_strict',))
ordered(orphan_sweep, ['discard_image_rendition_cache_strict(&sha256)?', 'discard_cache_strict(', 'self.move_blob_to_quarantine(&blob.path())?'], 'orphan derivative-first order')
quarantine_retire = function_segment(attachments, 'pub(super) fn discard_orphaned_quarantine_for_hashes_strict', ('pub(super) fn purge_old_quarantine_files',))
ordered(quarantine_retire, ['discard_image_rendition_cache_strict(sha256)?', 'discard_cache_strict(', 'self.discard_owned_plaintext(&mut owned)?'], 'quarantine final-owner derivative-first order')
purge_quarantine = function_segment(attachments, 'pub(super) fn purge_old_quarantine_files', ('fn attachment_blob_verification_stamp',))
ordered(purge_quarantine, ['validate_sha256(sha256)?', 'discard_image_rendition_cache_strict(sha256)?', 'discard_cache_strict(', 'self.discard_owned_plaintext(&mut owned)?'], 'quarantine retention derivative-first order')

# O9: producer-vs-retirement synchronization belongs to PrivacyCoordinator.
require(coordinator, [
    'pub(crate) struct PlaintextDerivativeCleanupGuard',
    'pub(crate) fn begin_plaintext_derivative_cleanup',
    'let state = self.wait_for_note_admission_clear(note_id)?;',
    'self.inner.plaintext_derivatives.begin_transition(note_id)?;',
    'pub(crate) fn begin_library_plaintext_derivative_cleanup',
    'self.inner.plaintext_derivatives.begin_library_transition()?;',
    'derivative_cleanup_drains_existing_producer_and_blocks_new_producer',
], 'coordinator derivative cleanup authority')
remove_attachment = function_segment(attachments, 'pub fn remove_attachment(&self, attachment_id: &str)', ('pub(super) fn prepare_attachment_add',))
ordered(remove_attachment, ['SELECT note_id FROM attachments WHERE id = ?1', 'begin_plaintext_derivative_cleanup(&note_id)?', 'prepare_attachment_remove(attachment_id)?', 'finish_attachment_remove'], 'attachment remove producer drain')
require(attachments, ['pub(super) fn reconcile_attachment_storage(', 'begin_library_plaintext_derivative_cleanup()?'], 'attachment recovery producer drain')
restore_revision = function_segment(history, 'pub fn restore_note_revision(', ('pub fn revision_retention',))
ordered(restore_revision, ['SELECT note_id FROM note_revisions WHERE id = ?1', 'begin_plaintext_derivative_cleanup(&note_id)?', 'DELETE FROM attachments WHERE note_id = ?1', 'quarantine_blob_if_unreferenced'], 'revision restore producer drain')

# O10: persistent owners exist before library producers; external output recovery
# happens before normal native staging cleanup/application startup.
ordered(init, ['persistent_rendition_owner(', 'persistent_cache_owner(', 'let mut connection = self.connection()?'], 'cache ownership startup order')
ordered(main, ['recover_stale_output_transactions()', 'cleanup_stale_pdf_exports()'], 'output recovery startup order')
require(protection, ['persistent_rendition_owner(&rendition_root)?', '.remove_tree_strict()?', 'discard_all_cache_strict('], 'strict protection derivative cleanup')

# Private hidden siblings use create-new private primitives without chmodding an
# external user-selected parent.
require(private_temp, [
    'pub(crate) fn create_private_file_in_existing_directory',
    'options.write(true).create_new(true)',
    'pub(crate) fn create_private_directory_in_existing_directory',
], 'external sibling private creation')

# Regression tests for transaction crash boundaries are part of the native gate.
for marker in (
    'file_transaction_restores_previous_after_abandonment_before_publish',
    'file_transaction_commits_and_retires_hidden_predecessor',
    'recovery_rolls_back_preserved_predecessor_and_partial_temporary',
    'directory_transaction_owns_tree_before_children_exist',
    'concurrent_transactions_for_same_destination_fail_closed',
    'same_destination_aliases_share_one_reservation',
    'retry_reconciles_released_stale_destination_reservation',
    'new_file_destination_race_preserves_external_file',
    'directory_destination_race_preserves_external_tree',
    'unavailable_output_parent_preserves_recovery_record',
    'output_record_checksum_detects_valid_json_path_corruption',
):
    if marker not in output:
        raise SystemExit(f'2b.8 output adversarial test missing: {marker}')

for marker in (
    'durable_namespace_rejects_symlink_parent_escape',
    'durable_namespace_strict_cleanup_accepts_absent_nested_subtree',
    'durable_namespace_strict_cleanup_rejects_symlink_parent_escape',
    'durable_namespace_refuses_materialization_after_marker_loss',
):
    if marker not in owned:
        raise SystemExit(f'2b.8 durable namespace adversarial test missing: {marker}')

print('Vault 2b.8 plaintext-authority closure guard passed.')
PY

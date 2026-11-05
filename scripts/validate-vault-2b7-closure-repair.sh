#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
case "$BUILD_ID" in 2b.7|2b.8|2b.9|2b.10|2b.11|2b.12|2b.13|2b.14|2b.15|2b.16|2b.17|2b.18|2b.19|2b.20|2b.21|2b.22|2b.23|2b.24|2b.25|2b.26|2b.27|2b.28|2b.29|2b.30|2b.31|2b.32|2b.33|2b.34|2b.35|2b.36|2b.37|2b.38|2b.39|2b.40|2b.41|2b.43) ;; *) echo "Expected build 2b.7+" >&2; exit 1 ;; esac

python3 - <<'PY'
from pathlib import Path

def read(path): return Path(path).read_text(errors='ignore')
def require(src, markers, label):
    for marker in markers:
        if marker not in src:
            raise SystemExit(f'{label} missing: {marker}')
def segment(src, start, end=None):
    a=src.find(start)
    if a < 0: raise SystemExit(f'missing segment: {start}')
    if end is None: return src[a:]
    b=src.find(end, a+len(start))
    return src[a:b if b >= 0 else len(src)]

coord=read('src/security/privacy_coordinator.rs')
protection=read('src/storage/protection.rs')
notes=read('src/storage/notes.rs')
links=read('src/storage/note_links.rs')
staging=read('src/storage/plaintext_staging.rs')
attachments=read('src/storage/attachments.rs')
history=read('src/storage/history.rs')
initialization=read('src/storage/initialization.rs')
portability=read('src/storage/portability.rs')

# I3/I4: transition-owned functions must never re-enter admission-aware public readers.
protect=segment(protection, 'pub(crate) fn protect_note_with_password', 'fn scrub_managed_plaintext_residue_after_protection')
lock=segment(protection, 'pub(crate) fn lock_protected_note', 'pub(crate) fn remove_note_protection_with_password')
delete_cleanup=segment(notes, 'pub(super) fn retire_note_plaintext_before_delete_unlocked', 'pub(super) fn hard_delete_note_unlocked')
if 'self.get_note(note_id)' in protect:
    raise SystemExit('Protect still re-enters public protected hydration')
if 'self.get_note(note_id)' in lock:
    raise SystemExit('Lock still re-enters public protected hydration')
if 'self.list_attachments(note_id)' in delete_cleanup:
    raise SystemExit('Delete cleanup still re-enters public protected attachment access')
require(protection, ['protect_and_lock_complete_without_reentering_privacy_admission'], 'Protect/Lock behavioral regression')
require(notes, ['protected_permanent_delete_does_not_reenter_note_access'], 'protected Delete behavioral regression')
require(notes, ['permanent_delete_transfers_revision_only_blob_ownership_before_cascade_commit', 'expired_trash_purge_fails_closed_on_protected_ordinary_revision_residue'], 'hard-delete/purge historical ownership regressions')

restore=segment(coord, 'pub(crate) fn begin_protected_note_restore', 'pub(crate) fn begin_retained_protected_note_mutation')
if 'wait_for_note_admission_clear(note_id)' in restore:
    raise SystemExit('protected Restore queues behind an existing transition')
require(restore, [
    'state.library_transition.is_some() || state.note_transitions.contains_key(note_id)',
    'return Err("privacy transition is already in progress");',
    'self.inner.vault_runtime.acquire_note_access(note_id)?',
    '.insert(note_id.clone(), NotePrivacyTransition::Restore);',
], 'fail-closed protected Restore admission')
require(coord, ['protected_restore_rejects_existing_same_note_transition_without_waiting'], 'Restore conflict regression')

# 2b.4 cutover: encrypted links are authoritative for protected sources; no plaintext shadow is recreated.
require(links, [
    'protected_outgoing_note_links_if_unlocked(source_note_id)?',
    'protected_outgoing_note_links_if_unlocked(&source_id)?',
    'NOT EXISTS(',
    'vault_note_roots root',
], 'protected outgoing-link/backlink read cutover')
require(protection, [
    'unlocked protected source must resolve its encrypted outgoing links',
    'locked protected source must not reveal encrypted outgoing links',
    'unlock must restore encrypted outgoing-link behavior without a plaintext shadow',
    'ordinary_source_resolves_protected_target_title_only_while_target_is_unlocked',
], 'protected link lock/unlock behavioral regression')
require(links, [
    'row.get::<_, i64>(5)? != 0,',
    'match self.get_note(&target_id)?',
    '.protection\n                            .is_unlocked()',
], 'ordinary-source protected-target dynamic title resolution')

# I1/I2: canonical -> quarantine is predecessor/successor transfer, not a raw filesystem rename.
move=segment(attachments, 'pub(super) fn move_blob_to_quarantine', 'pub(super) fn prune_unreferenced_attachment_records_unlocked')
if 'fs::rename' in move:
    raise SystemExit('quarantine move bypasses OwnedPlaintext')
require(move, [
    'require_canonical_blob_owned_plaintext',
    'reserve_orphan_blob_owned_plaintext',
    'rename_owned_canonical_blob_to_orphan',
    'complete_owned_library_blob_rename',
], 'owned quarantine transfer')
for start,end in [
    ('pub(super) fn discard_orphaned_quarantine_for_hashes_strict', 'pub(super) fn purge_old_quarantine_files'),
    ('pub(super) fn purge_old_quarantine_files', 'fn attachment_blob_verification_stamp'),
]:
    part=segment(attachments,start,end)
    if 'fs::remove_file(' in part:
        raise SystemExit(f'quarantine deletion bypasses OwnedPlaintext: {start}')
if 'ensure_canonical_blob_owned_plaintext' in staging:
    raise SystemExit('steady-state canonical quarantine still permits late ownership adoption')
require(staging, [
    'require_canonical_blob_owned_plaintext',
    'canonical attachment plaintext has no durable cleanup owner',
    'reserve_orphan_blob_owned_plaintext',
    'require_orphan_blob_owned_plaintext',
    'verify_managed_attachment_plaintext_ownership_unlocked',
    'rename_owned_canonical_blob_to_orphan',
    'complete_owned_library_blob_rename',
    'validate_library_blob_transfer',
    'validate_orphan_blob_relative_path',
    'orphan_transfer_recovers_after_rename_before_predecessor_retirement',
    'startup_verification_rejects_preexisting_unowned_orphan_plaintext',
], 'durable quarantine ownership state machine')

# Database ownership may disappear only in a transaction that establishes the generic successor owner.
require(attachments, [
    'ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(',
    'quarantine_transfer_keeps_durable_owner_until_plaintext_deletion',
    'pending_add_remains_the_only_owner_when_last_attachment_reference_is_removed',
    'corrupt_pending_add_fails_closed_before_canonical_ownership_can_move',
    'abandoned_pending_add_transfers_ownership_before_operation_retirement',
    'steady_state_quarantine_rejects_unowned_canonical_plaintext',
    'startup_verification_rejects_unowned_canonical_plaintext',
    'revision_pruning_transfers_revision_only_blob_ownership_before_commit',
], 'attachment DB->ledger ownership handoff')
collectable=segment(attachments, 'fn canonical_blob_is_collectable', 'pub(super) fn quarantine_blob_if_unreferenced')
require(collectable, [
    'pending attachment Add ownership record is corrupt',
    'serde_json::from_str',
    'return Ok(false);',
], 'canonical blob collectability fail-closed guard')
if '.is_ok_and(' in collectable or 'if let Ok(attachment) = serde_json::from_str' in collectable:
    raise SystemExit('canonical blob collectability silently ignores corrupt Add ownership')
quarantine_one=segment(attachments, 'pub(super) fn quarantine_blob_if_unreferenced', 'pub(super) fn discard_canonical_blob_if_unreferenced_strict')
require(quarantine_one, ['canonical_blob_is_collectable'], 'single-blob quarantine canonical authority')
quarantine_all=segment(attachments, 'pub(super) fn quarantine_orphan_blobs_unlocked', 'pub(super) fn discard_orphaned_quarantine_for_hashes_strict')
require(quarantine_all, [
    'pending attachment Add ownership record is corrupt',
    'serde_json::from_str',
], 'orphan scan corrupt-owner fail-closed guard')
if '.is_ok_and(' in quarantine_all or 'if let Ok(attachment) = serde_json::from_str' in quarantine_all:
    raise SystemExit('orphan scan silently ignores corrupt Add ownership')
require(attachments, [
    'discard_canonical_blob_if_unreferenced_strict',
    'collectable canonical blob exists without its durable cleanup owner',
], 'direct canonical cleanup authority')
ensure_handoff=segment(staging, 'pub(super) fn ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction', 'fn validate_managed_plaintext_transfer_record')
require(ensure_handoff, [
    "SELECT attachment_json FROM attachment_operations WHERE operation = 'add'",
    'pending attachment Add ownership record is corrupt',
    'if attachment.sha256 == sha256',
    'return Ok(());',
], 'pending Add ownership exclusion')
recovery_add=segment(attachments, 'pub(super) fn reconcile_attachment_storage_unlocked', 'pub fn attachment_file_path')
require(recovery_add, [
    'DELETE FROM attachment_operations WHERE id = ?1',
    'ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(',
    'transaction.commit()?;',
    'self.quarantine_blob_if_unreferenced(&attachment.sha256)?;',
], 'abandoned pending Add owner retirement handoff')
require(history, [
    'revision_blob_hashes_for_note',
    'revision_blob_hashes_for_revision_ids',
    'all_revision_blob_hashes',
    'ensure_unreferenced_revision_blob_ownership',
    'ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(',
    'let current_blob_hashes = current_attachments',
], 'revision prune/restore DB->ledger ownership handoff')
hard_delete=segment(notes, 'pub(super) fn hard_delete_note_unlocked', '#[cfg(feature = "remote-sync")]')
require(hard_delete, [
    'historical_blob_hashes',
    'revision_attachment_refs refs',
    'ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(',
    'transaction.commit()?;',
    'self.quarantine_blob_if_unreferenced(&sha256)?;',
    'self.discard_canonical_blob_if_unreferenced_strict(&sha256)?;',
], 'hard-delete DB->ledger ownership handoff')
purge=segment(notes, 'pub fn purge_expired_notes', 'pub fn permanently_delete_trashed_note')
require(purge, [
    'protected_revision_residue_sql',
    'protected Trash note retained ordinary revision attachment references',
    'let ordinary_blob_hashes = {',
    'revision_attachment_refs refs',
    'JOIN note_revisions revision',
    'AND NOT EXISTS(',
    'vault_note_roots root',
    'ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction(',
    'transaction.commit()?;',
    'self.discard_canonical_blob_if_unreferenced_strict(sha256)?;',
    'self.quarantine_orphan_blobs_unlocked()?;',
], 'expired Trash batch-purge ownership handoff')
require(notes, [
    'expired_trash_purge_transfers_ordinary_blob_ownership_before_cascade_commit',
    'expired_trash_purge_fails_closed_on_protected_ordinary_revision_residue',
    'expired_trash_purge_directly_retires_protected_ciphertext',
    'protected_permanent_delete_does_not_reenter_note_access',
    'protected_ciphertext_cleanup_owner_recovers_after_database_commit_crash_boundary',
], 'expired Trash purge ownership regressions')

# Startup must preserve live quarantine owners and retire only an absent path after a conclusive NotFound proof.
recovery=segment(staging, 'pub(super) fn recover_managed_plaintext_staging_unlocked', 'fn managed_plaintext_stage_is_required_transfer_successor')
require(recovery, [
    'validate_orphan_blob_relative_path(&stage.relative_path).is_ok()',
    'fs::symlink_metadata(&path)',
    'error.kind() == io::ErrorKind::NotFound',
    'release_uncreated_managed_plaintext_stage(&stage.id)?;',
], 'quarantine startup recovery')
if 'reconcile_orphan_quarantine_ownership_unlocked' in staging or 'reconcile_unowned_canonical_orphan_ownership_unlocked' in staging:
    raise SystemExit('startup still contains late plaintext ownership adoption machinery')
verify_startup=segment(staging, 'pub(super) fn verify_managed_attachment_plaintext_ownership_unlocked', '/// a separate durable staging row.')
require(verify_startup, [
    'quarantined plaintext exists without prior durable ownership',
    'canonical attachment plaintext exists without prior durable ownership',
    'pending attachment Add ownership record is corrupt',
    'canonical attachment blob has conflicting durable owners',
], 'strict startup plaintext ownership verification')
require(staging, [
    'quarantined plaintext has no durable cleanup owner',
    'startup_verification_rejects_preexisting_unowned_orphan_plaintext',
], 'steady-state/startup ownership boundary')
require(initialization, [
    'self.recover_vault_operations_unlocked()?;',
    'self.verify_managed_attachment_plaintext_ownership_unlocked()?;',
], 'startup ownership verification ordering')
if initialization.find('self.verify_managed_attachment_plaintext_ownership_unlocked()?;') < initialization.find('self.recover_vault_operations_unlocked()?;'):
    raise SystemExit('plaintext ownership verification runs before Vault-operation recovery')
require(portability, [
    'revision_attachment_refs refs',
    'hashes.extend(revision_blob_rows);',
], 'portability rollback revision-only blob ownership')

print('Vault 2b.7 focused closure-repair guard passed.')
PY

# Earlier guards remain regression evidence, but 2b.7 adds the behavioral holes they missed.
bash scripts/validate-vault-2b1-foundation.sh
bash scripts/validate-vault-2b2-privacy-authority.sh
bash scripts/validate-vault-2b3-publication-token.sh
bash scripts/validate-vault-2b4-granular-cutover.sh
bash scripts/validate-vault-2b5-consolidation.sh
bash scripts/validate-vault-2b6-independent-closure.sh

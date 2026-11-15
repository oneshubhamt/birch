#!/bin/zsh
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2a.17" || "$BUILD_ID" == "2a.18" || "$BUILD_ID" == "2a.19" || "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path

def text(path): return Path(path).read_text(errors='ignore')
build_id=text('build-id.txt').strip()
def require(haystack, markers, label):
    for marker in markers:
        if marker not in haystack:
            raise SystemExit(f"{label} missing: {marker}")
def ordered(haystack, markers, label):
    cursor = 0
    for marker in markers:
        pos = haystack.find(marker, cursor)
        if pos < 0:
            raise SystemExit(f"{label} missing/out of order: {marker}")
        cursor = pos + len(marker)

locations=text('src/storage/locations.rs')
backup=text('src/storage/backup_repository.rs')
attachments=text('src/storage/attachments.rs')
protection=text('src/storage/protection.rs')
portable=text('src/storage/portable_bundle.rs')
plaintext=text('src/storage/plaintext_staging.rs')
settings=text('src/ui/settings.rs')

require(locations, [
    'const BACKUP_LOCATION_HISTORY_VERSION: u32 = 3;',
    'pending_privacy_reset_directories: Vec<PathBuf>',
    'begin_backup_privacy_reset_for',
    'clear_backup_privacy_reset_required_for',
    'backup_privacy_reset_required_for',
    'pending_backup_privacy_reset_count_for',
    'require_pending_backup_location_available',
    'read_library_identity_if_present',
], 'backup privacy history')

begin=locations[locations.index('pub(super) fn begin_backup_privacy_reset_for'):]
ordered(begin, [
    'entry.pending_privacy_reset_directories.push(backup_directory.clone())',
    'write_backup_location_history_at(&path, libraries)?;',
    'Ok(backup_directories)',
], 'privacy obligation publication')

save=locations[locations.index('pub(super) fn save_storage_locations'):locations.index('pub(super) fn reset_storage_locations')]
ordered(save, [
    'require_pending_backup_location_available',
    'if !pending_backup_reset',
    'fs::create_dir_all(&backup_directory)?;',
], 'settings must not manufacture pending mount path')

require(backup, [
    'discharge_current_backup_privacy_obligation_unlocked',
    'current_backup_privacy_reset_pending_unlocked',
    'require_current_backup_privacy_ready_unlocked',
    'offline_historical_repository_keeps_privacy_obligation_until_reconnected',
    'current_repository_disappearance_stays_pending_and_cannot_be_recreated_in_place',
    'cannot prove that the remembered repository is mounted there',
], 'backup pending runtime')

reset=backup[backup.index('pub(super) fn reset_backup_repository_for_vault_unlocked'):backup.index('pub fn backup_repository_budget_bytes')]
ordered(reset, [
    'begin_backup_privacy_reset_for',
    'reset_backup_repositories_for_vault_unlocked',
], 'backup reset must publish obligation first')
required_reset = [
    'DirectoryOwnership::acquire',
    'purge_owned_backup_repository_for_vault_unlocked_at',
    'clear_backup_privacy_reset_required_for',
]
if build_id not in ('2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    required_reset.append('write_repository_owner')
require(reset, required_reset, 'remembered repository invalidation')
if build_id in ('2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    purge=backup[backup.index('fn purge_owned_backup_repository_for_vault_unlocked_at'):backup.index('/// E3 privacy boundary for selective encryption.')]
    require(purge, [
        'establish_backup_repository_namespace_unlocked_at',
        'namespace.remove_tree_strict()',
        'automatic backup repository still exists after Vault cleanup',
    ], '2b.9 crash-durable backup purge authority')

# A missing/false mount must not be recreated during Vault reset.
current=reset[reset.index('if backup_directory != &self.automatic_backup_directory'):]
if 'Err(error) if error.kind() == std::io::ErrorKind::NotFound => {\n                    fs::create_dir_all' in current:
    raise SystemExit('current backup reset still recreates a vanished mount path')

# Normal live repository surfaces must cross the pending-state guard before touching layout.
for signature in [
    'pub(super) fn reconcile_backup_repository_unlocked',
    'fn record_repository_result',
    'fn create_repository_snapshot_unlocked',
    'fn automatic_backup_due_unlocked',
    'fn prune_backup_repository_unlocked',
    'pub fn inspect_backup_repository_snapshot',
    'fn stage_backup_repository_snapshot_unlocked',
]:
    part=backup[backup.index(signature):]
    first_layout=part.find('backup_repository_layout()')
    guard=part.find('require_current_backup_privacy_ready_unlocked()?;')
    if guard < 0 or (first_layout >= 0 and guard > first_layout):
        raise SystemExit(f'pending backup guard missing/late in {signature}')

status=backup[backup.index('fn backup_repository_status_unlocked'):backup.index('pub fn inspect_backup_repository_snapshot')]
status_layout = ('let (_namespace, layout) = self.verified_backup_repository_layout_unlocked()?;'
                 if build_id in ('2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') else 'let layout = self.backup_repository_layout();')
ordered(status, [
    'pending_backup_privacy_reset_count_for',
    'if self.current_backup_privacy_reset_pending_unlocked()? {',
    'pending_privacy_reset_count,',
    status_layout,
], 'safe pending backup status')

require(attachments, [
    'discard_orphaned_quarantine_for_hashes_strict',
    'rsplit_once',
    'sha256_values.contains(candidate_hash)',
    'matching attachment quarantine artifact is not a file',
], 'hash-targeted orphan quarantine')
quarantine=attachments[attachments.index('pub(super) fn discard_orphaned_quarantine_for_hashes_strict'):attachments.index('pub(super) fn purge_old_quarantine_files')]
if build_id in ('2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    ordered(quarantine, [
        'if !sha256_values.contains(candidate_hash)',
        'require_orphan_blob_owned_plaintext(&relative)?',
        'discard_owned_plaintext(&mut owned)?;',
        'sync_parent_directory',
    ], 'strict owned orphan cleanup')
    if 'remove_file(&entry.path())?;' in quarantine:
        raise SystemExit('2b.7 orphan cleanup regressed to unmanaged deletion')
else:
    ordered(quarantine, [
        'if !sha256_values.contains(candidate_hash)',
        'remove_file(&entry.path())?;',
        'sync_parent_directory',
    ], 'strict orphan cleanup')

residue_markers = [
    'discard_orphaned_quarantine_for_hashes_strict(&plaintext_hashes)',
    'unrelated_orphan',
]
if build_id not in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    residue_markers.extend([
        "VALUES ('vault.e3_privacy_scrub', '6')",
        "value = '6'",
        'e3_v6_upgrade_clears_legacy_attachment_quarantine_for_protected_library',
    ])
else:
    if 'vault.e3_privacy_scrub' in protection or 'scrub_existing_protected_residue_for_e3_unlocked' in protection:
        raise SystemExit('2b.5 retained obsolete E3 upgrade marker/migration scrub')
require(protection, residue_markers, 'persistent residue scrub')
ordered(protection[protection.index('fn scrub_managed_plaintext_residue_after_hashes'):], [
    'discard_orphaned_quarantine_for_hashes_strict(&plaintext_hashes)',
    'reset_backup_repository_for_vault_unlocked()',
    'scrub_sqlite_plaintext_residue_unlocked()',
], 'post-protection persistent scrub')
production_protect = protection[
    protection.index('pub(crate) fn protect_note_with_password_and_public_label('):
    protection.index('/// Close Rustnote-managed plaintext residues after the canonical protection')
]
require(production_protect, [
    'scrub_managed_plaintext_residue_after_protection(',
], 'production post-protection residue authority')
for forbidden in ('SqliteResidueScrubMode', 'connection.execute_batch("VACUUM;")'):
    if forbidden in protection:
        raise SystemExit(f'2b.42.8 retained obsolete post-protection residue mechanism: {forbidden}')

# 2a.16 remains the sole portability plaintext owner authority.
require(plaintext, [
    'reserve_portability_owned_plaintext',
    'reserve_portability_blob_owned_plaintext',
], 'inherited portability owner')
require(portable, [
    'reserve_portability_blob_owned_plaintext',
    'transfer_owned_plaintext_stage', 'complete_owned_plaintext_transfer',
], 'inherited portable bundle owner transfer')

require(settings, [
    'pending_backup_privacy_resets',
    'Encryption is complete locally.',
    'still require privacy invalidation',
], 'backup privacy UI status')
PY

echo "E0-E3 2a.17 persistent residue and backup closure guard passed."

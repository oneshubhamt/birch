#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

backup = Path('src/storage/backup_repository.rs').read_text(errors='ignore')
plaintext = Path('src/security/plaintext.rs').read_text(errors='ignore')
locations = Path('src/storage/locations.rs').read_text(errors='ignore')
storage_mod = Path('src/storage/mod.rs').read_text(errors='ignore')


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f'{label} missing: {marker}')


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f'{label} retained obsolete marker: {marker}')

require(backup, [
    'const BACKUP_NAMESPACE_MARKER_FILE_NAME: &str = ".rustnote-backup-repository-owner";',
    'fn backup_namespace_marker(backup_directory: &Path) -> PathBuf',
    'backup_directory.join(BACKUP_NAMESPACE_MARKER_FILE_NAME)',
    'fn establish_backup_repository_namespace_unlocked_at(',
    'DurablePlaintextNamespace::open_existing(',
    'DurablePlaintextNamespace::establish(',
    'fn verified_backup_repository_layout_unlocked(',
    'namespace.remove_tree_strict()',
    'automatic backup repository still exists after Vault cleanup',
], 'external backup namespace authority')

# 2b.42.9 deliberately removes the one-way 2b.8 adoption convention. There is
# now one ownership convention only: the external durable namespace marker.
forbid(backup, [
    'INLINE_OWNER',
    'InlineRepositoryOwner',
    'inline_owner',
    'read_inline_repository_owner',
    'adopt_existing_with_proof',
    'superseded inline backup owner',
], 'single backup ownership convention')
forbid(plaintext, [
    'adopt_existing_with_proof',
    'allow_nonempty_adoption',
], 'durable namespace API')

require(plaintext, [
    'pub(crate) fn establish(',
    'if root_nonempty {',
    'durable plaintext namespace marker is missing for a nonempty managed root',
    'durable_namespace_establish_refuses_posthoc_adoption_of_nonempty_root',
    'durable_namespace_open_existing_never_recreates_missing_marker',
    'durable_namespace_strict_cleanup_refuses_marker_loss',
], 'fail-closed namespace establishment')

require(backup, [
    'storage_refuses_nonempty_repository_without_durable_namespace_authority',
    'repository_namespace_owner_is_external_and_recovers_missing_index',
    'vault_reset_fails_closed_for_corrupt_external_repository_owner',
    'repository_namespace_owner_prevents_cross_library_reuse_even_without_index',
    'interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear',
    'restart_recovers_after_repository_removed_before_privacy_obligation_retirement',
    'steady_state_backup_writes_fail_closed_after_namespace_marker_loss',
    'offline_historical_repository_keeps_privacy_obligation_until_reconnected',
    'current_repository_disappearance_stays_pending_and_cannot_be_recreated_in_place',
], 'backup ownership/recovery regressions')

# Current backup history is identity-bearing and current-format-only.
require(locations, [
    'const BACKUP_LOCATION_HISTORY_VERSION: u32 = 3;',
    '#[serde(deny_unknown_fields)]',
    'fn read_current_backup_history_at(',
    'if history.version != BACKUP_LOCATION_HISTORY_VERSION',
    'validate_library_id(&entry.library_id)?;',
    'backup_history_rejects_precurrent_versions',
    'backup_history_requires_current_library_identity_shape',
], 'current backup history authority')
forbid(locations, [
    '1 | 2 | BACKUP_LOCATION_HISTORY_VERSION',
    'entry.library_id.is_empty()',
    'Migrate a v1 path-scoped entry',
], 'backup history migration compatibility')

require(storage_mod, [
    'validate_storage_location_ownership(storage.library_id(), &storage_locations)?;',
    'discharge_current_backup_privacy_obligation_unlocked()?'
], 'storage open ordering')

print('Vault 2b.9 backup-ownership regression guard passed (single current authority).')
PY

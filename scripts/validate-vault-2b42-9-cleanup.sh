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
            raise SystemExit(f'{label} missing: {marker}')


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f'{label} contains forbidden marker: {marker}')

src_text = '\n'.join(path.read_text(errors='ignore') for path in Path('src').rglob('*.rs'))
editor_spec = read('src/editor_document_spec.rs')
plaintext = read('src/security/plaintext.rs')
backup = read('src/storage/backup_repository.rs')
locations = read('src/storage/locations.rs')
sync = read('src/ui/synchronization.rs')
privacy = read('src/ui/privacy.rs')
coord = read('src/security/privacy_coordinator.rs')
runner = read('scripts/run-browser-behavior-tests.mjs')

# 1. No dead-code lint suppression remains in production Rust.
forbid(src_text, [
    '#![allow(dead_code)]',
    '#[allow(dead_code)]',
    '#[expect(dead_code)]',
], 'production dead-code policy')

# 2. Parse-only schema fields remain strict without lint suppression.
require(editor_spec, [
    '#[serde(rename = "prosemirror")]',
    'pub(crate) _prosemirror: Option<Value>',
    '#[serde(rename = "default")]',
    'pub(crate) _default_value: Option<Value>',
], 'parse-only editor schema fields')

# 3. Durable namespace ownership is fail-closed with no exceptional adoption API.
require(plaintext, [
    'pub(crate) fn establish(',
    'if root_nonempty {',
    'durable plaintext namespace marker is missing for a nonempty managed root',
    'durable_namespace_establish_refuses_posthoc_adoption_of_nonempty_root',
], 'durable namespace current authority')
forbid(plaintext, ['adopt_existing_with_proof', 'allow_nonempty_adoption'], 'durable namespace migration API')

# 4. Backup repository has one ownership convention: external durable marker.
require(backup, [
    'const BACKUP_NAMESPACE_MARKER_FILE_NAME: &str = ".rustnote-backup-repository-owner";',
    'DurablePlaintextNamespace::open_existing(',
    'DurablePlaintextNamespace::establish(',
    'storage_refuses_nonempty_repository_without_durable_namespace_authority',
    'repository_namespace_owner_is_external_and_recovers_missing_index',
], 'backup repository ownership')
forbid(backup, [
    'INLINE_OWNER',
    'InlineRepositoryOwner',
    'inline_owner',
    'read_inline_repository_owner',
    'adopt_existing_with_proof',
], 'backup repository compatibility ownership')

# 5. Backup location history is current-format-only and identity-bearing.
require(locations, [
    'const BACKUP_LOCATION_HISTORY_VERSION: u32 = 3;',
    'fn read_current_backup_history_at(',
    'if history.version != BACKUP_LOCATION_HISTORY_VERSION',
    'validate_library_id(&entry.library_id)?;',
    'backup_history_rejects_precurrent_versions',
    'backup_history_requires_current_library_identity_shape',
], 'current backup history')
forbid(locations, [
    '1 | 2 | BACKUP_LOCATION_HISTORY_VERSION',
    'entry.library_id.is_empty()',
    'Migrate a v1 path-scoped entry',
    '#[serde(default)]\n    library_id',
], 'backup history compatibility')

# 6. Current backup metadata is strict instead of silently defaulting old shapes.
require(backup, [
    '#[serde(deny_unknown_fields)]\nstruct RepositorySnapshotManifest',
    '#[serde(deny_unknown_fields)]\nstruct BackupRepositoryIndex',
], 'strict backup metadata parsing')
forbid(backup, [
    '#[serde(default)]\n    build_identifier',
    '#[serde(default)]\n    library_id',
], 'backup metadata compatibility defaults')

# 7. Legitimate current crash recovery remains intact.
require(locations, [
    'deleted_library_recovers_historical_identity_for_the_same_data_path',
    'failed_bootstrap_identity_is_reconciled_only_while_library_is_pristine',
    'non_pristine_library_identity_is_never_rewritten_from_history',
], 'current library identity recovery')
require(backup, [
    'interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear',
    'restart_recovers_after_repository_removed_before_privacy_obligation_retirement',
    'steady_state_backup_writes_fail_closed_after_namespace_marker_loss',
], 'current backup crash recovery')

# 8. One-use editor publication helper is folded into synchronization context.
require(sync, [
    'impl EditorSynchronizationContext',
    'fn capture_publication(&self) -> Option<PrivacyPublicationToken>',
    'let Some(token) = context.capture_publication() else',
], 'editor synchronization publication capture')
forbid(sync, ['capture_current_editor_publication'], 'obsolete editor publication helper')

# 9. Scoped publication remains the only production vocabulary.
require(coord, [
    'library_publication_generation: u64',
    'multi_note_publication_generation: u64',
    'note_publication_generations: HashMap<NoteId, u64>',
], 'scoped coordinator publication state')
require(privacy, [
    'library_revocations: u32',
    'multi_note_revocations: u32',
    'note_revocations: HashMap<NoteId, u32>',
], 'scoped UI revocation state')
forbid(src_text, [
    'active_revocations: u32',
    'capture_at_ui_generation',
    'capture_current_editor_publication',
], 'obsolete global publication vocabulary')

# 10. Historical validator entry points now assert current scoped architecture.
for script in [
    'scripts/validate-vault-2b2-privacy-authority.sh',
    'scripts/validate-vault-2b3-publication-token.sh',
    'scripts/validate-security-e0-e3-ui-lifetime-2a18.sh',
    'scripts/validate-security-e0-e3-adversarial-closure-2a19.sh',
]:
    text = read(script)
    forbid(text, [
        'active_publications: usize',
        'active_revocations: u32',
        'capture_at_ui_generation',
    ], f'current inherited validator {script}')

# 11. Stale implementation-version commentary is absent from production source.
forbid(src_text, ['2b.'], 'production historical pass commentary')

# 12. 2b.42.8 no-VACUUM security decision remains intact.
forbid(read('src/storage/protection.rs'), ['execute_batch("VACUUM;")', 'SqliteResidueScrubMode'], '2b.42.8 production cleanup')
require(read('src/storage/protection.rs'), [
    'PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;',
    "INSERT INTO notes_fts(notes_fts) VALUES('rebuild');",
    'ensure_wal_truncated(&connection)',
], '2b.42.8 residue sequence')

# 13. Dependency-free behavior runner includes the cleanup contract.
require(runner, ['vault-cleanup-2b42-9-contract.test.mjs'], 'behavior runner')

# 14. Historical implementation evidence is provenance; QA-report removal is Q4.
print('Vault 2b.42.9 current dead-code + architecture cleanup validation passed.')
PY

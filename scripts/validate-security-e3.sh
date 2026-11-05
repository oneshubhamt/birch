#!/bin/zsh
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2a.18" || "$BUILD_ID" == "2a.19" || "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
[[ -f SECURITY_ENCRYPTION_E3_CONTRACT.md ]] || { echo "Missing E3 security contract." >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path

def text(path): return Path(path).read_text(errors='ignore')
build_id=text('build-id.txt').strip()
def require(haystack, markers, label):
    for marker in markers:
        if marker not in haystack:
            raise SystemExit(f"{label} missing: {marker}")

schema=text('src/storage/schema.rs'); protection=text('src/storage/protection.rs')
derivative=text('src/security/derivative.rs'); transport=text('src/storage/transport.rs')
protocol=text('src/attachment_protocol.rs'); media=text('src/media_compat.rs')
staging=text('src/native/staging.rs'); temp_owner=text('src/native/temp_owner.rs')
private_temp=text('src/native/private_temp.rs'); handoff=text('src/native/handoff.rs')
backup=text('src/storage/backup_repository.rs'); locations=text('src/storage/locations.rs')
storage=text('src/storage/mod.rs'); security=text('src/security/mod.rs')
vault=text('crates/rustnote-vault/src/lib.rs'); note_card=text('src/ui/note_card.rs')
attachment_app=text('src/application/attachments.rs')
media_server=text('src/media_server.rs'); import_js=text('editor-src/attachments/import.js')
controller=text('editor-src/runtime/controller.js'); contract=text('SECURITY_ENCRYPTION_E3_CONTRACT.md')
runtime=text('src/security/runtime.rs'); autosave=text('src/autosave.rs'); journal=text('src/autosave/emergency_journal.rs')
backup_locations=text('src/storage/locations.rs'); backup_repository=text('src/storage/backup_repository.rs')
plaintext_staging=text('src/storage/plaintext_staging.rs'); attachment_ingest=text('src/storage/attachment_ingest.rs'); portable_bundle=text('src/storage/portable_bundle.rs'); notes=text('src/storage/notes.rs'); attachments=text('src/storage/attachments.rs')

if build_id in ('2b.4', '2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    markers = [
        'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;',
        'CREATE TABLE vault_objects', 'CREATE TABLE vault_note_roots',
        'CREATE TABLE vault_blob_refs', 'CREATE TABLE managed_plaintext_staging',
    ]
    if build_id == '2b.4':
        markers.append('DROP TABLE protected_notes;')
    else:
        if 'protected_notes' in schema or 'protected_note_blobs' in schema or 'const SCHEMA_' in schema:
            raise SystemExit('E3/2b.5 retained obsolete monolithic/schema migration compatibility')
    require(schema, markers, 'E3/2b.4+ schema')
else:
    require(schema, ['pub(super) const CURRENT_SCHEMA_VERSION: i64 = 20;', 'CREATE TABLE protected_notes', 'CREATE TABLE managed_plaintext_staging'], 'E3 schema')
e3_protection_markers = [
    'PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;', 'PRAGMA wal_checkpoint(TRUNCATE)',
    "INSERT INTO notes_fts(notes_fts) VALUES('rebuild')", 'ensure_wal_truncated(&connection)',
    'begin_note_privacy_transition(', 'NotePrivacyTransition::Protect', 'scrub_managed_plaintext_residue_after_protection',
    'discard_staged_derivatives_for_note', 'discard_staged_derivatives_for_attachments',
    'note_handoff_source_key(note_id)', 'reset_backup_repository_for_vault_unlocked',
]
if build_id in ('2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    # 2b.8 consolidates persistent derivative cleanup into the strict
    # quarantine-retirement authority so protection and normal attachment
    # retirement cannot drift into two cleanup implementations.
    e3_protection_markers.append('discard_orphaned_quarantine_for_hashes_strict')
    require(attachments, [
        'pub(super) fn discard_orphaned_quarantine_for_hashes_strict',
        'discard_image_rendition_cache_strict(sha256)?',
        'crate::media_compat::discard_cache_strict(',
    ], 'E3/2b.8 delegated derivative cleanup')
else:
    e3_protection_markers.extend(['discard_image_rendition_cache_strict', 'discard_cache_strict'])
if build_id not in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    e3_protection_markers.extend(["VALUES ('vault.e3_privacy_scrub', '6')", "value = '6'"])
else:
    if 'vault.e3_privacy_scrub' in protection or 'scrub_existing_protected_residue_for_e3_unlocked' in protection:
        raise SystemExit('2b.5 retained obsolete one-time E3 upgrade-marker machinery')
require(protection, e3_protection_markers, 'E3 protection')
if 'connection.execute_batch("VACUUM;")' in protection:
    raise SystemExit('E3/2b.42.8 retained obsolete VACUUM after native forensic closure')
require(derivative, ['PlaintextDerivativeGate', 'PlaintextDerivativePermit', 'PlaintextDerivativeTransition',
                     'transition_waits_for_existing_permit_and_blocks_new_permits'], 'derivative barrier')
if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(transport, [
        'AttachmentPlaintextIngressGuard', 'plaintext_attachment_ingress_permit',
        'prepare_ephemeral_plaintext_directory',
        'authorization: crate::security::PlaintextOperationPermit',
        'acquire_plaintext_operation_for_note(note_id)',
    ], 'transport atomic compound barrier')
else:
    require(transport, ['plaintext_derivative_permit', 'plaintext_attachment_ingress_permit'], 'transport barrier')
if build_id in ('2b.4', '2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(protocol, [
        'Image editing is unavailable while the note is in Trash',
        'Cache-Control", "no-store"',
        'protected_pdf_annotation_guard_keeps_lock_waiting_until_response_scope_drops',
    ], '2b.4 protocol privacy')
else:
    require(protocol, ['Image editing is unavailable for encrypted notes',
                       'Annotated export staging is unavailable for encrypted notes',
                       'Cache-Control", "no-store"'], 'protocol privacy')
if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(media, [
        'resource: AttachmentTransportResource',
        'resource` owns the compound plaintext/Vault authorization',
        'persistent media compatibility caching is disabled for encrypted notes',
        'discard_cache_strict',
    ], 'media compound derivative boundary')
else:
    require(media, ['_plaintext_permit: PlaintextDerivativePermit',
                    'background worker, not a waiting caller, owns the transition permit',
                    'persistent media compatibility caching is disabled for encrypted notes',
                    'discard_cache_strict'], 'media derivative boundary')
require(staging, ['PlaintextDerivativeOwner', 'discard_staged_derivatives_for_note',
                  'ensure_derivative_stage_attachment_identity', 'claim_registered_staged_image_file'], 'staging identity')
require(temp_owner, ['session-v2-', 'CLEANUP_CLAIM_FILE', 'claim_abandoned_owner_for_cleanup',
                     'claim_unowned_v2_for_cleanup'], 'temp ownership v2')
require(private_temp, ['rustnote_temp_directory', 'ensure_directory', 'ensure_private_leaf_directory'], 'private temp boundary')
require(handoff, ['note_handoff_source_key', 'discard_handoffs_for_source_keys'], 'handoff ownership')
if build_id in ('2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(backup, [
        'BACKUP_NAMESPACE_MARKER_FILE_NAME',
        'DurablePlaintextNamespace::establish',
        'ensure_backup_repository_identity_unlocked',
        'establish_backup_repository_namespace_unlocked_at',
        'namespace.remove_tree_strict()',
    ], 'backup durable namespace ownership')
    for obsolete in ('struct BackupRepositoryOwner', 'fn write_repository_owner', 'fn read_repository_owner', 'legacy_root_repository'):
        if obsolete in backup:
            raise SystemExit(f'2b.9 retained obsolete backup ownership machinery: {obsolete}')
else:
    require(backup, ['OWNER_FILE_NAME', 'BackupRepositoryOwner', 'ensure_backup_repository_identity_unlocked',
                     'write_repository_owner', 'library_id'], 'backup ownership identity')
require(locations, ['LIBRARY_IDENTITY_FILE', 'ensure_library_identity', 'library_id',
                    'remember_storage_location'], 'library backup history identity')
require(storage, ['library_id: String', 'privacy_coordinator: PrivacyCoordinator',
                  'PrivacyCoordinator::new()', 'ensure_backup_repository_identity_unlocked()', 'remember_storage_location(storage.library_id()'], 'storage constructor invariants')
require(vault, ['Zeroizing<[u8; KEY_BYTES]>', 'secure_random(bytes.as_mut())', 'VaultKey::from_zeroizing(bytes)'], 'key zeroization')
require(security, ['pub(crate) fn replace_secret_input', 'pub(crate) fn clear_secret_input', 'pub(crate) fn take_from'], 'password hygiene')
require(note_card, ['SecretString::take_from', 'replace_secret_input', 'clear_secret_input'], 'UI password hygiene')
require(handoff, ['note_handoff_source_key'], 'pending handoff ownership')
require(attachment_app, ['open_managed_file_for_note', 'plaintext_attachment_ingress_permit'], 'pending attachment handoff')
require(media_server, ['Cache-Control: private, no-store'], 'WebView media no-store')
require(import_js, ["'X-Rustnote-Note-Id'", "cache: 'no-store'"], 'image import authorization')
require(controller, ['stageImageImportFiles(images, this.imageImportStageUrl, this.noteId)'], 'image import note identity')

# Avoid reintroducing filename-bearing diagnostics in the two paths cleaned by 2a.12.
for path in ('src/application/attachments.rs','src/media_compat.rs','src/media_server.rs'):
    s=text(path)
    for line in s.splitlines():
        if ('eprintln!' in line or 'println!' in line) and ('file_name' in line or 'display()' in line):
            raise SystemExit(f'filename/path-bearing diagnostic remains in {path}: {line.strip()}')

require(contract, ['Managed plaintext-residue guarantee', 'E3 definition of done',
                   '2a.12 security-boundary consolidation amendment',
                   '2a.13 adversarial behavioral closure amendment',
                   '2a.14 independent final closure audit amendment',
                   '2a.15 plaintext ownership remediation amendment',
                   '2a.16 canonical plaintext ownership amendment',
                   '2a.17 persistent residue and backup closure amendment'], 'E3 contract')
protection_behavior_markers = [
    'vault_recovery_restores_plaintext_retired_before_protection_commit',
    'vault_recovery_finishes_protection_after_database_commit',
    'vault_recovery_rolls_back_prepared_unprotect_without_publishing_plaintext',
    'vault_recovery_finishes_plaintext_after_unprotect_commit',
    'protect_waits_for_active_derivative_then_scrubs_cache_and_stage',
    'protection_cleanup_failure_stays_journaled_until_recovery_succeeds',
    'pending_attachment_mutations_block_encrypt_until_their_durable_state_is_resolved',
    'revision_only_plaintext_is_removed_from_sqlite_and_wal_on_protection',
]
if build_id in ('2b.4', '2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    protection_behavior_markers += [
        'protected_unprotected_protected_cycle_preserves_granular_state_and_residue_rules',
        'lock_waits_for_scoped_protected_read_and_suppresses_late_publication',
    ]
else:
    protection_behavior_markers += [
        'protected_unprotected_protected_cycle_preserves_payload_and_residue_rules',
        'lock_waits_for_scoped_payload_read_and_suppresses_late_publication',
    ]
require(protection, protection_behavior_markers, 'inherited 2a.13 protection behavior matrix')
require(runtime, [
    'lock_waits_for_active_read_permit_and_rejects_new_reads_after_revocation',
    'failed_lock_cleanup_retains_session_and_plaintext_for_retry',
    'tracked_materialization_cannot_promote_after_revocation_begins',
    'concurrent_materialization_reuses_canonical_path_without_orphaning_duplicate',
    'Snapshot without unregistering',
], 'Lock ownership and retry closure')
require(autosave, ['vault_barrier_prevents_stale_plaintext_emergency_journal_recreation'], 'inherited 2a.13 stale autosave')
require(backup_locations, ['backup_privacy_history_never_forgets_previous_locations',
                           'backup_privacy_history_rejects_cross_library_repository_reuse'], 'inherited 2a.13 backup history')
if build_id in ('2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(backup_repository, [
        'repository_namespace_owner_prevents_cross_library_reuse_even_without_index',
        'repository_namespace_owner_is_external_and_recovers_missing_index',
        'interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear',
        'restart_recovers_after_repository_removed_before_privacy_obligation_retirement',
        'steady_state_backup_writes_fail_closed_after_namespace_marker_loss',
    ], '2b.9 backup namespace ownership and crash recovery')
else:
    require(backup_repository, ['repository_owner_prevents_cross_library_reuse_even_without_index'], 'inherited 2a.13 backup ownership')
require(protocol, ['protected_pdf_annotation_guard_keeps_lock_waiting_until_response_scope_drops'], '2a.16 PDF annotation lease')
require(protection, [
    'OwnedPlaintext::reserve_vault_session(',
    'promote_vault_materialized_attachment(note_id, attachment.id.clone())',
    'failed_protected_attachment_authentication_leaves_no_untracked_plaintext',
    'plaintext cleanup is incomplete',
], '2a.16 protected attachment ownership')
plaintext_owner_markers = [
    'reserve_attachment_owned_plaintext', 'reserve_portability_owned_plaintext',
    'reserve_portability_blob_owned_plaintext', 'ensure_portability_blob_plaintext_stage_in_transaction',
    'transfer_owned_plaintext_stage', 'complete_owned_plaintext_transfer',
    'recover_managed_plaintext_staging_unlocked',
    'failed_plaintext_cleanup_keeps_durable_owner_for_retry',
]
if build_id not in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    plaintext_owner_markers += [
        'cleanup_managed_plaintext_stage_after_error',
        'recover_legacy_plaintext_staging_unlocked',
    ]
require(plaintext_staging, plaintext_owner_markers, '2a.16 durable plaintext ownership')
if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') and 'cleanup_managed_plaintext_stage_after_error' in plaintext_staging:
    raise SystemExit('2b.5 retained obsolete managed-plaintext cleanup helper')
if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') and 'recover_legacy_plaintext_staging_unlocked' in plaintext_staging:
    raise SystemExit('2b.5 must not retain obsolete pre-ledger plaintext staging recovery')
require(attachment_ingest, [
    'reserve_attachment_owned_plaintext(note_id, &staging_relative_path)',
    'owned_plaintext.copy_from(source)', 'transfer_owned_plaintext_stage(', 'complete_owned_plaintext_transfer(',
    'discard_attachment_ingest_job(&job)?', 'OwnedPlaintext::durable_attachment_ingest_existing',
    'failed_pending_ingest_cleanup_keeps_durable_job_owner',
], '2a.16 attachment ingest ownership')
require(portable_bundle, [
    'reserve_portability_blob_owned_plaintext(&relative)', 'owned_plaintext.create_file()',
    'struct MaterializedBundleBlob', 'owned_plaintext: crate::security::OwnedPlaintext',
    'materialized_blobs: &mut [MaterializedBundleBlob]',
    'referenced_blob_hashes',
], '2a.16 bundle blob ownership')
require(notes, [
    'discard_managed_plaintext_staging_for_note(note_id)?',
    'discard_attachment_ingests_for_note(note_id)?',
    'discard_attachment_operation_staging_for_note(note_id)?',
], '2a.16 delete-before-owner-loss')
require(journal, [
    'discard_protected_recovery_records(&valid_paths)?',
    'discard_protected_journal_records(storage, &directory)?',
    'startup_discards_corrupt_protected_records_before_quarantine',
    'startup_fails_closed_when_protected_root_record_cannot_be_removed',
    'startup_removes_legacy_quarantine_for_protected_note',
    'quarantine_cleanup_is_scoped_to_the_target_note',
], '2a.16 emergency journal closure')
require(temp_owner, ['abandoned_owner_lock_is_removed_without_pid_heuristics',
                     'markerless_v2_cleanup_publishes_claim_before_removal',
                     'v2_owner_setup_crash_window_is_removed_without_pid_heuristics'], 'inherited 2a.13 temp abnormal termination')
if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    for obsolete in ('legacy_directory_reusing_current_pid_is_removed', 'rustnote-vault-', 'session-*'):
        if obsolete in temp_owner:
            raise SystemExit(f'2b.5 retained obsolete temp-layout compatibility marker: {obsolete}')
if 'const TAG_BYTES: usize = 32;' not in vault or 'Aegis256::<TAG_BYTES>' not in vault:
    raise SystemExit('full 256-bit AEGIS-256 tag invariant missing')
allowed_vault_users = {
    'src/security/mod.rs',
    'src/security/backup_key_authority.rs',
    'src/platform/secure_key_store.rs',
}
for path in Path('src').rglob('*.rs'):
    s=path.read_text(errors='ignore')
    if path.as_posix() not in allowed_vault_users and 'rustnote_vault' in s:
        raise SystemExit(f'rustnote-vault bypasses audited security boundary: {path}')
PY
if [[ "${RUSTNOTE_E3_STATIC_ONLY:-0}" == "1" ]]; then
  echo "Security E3 static contract passed (build $BUILD_ID; Rust compilation skipped)."
  exit 0
fi
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
echo "Security E3 contract passed (build $BUILD_ID)."

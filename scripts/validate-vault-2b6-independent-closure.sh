#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
case "$BUILD_ID" in 2b.6|2b.7|2b.8|2b.9|2b.10|2b.11|2b.12|2b.13|2b.14|2b.15|2b.16|2b.17|2b.18|2b.19|2b.20|2b.21|2b.22|2b.23|2b.24|2b.25|2b.26|2b.27|2b.28|2b.29|2b.30|2b.31|2b.32|2b.33|2b.34|2b.35|2b.36|2b.37|2b.38|2b.39|2b.40|2b.41|2b.43) ;; *) echo "Expected build 2b.6+" >&2; exit 1 ;; esac

python3 - <<'PY'
from pathlib import Path
import re, sqlite3

def read(path):
    return Path(path).read_text(errors='ignore')

def require(source, markers, label):
    for marker in markers:
        if marker not in source:
            raise SystemExit(f'{label} missing: {marker}')

def segment(source, start, end=None):
    a = source.find(start)
    if a < 0:
        raise SystemExit(f'missing segment: {start}')
    if end is None:
        return source[a:]
    b = source.find(end, a + len(start))
    return source[a:b if b >= 0 else len(source)]

def raw_const(source, name):
    match = re.search(rf'const {name}: &str = r#"(.*?)"#;', source, re.S)
    if not match:
        raise SystemExit(f'cannot extract {name}')
    return match.group(1)

coord = read('src/security/privacy_coordinator.rs')
owned = read('src/security/plaintext.rs')
staging = read('src/storage/plaintext_staging.rs')
sensitive = read('src/security/sensitive_data.rs')
objects = read('src/storage/vault_objects.rs')
protection = read('src/storage/protection.rs')
notes = read('src/storage/notes.rs')
note_links = read('src/storage/note_links.rs')
schema = read('src/storage/schema.rs')
init = read('src/storage/initialization.rs')
backups = read('src/storage/backups.rs')
backup_repo = read('src/storage/backup_repository.rs')
journal = read('src/autosave/emergency_journal.rs')
ui_privacy = read('src/ui/privacy.rs')
ui_workflows = read('src/ui/editor_workflows.rs')
ui_picker = read('src/ui/note_picker.rs')
ui_metadata = read('src/ui/note_metadata.rs')
ui_links = read('src/ui/note_links.rs')
attachments = read('src/storage/attachments.rs')
ingest = read('src/storage/attachment_ingest.rs')
portability = read('src/storage/portability.rs')

# I1/I2 — one typed owner, ownership before materialization, unlink/absence before retirement.
require(owned, [
    'pub(crate) struct OwnedPlaintext',
    'PlaintextLifecycle::Reserved',
    'PlaintextLifecycle::Materialized',
    'PlaintextLifecycle::Transferred',
    'PlaintextLifecycle::Retiring',
    'PlaintextLifecycle::Retired',
    'fn path_entry_exists(path: &Path) -> io::Result<bool>',
    'fs::symlink_metadata(path)',
    'Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false)',
    'fn require_path_absent(path: &Path) -> io::Result<()>',
    'metadata_failure_is_not_treated_as_plaintext_absence',
], 'I1/I2 OwnedPlaintext authority')
production_owned = owned[:owned.find('#[cfg(test)]')]
if '.exists()' in production_owned:
    raise SystemExit('I2 production OwnedPlaintext still uses Path::exists() for lifecycle decisions')
release = segment(staging, 'pub(super) fn release_uncreated_owned_plaintext', 'pub(super) fn transfer_owned_plaintext_stage')
require(release, [
    'transaction_with_behavior(TransactionBehavior::Immediate)',
    'owned.ensure_path_absent()?;',
    'owned.mark_uncreated_durable_owner_retired()?;',
    'transaction.commit()?;',
], 'I2 uncreated-owner retirement transaction')
if not (release.find('owned.ensure_path_absent()?;') < release.find('DELETE FROM managed_plaintext_staging') < release.find('transaction.commit()?;')):
    raise SystemExit('I2 uncreated owner row can retire before absence proof')
transfer = segment(staging, 'pub(super) fn complete_owned_library_blob_rename', 'fn transfer_managed_plaintext_stage')
require(transfer, [
    'owned.ensure_path_absent()?;',
    'plaintext_transfer_successor_is_owned',
    'transaction_with_behavior(TransactionBehavior::Immediate)',
    'owned.retire_transferred_durable_owner()?;',
    'transaction.commit()?;',
], 'I2 crash-recoverable transfer retirement')
if '.exists()' in transfer:
    raise SystemExit('I2 restore-transfer completion still treats Path::exists() as an absence proof')
require(staging, [
    'reserve_managed_plaintext_stage(',
    'rename_owned_restore_plaintext_to_canonical',
    "transfer_state = 'prepared'",
    'recover_managed_plaintext_staging_unlocked',
], 'I1/I2 durable owner/recovery state machine')
require(attachments, ['reserve_attachment_owned_plaintext', 'copy_and_hash_from(source)'], 'I1 attachment ingestion')
require(ingest, ['reserve_attachment_owned_plaintext', 'OwnedPlaintext::durable_attachment_ingest_existing'], 'I1 durable attachment ingest')

# I3/I4 — one PrivacyCoordinator owns admission, producer gating, Vault revocation/drain and storage mutation ordering.
require(coord, [
    'pub(crate) struct PrivacyCoordinator',
    'pub(crate) fn begin_note_transition',
    'pub(crate) fn begin_note_transitions',
    'pub(crate) fn begin_library_transition',
    'fn admit_note(',
    'fn admit_library(',
    'publication_drained',
    'operation_lock',
    'plaintext_derivatives',
    'vault_runtime',
    'NotePrivacyTransition::Restore',
], 'I3/I4 coordinator')
for path in Path('src').rglob('*.rs'):
    if path.as_posix().startswith('src/security/'):
        continue
    source = path.read_text(errors='ignore')
    for primitive in (
        'begin_note_revocation(', 'begin_note_revocations(', 'begin_library_revocation(',
        'begin_plaintext_derivative_transition(', 'begin_library_plaintext_derivative_transition(',
    ):
        if primitive in source:
            raise SystemExit(f'I3 privacy primitive escaped coordinator: {path}: {primitive}')
require(notes, ['NotePrivacyTransition::Delete', 'retire_note_plaintext_before_delete_unlocked'], 'I4 hard delete authority')
require(portability, ['begin_multi_note_privacy_transition', 'NotePrivacyTransition::Delete'], 'I4 import rollback authority')
require(backups, ['begin_library_privacy_transition', 'LibraryPrivacyTransition::Restore'], 'I4 complete archive restore authority')
require(backup_repo, ['begin_library_privacy_transition', 'LibraryPrivacyTransition::Restore'], 'I4 repository restore authority')

# I5 — publication token combines coordinator authorization and UI generations; real plaintext handoffs lease it.
require(ui_privacy, [
    'pub(super) struct PrivacyPublicationToken',
    'authorization: PrivacyPublicationAuthorization',
    'pub(super) fn begin_publication',
    'pub(super) fn publish(&self, handoff: impl FnOnce()) -> bool',
    'let coordinator = self.authorization.begin_publication()?;',
], 'I5 publication authority')
require(ui_picker, ['publication_token.publish(||'], 'I5 note-picker publication')
require(ui_links, ['publication_token.begin_publication()'], 'I5 backlinks publication')
require(ui_metadata, ['publication_token.begin_publication()', 'publication_token.publish(||'], 'I5 metadata/revision publication')
restore_flow = segment(ui_workflows, 'pub(super) async fn restore_revision_flow', 'pub(super) async fn')
require(restore_flow, [
    'pre_restore_publication_token',
    'let publication_token = publication_scope',
    '.capture()',
    'publication_token.begin_publication()',
], 'I5 post-Restore publication reauthorization')

# I6 — default-private specification and narrow public stub drive scrub validation.
require(sensitive, [
    'pub(crate) struct SensitiveNoteData',
    'pub(crate) struct ProtectedPublicPersistenceSpec',
    'pub(crate) const SCRUB_STEPS',
    'pub(crate) const NOTE_COLUMNS',
    'pub(crate) const NOTE_SANITIZED_COLUMNS',
    'pub(crate) const CURRENT_PROTECTED_NOTE_FORBIDDEN_TABLES',
    'scrub_plan_covers_every_declared_sensitive_relational_surface',
], 'I6 sensitive-data authority')
require(protection, [
    'for &step in ProtectedPublicPersistenceSpec::SCRUB_STEPS',
    'INSERT INTO notes_fts(notes_fts) VALUES(\'rebuild\')',
    'PRAGMA secure_delete = ON',
    'ensure_wal_truncated(&connection)',
], 'I6 forensic scrub path')

# I7 — granular mutation locality and immutable revisions.
save = segment(protection, 'pub(super) fn persist_protected_note_content', 'pub(super) fn mutate_protected_metadata_if_unlocked')
require(save, ['read_vault_graph_head', 'needs_private_state', 'revision_head.as_ref()', 'commit_vault_graph_update'], 'I7 protected save locality')
for forbidden in ('read_vault_current_graph', 'read_vault_revision_chain'):
    if forbidden in save:
        raise SystemExit(f'I7 protected autosave hydrates unrelated graph/history: {forbidden}')
prepare_revision = segment(protection, 'fn prepare_next_vault_revision', 'fn commit_vault_graph_update')
if 'read_vault_revision_chain' in prepare_revision:
    raise SystemExit('I7 revision append traverses protected history')
require(prepare_revision, ['previous_revision_object', 'revision_head.as_ref()'], 'I7 O(1) revision append')
commit = segment(objects, 'pub(crate) fn commit_vault_manifest_root', 'pub(crate) fn replace_vault_blob_refs')
require(commit, [
    'let old_manifest: VaultManifest = self.read_vault_object(',
    '&root.manifest_object_id',
    '(&old_manifest.note_private_state, &new_manifest.note_private_state)',
    '(&old_manifest.metadata_state, &new_manifest.metadata_state)',
    '(&old_manifest.attachment_index, &new_manifest.attachment_index)',
], 'I7 object-store-owned mutable retirement')
signature = commit[:commit.find(') -> StorageResult<VaultObjectId>') + 1]
if 'old_manifest: &VaultManifest' in signature or 'superseded_mutable_objects' in signature:
    raise SystemExit('encrypted-object commit still trusts caller-supplied retirement provenance')
if 'revision_head' in commit or 'revision_floor' in commit:
    raise SystemExit('ordinary mutable-object retirement can directly delete revision/history objects')

# Object authenticity/crash consistency and one representation.
require(objects, [
    'VaultObjectKind::Manifest', 'VaultObjectKind::NotePrivateState',
    'VaultObjectKind::MetadataState', 'VaultObjectKind::AttachmentIndex',
    'VaultObjectKind::Revision', 'vault_object_context_bytes',
    'install_vault_note_root', 'delete_unreachable_vault_objects',
], 'granular authenticated object store')
for path in Path('src').rglob('*.rs'):
    source = path.read_text(errors='ignore')
    for obsolete in ('ProtectedNotePayload', 'protected_note_blobs'):
        if obsolete in source:
            raise SystemExit(f'obsolete protected representation remains: {path}: {obsolete}')
    if re.search(r'\bprotected_notes\b', source):
        raise SystemExit(f'obsolete protected_notes table/model remains: {path}')

# SQLite/WAL/FTS, snapshots and startup recovery.
require(backups, ['ensure_no_pending_vault_operations_for_snapshot', 'SELECT COUNT(*) FROM vault_operations'], 'snapshot Vault-operation barrier')
require(init, [
    'recover_managed_plaintext_staging_unlocked()?;',
    'cleanup_untracked_incoming_files(&self.list_attachment_operations()?)?;',
    'recover_vault_operations_unlocked()?;',
], 'startup crash recovery')
initialize = segment(init, 'pub(super) fn initialize(&self)', 'pub(super) fn install_runtime_database_aids')
for forbidden in ('read_vault_current_graph', 'read_vault_revision_chain', 'unlock_protected_note'):
    if forbidden in initialize:
        raise SystemExit(f'startup performs eager protected graph work: {forbidden}')
require(journal, ['if note.protection.is_protected()', 'discard_protected_journal_records'], 'protected emergency-journal denial/recovery')

# Independent adversarial corpus: require direct coverage of the highest-risk crash,
# corruption, residue, locality, attachment and protected↔unprotected boundaries.
require(protection, [
    'vault_manifest_swap_is_rollback_safe_and_missing_component_fails_closed',
    'large_protected_revision_history_does_not_expand_autosave_mutation_scope',
    'protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables',
    'protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot',
    'persistent_snapshots_fail_closed_while_vault_operation_is_pending',
    'protected_note_encrypts_attachment_blob_and_restores_it_losslessly',
    'failed_protected_attachment_authentication_leaves_no_untracked_plaintext',
    'vault_recovery_finishes_plaintext_after_unprotect_commit',
    'vault_recovery_finishes_protection_after_database_commit',
    'vault_recovery_rolls_back_prepared_unprotect_without_publishing_plaintext',
    'protected_unprotected_protected_cycle_preserves_granular_state_and_residue_rules',
], 'independent protected-state adversarial corpus')
require(coord, [
    'same_note_transition_conflicts_fail_closed',
    'protected_restore_rejects_existing_same_note_transition_without_waiting',
    'library_and_note_transitions_are_mutually_exclusive',
    'lock_waits_for_existing_reader_before_storage_mutation',
    'protect_waits_for_plaintext_producer_and_blocks_new_producer',
    'restore_invalidates_pre_restore_publication_and_allows_fresh_post_restore_authorization',
], 'independent transition/publication adversarial corpus')

# 2b.7 closure: prohibit the concrete blind spots that invalidated the prior certification.
protect_flow = segment(protection, 'pub(crate) fn protect_note_with_password', 'fn scrub_managed_plaintext_residue_after_protection')
if 'self.get_note(note_id)' in protect_flow:
    raise SystemExit('Protect re-enters public protected-note hydration while its transition is held')
lock_flow = segment(protection, 'pub(crate) fn lock_protected_note', 'pub(crate) fn remove_note_protection_with_password')
if 'self.get_note(note_id)' in lock_flow:
    raise SystemExit('Lock re-enters public protected-note hydration while its transition is held')
delete_cleanup = segment(notes, 'pub(super) fn retire_note_plaintext_before_delete_unlocked', 'pub(super) fn hard_delete_note_unlocked')
if 'self.list_attachments(note_id)' in delete_cleanup:
    raise SystemExit('Delete cleanup re-enters public protected attachment access')
restore_entry = segment(coord, 'pub(crate) fn begin_protected_note_restore', 'pub(crate) fn begin_retained_protected_note_mutation')
if 'wait_for_note_admission_clear(note_id)' in restore_entry:
    raise SystemExit('protected Restore still queues behind a same-note transition')
require(restore_entry, [
    'state.library_transition.is_some() || state.note_transitions.contains_key(note_id)',
    'return Err("privacy transition is already in progress");',
], 'I4 protected Restore fail-closed admission')
require(note_links, [
    'protected_outgoing_note_links_if_unlocked',
    'vault_note_roots root',
], 'protected link/backlink encrypted read cutover')
quarantine_move = segment(attachments, 'pub(super) fn move_blob_to_quarantine', 'pub(super) fn prune_unreferenced_attachment_records_unlocked')
if 'fs::rename' in quarantine_move:
    raise SystemExit('orphan quarantine bypasses typed crash-recoverable rename authority')
require(quarantine_move, [
    'require_canonical_blob_owned_plaintext',
    'reserve_orphan_blob_owned_plaintext',
    'rename_owned_canonical_blob_to_orphan',
    'complete_owned_library_blob_rename',
], 'I1/I2 orphan quarantine transfer authority')
require(staging, [
    'ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction',
    'validate_orphan_blob_relative_path',
    'orphan_transfer_recovers_after_rename_before_predecessor_retirement',
], 'I1/I2 orphan durability/recovery')
require(protection, [
    'protect_and_lock_complete_without_reentering_privacy_admission',
    'unlocked protected source must resolve its encrypted outgoing links',
], '2b.7 protected transition/link behavioral coverage')
require(notes, ['protected_permanent_delete_does_not_reenter_note_access'], '2b.7 protected delete coverage')
require(attachments, ['quarantine_transfer_keeps_durable_owner_until_plaintext_deletion'], '2b.7 quarantine lifetime coverage')

# Canonical schema itself must be executable and contain only the granular root/object tables.
base = raw_const(schema, 'BASE_SCHEMA')
db = sqlite3.connect(':memory:')
db.executescript(base)
if db.execute('PRAGMA user_version').fetchone()[0] != 21:
    raise SystemExit('canonical schema version is not 21')
for table in ('vault_objects', 'vault_note_roots', 'vault_blob_refs', 'managed_plaintext_staging'):
    if db.execute("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?", (table,)).fetchone() is None:
        raise SystemExit(f'canonical schema missing {table}')
for table in ('protected_notes', 'protected_note_blobs'):
    if db.execute("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?", (table,)).fetchone() is not None:
        raise SystemExit(f'canonical schema retained obsolete {table}')

print('Vault 2b.6 independent I1-I7 architectural/security closure guard passed.')
PY

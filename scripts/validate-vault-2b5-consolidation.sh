#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
case "$BUILD_ID" in 2b.5|2b.6|2b.7|2b.8|2b.9|2b.10|2b.11|2b.12|2b.13|2b.14|2b.15|2b.16|2b.17|2b.18|2b.19|2b.20|2b.21|2b.22|2b.23|2b.24|2b.25|2b.26|2b.27|2b.28|2b.29|2b.30|2b.31|2b.32|2b.33|2b.34|2b.35|2b.36|2b.37|2b.38|2b.39|2b.40|2b.41|2b.43) ;; *) echo "Expected build 2b.5+" >&2; exit 1 ;; esac

python3 - <<'PY'
from pathlib import Path
import re, sqlite3

build_id = Path('build-id.txt').read_text().strip()

def read(path): return Path(path).read_text(errors='ignore')
def require(src, markers, label):
    for marker in markers:
        if marker not in src: raise SystemExit(f'{label} missing: {marker}')
def segment(src, start, end=None):
    a=src.find(start)
    if a < 0: raise SystemExit(f'missing function/segment: {start}')
    if end is None: return src[a:]
    b=src.find(end, a+len(start))
    return src[a:b if b >= 0 else len(src)]

def raw_const(src,name):
    m=re.search(rf'const {name}: &str = r#"(.*?)"#;',src,re.S)
    if not m: raise SystemExit(f'cannot extract {name}')
    return m.group(1)

schema=read('src/storage/schema.rs')
init=read('src/storage/initialization.rs')
coord=read('src/security/privacy_coordinator.rs')
owned=read('src/security/plaintext.rs')
staging=read('src/storage/plaintext_staging.rs')
attachments=read('src/storage/attachments.rs')
attachment_ingest=read('src/storage/attachment_ingest.rs')
protection=read('src/storage/protection.rs')
notes=read('src/storage/notes.rs')
sensitive=read('src/security/sensitive_data.rs')
note_candidates=read('src/storage/note_candidates.rs')
backups=read('src/storage/backups.rs')
native_staging=read('src/native/staging.rs')
private_temp=read('src/native/private_temp.rs')
temp_owner=read('src/native/temp_owner.rs')
runtime=read('src/security/runtime.rs')
native_mod=read('src/native/mod.rs')

# O1 — inherited 2b.1–2b.4 closure holes remain repaired.
require(coord,[
    'NotePrivacyTransition::Restore','protected_restore_rejects_existing_same_note_transition_without_waiting',
    'admitted_lock_blocks_new_reader_before_vault_revocation_begins',
    'admitted_protect_blocks_new_plaintext_producer_before_derivative_drain',
    'restore_is_a_same_note_conflicting_transition',
], 'coordinator linearization')
restore=segment(protection,'pub(super) fn restore_protected_revision_if_unlocked','pub(super) fn persist_protected_runtime_normalization_if_unlocked')
require(restore,['begin_protected_note_restore(note_id)?','restore_guard.access()','restore_guard.into_retained_access()'], 'protected Restore transition authority')
require(protection,['for &step in ProtectedPublicPersistenceSpec::SCRUB_STEPS','protected_note_scrub_assignment','protected_attachment_scrub_assignment'], 'default-private scrub authority')
require(sensitive,['pub(crate) const SCRUB_STEPS','scrub_plan_covers_every_declared_sensitive_relational_surface'], 'canonical scrub specification')

# O2 — obsolete architecture/compatibility is actually deleted, not hidden.
for path in Path('src').rglob('*.rs'):
    source=path.read_text(errors='ignore')
    for obsolete in (
        'ProtectedNotePayload','protected_note_blobs',
        'recover_legacy_plaintext_staging_unlocked','recover_legacy_retiring_plaintext_unlocked',
        'cleanup_legacy_incoming_unlocked','scrub_existing_protected_residue_for_e3_unlocked',
        'vault.e3_privacy_scrub',
    ):
        if obsolete in source:
            raise SystemExit(f'obsolete 2a/2b compatibility remains in production source: {path}: {obsolete}')
    if re.search(r'\bprotected_notes\b', source):
        raise SystemExit(f'obsolete 2a/2b protected_notes representation remains in production source: {path}')
if 'const SCHEMA_' in schema:
    raise SystemExit('historical pre-release migration ladder remains in production schema')
require(init,['0 => {','transaction.execute_batch(BASE_SCHEMA)?;','CURRENT_SCHEMA_VERSION => Ok(())'], 'fresh/current-only pre-release schema')
for old_version in ('12 => {','13 => {','14 => {','15 => {','16 => {','17 => {','18 => {','19 => {','20 => {'):
    if old_version in init: raise SystemExit(f'obsolete schema compatibility branch remains: {old_version}')

for obsolete_api in (
    'attachment_ids_for_request',
    'attachment_is_protected',
    'export_markdown_library',
    'export_markdown_notebook',
    'cleanup_managed_plaintext_stage_after_error',
    'protected_note_access_if_unlocked',
    'is_note_protected_canonical',
    'scrub_protected_attachment_ingests_for_vault_unlocked',
    'legacy_rustnote_temp_directory',
    'cleanup_current_and_legacy_sensitive_stage',
    'cleanup_prefixed_session_directories_owned_like',
    'legacy_pid_removable',
):
    for path in Path('src').rglob('*.rs'):
        if obsolete_api in path.read_text(errors='ignore'):
            raise SystemExit(f'dead/obsolete 2b.5 API remains: {path}: {obsolete_api}')
if Path('src/native/process.rs').exists() or 'mod process;' in native_mod:
    raise SystemExit('obsolete PID-liveness compatibility module remains')
for source,label in (
    (runtime,'Vault runtime'),
    (private_temp,'private temp'),
    (temp_owner,'temp owner'),
    (native_staging,'native staging'),
):
    for obsolete in ('legacy 2a.11','legacy PID','migration cleanup'):
        if obsolete in source:
            raise SystemExit(f'{label} retains obsolete temp-layout compatibility: {obsolete}')
require(temp_owner,[
    'name.starts_with("session-v2-")',
    'claim_abandoned_owner_for_cleanup',
    'claim_unowned_v2_for_cleanup',
], 'current v2 temp ownership cleanup')

# Current crash recovery stays: deletion must not accidentally remove live ownership/recovery.
require(init,[
    'self.recover_managed_plaintext_staging_unlocked()?;',
    'self.cleanup_untracked_incoming_files(&self.list_attachment_operations()?)?;',
    'self.recover_vault_operations_unlocked()?;',
], 'current startup recovery')
require(staging,[
    'recover_managed_plaintext_staging_unlocked','managed_plaintext_stage_is_required_transfer_successor',
    'managed_plaintext_transfer_successor_is_owned','plaintext_transfer_successor_is_owned',
    'get_managed_plaintext_stage','list_managed_plaintext_stages','managed_plaintext_stage_path',
], 'current durable plaintext ledger')
require(protection,['cleanup_stale_vault_incoming_unlocked','recover_vault_operations_unlocked'], 'current Vault-operation recovery')

# One plaintext authority extends through attachment-operation staging/rename/delete.
require(owned,[
    'DurableManagedAuthority::AttachmentOperation','durable_attachment_operation_existing',
    'rename_durable_attachment_operation','failed_attachment_operation_rename_retains_predecessor_ownership',
    'DurableManagedAuthority::AttachmentIngest','durable_attachment_ingest_existing',
], 'attachment-operation OwnedPlaintext authority')
materialize=segment(attachments,'fn materialize_attachment_blob','pub(super) fn discard_attachment_operation_staging_for_note')
require(materialize,['OwnedPlaintext::durable_attachment_operation_existing','rename_durable_attachment_operation'], 'attachment materialization ownership')
if 'fs::rename(&staging_path, &final_path)' in materialize:
    raise SystemExit('attachment operation raw-renames managed plaintext')
discard=segment(attachments,'fn discard_staged_attachment','fn attachment_blob_is_verified')
require(discard,['OwnedPlaintext::durable_attachment_operation_existing','owned.remove_file()?'], 'attachment cleanup ownership')
discard_ingest=segment(attachment_ingest,'fn discard_attachment_ingest_job','fn discard_attachment_ingest_jobs')
require(discard_ingest,[
    'OwnedPlaintext::durable_attachment_ingest_existing',
    'owned.remove_file()?','delete_attachment_ingest_job',
], 'attachment-ingest durable ownership')
if discard_ingest.find('delete_attachment_ingest_job') < discard_ingest.find('owned.remove_file()?'):
    raise SystemExit('attachment ingest owner can retire before plaintext unlink succeeds')
require(attachment_ingest,['failed_pending_ingest_cleanup_keeps_durable_job_owner'], 'attachment-ingest cleanup failure regression')
incoming_cleanup=segment(attachments,'pub(super) fn cleanup_untracked_incoming_files','pub(super) fn quarantine_blob_if_unreferenced')
require(incoming_cleanup,['crate::native::privacy::remove_file(&entry.path())?'], 'unowned incoming privacy cleanup boundary')
if 'fs::remove_file(entry.path())' in incoming_cleanup:
    raise SystemExit('unowned incoming plaintext bypasses privacy cleanup boundary')

# No production caller recreates old transition primitives.
for path in Path('src').rglob('*.rs'):
    if path.as_posix().startswith('src/security/'):
        continue
    src=path.read_text(errors='ignore')
    for marker in ('begin_note_revocation(','begin_note_revocations(','begin_library_revocation(',
                   'begin_plaintext_derivative_transition(','begin_library_plaintext_derivative_transition('):
        if marker in src: raise SystemExit(f'old privacy choreography escaped coordinator: {path}: {marker}')

# O3 — algorithmic performance closure. Ordinary notes take a cheap protection
# decision; protected autosave opens only mutation-local objects and at most the
# revision head, not history.
persist_note=segment(notes,'pub(super) fn persist_note','pub(super) fn persist_existing_note_scoped')
require(persist_note,['protected_note_exists(transaction, &note.id)','Ok(false) => {}'], 'ordinary note Vault-free branch')
save=segment(protection,'pub(super) fn persist_protected_note_content','pub(super) fn mutate_protected_metadata_if_unlocked')
require(save,['read_vault_graph_head','needs_private_state','revision_head.as_ref()','read_vault_object'], 'protected autosave locality')
for forbidden in ('read_vault_current_graph','read_vault_revision_chain'):
    if forbidden in save: raise SystemExit(f'protected autosave scales with unrelated state/history: {forbidden}')
prepare=segment(protection,'fn prepare_next_vault_revision','fn commit_vault_graph_update')
if 'read_vault_revision_chain' in prepare: raise SystemExit('revision append traverses history')
require(prepare,['revision_head.as_ref()','previous_revision_object'], 'O(1) revision append')
# Startup must recover small ledgers but never decrypt/traverse every protected graph.
initialize=segment(init,'pub(super) fn initialize(&self)','pub fn deferred_maintenance')
for forbidden in ('read_vault_current_graph','read_vault_revision_chain','unlock_protected_note'):
    if forbidden in initialize: raise SystemExit(f'startup performs protected graph/history work: {forbidden}')

# Ordinary search remains FTS5 and does not require Vault graph reconstruction.
require(note_candidates,['notes_fts WHERE notes_fts MATCH ?1'], 'ordinary search FTS5 path')
for forbidden in ('read_vault_current_graph','read_vault_revision_chain','read_vault_object'):
    if forbidden in note_candidates:
        raise SystemExit(f'ordinary candidate search performs Vault object work: {forbidden}')

# Unlock authenticates/materializes the current graph once and never walks history.
unlock=segment(protection,'pub(crate) fn unlock_protected_note','pub(crate) fn lock_protected_note')
if unlock.count('read_vault_current_graph') != 1:
    raise SystemExit('protected Unlock must authenticate/decrypt the current graph exactly once')
for forbidden in ('read_vault_revision_chain','self.get_note(note_id)'):
    if forbidden in unlock:
        raise SystemExit(f'protected Unlock performs redundant/history work: {forbidden}')
require(unlock,['materialize_vault_note(base, &graph)?','insert_note_key'], 'single-pass protected Unlock')

# Lock operates on authorization/staging metadata only; it must not decrypt the
# protected graph or traverse revision history.
lock=segment(protection,'pub(crate) fn lock_protected_note','pub(crate) fn remove_note_protection_with_password')
for forbidden in ('read_vault_current_graph','read_vault_revision_chain','read_vault_object'):
    if forbidden in lock:
        raise SystemExit(f'Lock performs unnecessary protected-object work: {forbidden}')

# Protected attachment opening reads only the manifest head + attachment index,
# then streams one independently encrypted blob into Vault-session plaintext.
if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    open_attachment=segment(protection,'pub(super) fn protected_attachment_with_access','pub(super) fn protected_attachment_resource')
else:
    open_attachment=segment(protection,'pub(super) fn protected_attachment_resource','fn with_unlocked_metadata_state')
require(open_attachment,['read_vault_graph_head','manifest.attachment_index','materialize_protected_attachment'], 'protected attachment open locality')
for forbidden in ('read_vault_current_graph','read_vault_revision_chain'):
    if forbidden in open_attachment:
        raise SystemExit(f'protected attachment open hydrates unrelated protected state: {forbidden}')
materialize_protected=segment(protection,'fn materialize_protected_attachment','pub(super) fn prune_unlocked_protected_revision_history')
require(materialize_protected,['reserve_vault_session','open_protected_blob','BufReader::new','BufWriter::new'], 'streamed protected attachment materialization')

# Note-only Markdown export no longer carries the obsolete library/notebook scope
# machinery or scans the entire note table to find one requested note.
export_note=segment(backups,'fn export_markdown_scope_controlled','fn export_one_markdown_note')
require(export_note,['WHERE n.id = ?1','statement.query([&scope.note_id])'], 'note export targeted query')
for obsolete in ('MarkdownExportScope','notebook_export_paths','FROM notebooks'):
    if obsolete in export_note:
        raise SystemExit(f'obsolete/broad Markdown export scope remains: {obsolete}')

# O4 — one steady-state protected representation and granular GC.
require(schema,['CREATE TABLE vault_objects','CREATE TABLE vault_note_roots','CREATE TABLE vault_blob_refs'], 'single granular schema')
require(protection,['commit_vault_graph_update','delete_unreachable_vault_objects','prune_unlocked_protected_revision_history'], 'granular mutation/GC')
if re.search(r'INSERT\s+INTO\s+note_revisions', restore, re.I):
    raise SystemExit('protected revision restore repopulates ordinary revision storage')

# Executable canonical schema proof.
base=raw_const(schema,'BASE_SCHEMA')
db=sqlite3.connect(':memory:'); db.executescript(base)
if db.execute('PRAGMA user_version').fetchone()[0] != 21: raise SystemExit('BASE_SCHEMA is not schema 21')
for table in ('vault_objects','vault_note_roots','vault_blob_refs','managed_plaintext_staging','attachment_operations'):
    if db.execute("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?",(table,)).fetchone() is None:
        raise SystemExit(f'canonical schema missing {table}')

# O5 — required expanded adversarial proof remains in the Rust test corpus.
for marker in (
    'admitted_lock_blocks_new_reader_before_vault_revocation_begins',
    'admitted_protect_blocks_new_plaintext_producer_before_derivative_drain',
    'failed_attachment_operation_rename_retains_predecessor_ownership',
    'failed_pending_ingest_cleanup_keeps_durable_job_owner',
    'protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only',
    'large_protected_revision_history_does_not_expand_autosave_mutation_scope',
    'unlocked protected note accepts encrypted attachment ingestion',
    'restart after a pre-commit crash boundary must retain the previous committed root',
    'restart after manifest commit must expose the new committed root',
):
    if marker not in (coord+owned+protection+attachment_ingest): raise SystemExit(f'2b.5 adversarial coverage missing: {marker}')

print('Vault 2b.5 consolidation/performance static/adversarial guard passed.')
PY

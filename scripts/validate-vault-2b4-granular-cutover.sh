#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

python3 - <<'PY'
from pathlib import Path

build_id = Path('build-id.txt').read_text().strip()
import re, sqlite3

build_id=Path('build-id.txt').read_text().strip()
def read(p): return Path(p).read_text(errors='ignore')
def require(src, markers, label):
    for m in markers:
        if m not in src: raise SystemExit(f'{label} missing: {m}')
def segment(src, start, end):
    a=src.find(start)
    if a < 0: raise SystemExit(f'missing segment start: {start}')
    b=src.find(end, a+len(start))
    return src[a:b if b >= 0 else len(src)]
def raw_const(src,name):
    m=re.search(rf'const {name}: &str = r#"(.*?)"#;',src,re.S)
    if not m: raise SystemExit(f'could not extract SQL constant {name}')
    return m.group(1)

schema=read('src/storage/schema.rs')
init=read('src/storage/initialization.rs')
sensitive=read('src/security/sensitive_data.rs')
objects=read('src/storage/vault_objects.rs')
protection=read('src/storage/protection.rs')
notes=read('src/storage/notes.rs')
tags=read('src/storage/tags.rs')
sources=read('src/storage/sources.rs')
transport=read('src/storage/transport.rs')
ui_editor=read('src/ui/editor.rs')
ui_privacy=read('src/ui/privacy.rs')
ui_workflows=read('src/ui/editor_workflows.rs')
application_notes=read('src/application/notes.rs')
storage_capabilities=read('src/storage/capabilities.rs')
ui_entity_commands=read('src/ui/entity_commands.rs')

# Canonical schema/current-only cutover.
require(schema,[
    'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;', 'pub(super) const BASE_SCHEMA',
    'CREATE TABLE vault_objects', 'CREATE TABLE vault_note_roots', 'CREATE TABLE vault_blob_refs',
    'PRAGMA user_version = 21;',
], 'schema-21 granular model')
if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    for p in Path('src').rglob('*.rs'):
        source=p.read_text(errors='ignore')
        for obsolete in ('ProtectedNotePayload','protected_note_blobs','const SCHEMA_'):
            if obsolete in source:
                raise SystemExit(f'2b.5 obsolete protected representation/migration remains: {p}: {obsolete}')
        if re.search(r'\bprotected_notes\b', source):
            raise SystemExit(f'2b.5 obsolete protected_notes representation remains: {p}')
    require(init,['0 => {','transaction.execute_batch(BASE_SCHEMA)?;','CURRENT_SCHEMA_VERSION => Ok(())'], 'fresh/current schema initialization')

# Canonical data/object vocabulary.
require(sensitive,[
    'struct SensitiveNoteData','struct NotePrivateState','struct MetadataState',
    'struct AttachmentIndex','struct RevisionObject','struct VaultBlobReference',
    'Deliberately **not** serializable',
], 'granular sensitive-data vocabulary')
require(objects,[
    'pub(crate) fn insert_vault_object','pub(crate) fn read_vault_object',
    'pub(crate) fn read_vault_graph_head','pub(crate) fn read_vault_current_graph',
    'pub(crate) fn commit_vault_manifest_root','pub(crate) fn read_vault_revision_chain',
], 'encrypted object store')

# Cutover/mutation locality.
require(notes,['protected_note_mutation_if_unlocked','persist_protected_note_content'], 'protected save routing')
save=segment(protection,'pub(super) fn persist_protected_note_content','pub(super) fn mutate_protected_metadata_if_unlocked')
require(save,[
    'read_vault_graph_head','needs_private_state','new_analysis.note_links != old_analysis.note_links',
    'new_analysis.attachment_ids != old_analysis.attachment_ids','private_changed.then_some(next)',
    'metadata_changed.then_some(next)','index_changed.then_some(next)','append_revision.as_ref()',
], 'protected save locality')
if 'read_vault_current_graph' in save or 'read_vault_revision_chain' in save:
    raise SystemExit('routine protected save regressed to full-graph/history traversal')
prepare=segment(protection,'fn prepare_next_vault_revision','fn commit_vault_graph_update')
require(prepare,['revision_head.as_ref()','read_vault_object','previous_revision_object'], 'O(1) revision append')
if 'read_vault_revision_chain' in prepare:
    raise SystemExit('revision append traverses existing protected history')
metadata=segment(protection,'pub(super) fn mutate_protected_metadata_if_unlocked','pub(super) fn persist_protected_attachment_from_staging_if_unlocked')
if 'read_vault_current_graph' in metadata: raise SystemExit('metadata-only mutation hydrates whole graph')
attachment=segment(protection,'pub(super) fn persist_protected_attachment_from_staging_if_unlocked','pub(super) fn remove_protected_attachment_if_unlocked')
if 'read_vault_current_graph' in attachment: raise SystemExit('attachment mutation hydrates whole graph')
require(sources,['mutate_protected_metadata_if_unlocked','SensitiveSourceLink'], 'sources/citations metadata authority')
require(transport,['replace_protected_pdf_annotation_page_if_unlocked'], 'PDF metadata authority')

require(tags,[
    'mutate_protected_metadata_if_unlocked',
    'mutate_protected_metadata_with_access',
    'begin_all_protected_notes_tag_mutation',
    'begin_protected_notes_mutation',
], 'tag metadata authority')
remove_tag=segment(tags,'pub fn remove_tag_from_note','pub fn delete_tag')
ordered_remove = [
    'mutate_protected_metadata_if_unlocked',
    'metadata.tags.retain',
    'return Ok(removed);',
    'DELETE FROM note_tags WHERE note_id = ?1 AND tag_id = ?2',
]
pos=0
for marker in ordered_remove:
    nxt=remove_tag.find(marker,pos)
    if nxt < 0:
        raise SystemExit(f'protected tag removal cutover missing/out of order: {marker}')
    pos=nxt+len(marker)
rename_tag=segment(tags,'pub fn rename_tag','pub fn remove_tag_from_note')
require(rename_tag,[
    'begin_all_protected_notes_tag_mutation()',
    'mutate_protected_metadata_with_access',
    'metadata.tags',
    'protected_affected',
], 'protected global tag rename')
delete_tag=segment(tags,'pub fn delete_tag','fn begin_all_protected_notes_tag_mutation')
require(delete_tag,[
    'begin_all_protected_notes_tag_mutation()',
    'mutate_protected_metadata_with_access',
    'metadata.tags.retain',
    'protected_affected',
], 'protected global tag delete')
require(tags,[
    'protected_tag_commands_mutate_encrypted_metadata_without_plaintext_relationships',
    'global_tag_rename_and_delete_cover_unlocked_protected_metadata',
    'global_tag_mutation_fails_closed_when_any_protected_note_is_locked',
    'SELECT COUNT(*) FROM note_tags WHERE note_id = ?1',
], 'protected tag command executable regressions')

require(storage_capabilities,[
    'protected_tag_note_store_commands_use_encrypted_metadata',
    '.remove_tag_from_note(&note.id, "secret")',
    'rename_tag("shared", "renamed")',
    'delete_tag("renamed")',
], 'NoteStore tag-command capability regression')
require(application_notes,[
    'pub(crate) fn remove_tag_from_note',
    '.remove_tag_from_note(note_id, name)',
    'pub(crate) fn rename_tag',
    '.rename_tag(current_name, new_name)',
    'pub(crate) fn delete_tag',
    '.delete_tag(name)',
], 'application tag-command capability routing')
require(ui_entity_commands,[
    'TagCommand::RemoveFromNote',
    '.remove_tag_from_note(note_id, name)',
    'TagCommand::Rename',
    '.rename_tag(from, to)',
    'TagCommand::Delete',
    '.delete_tag(name)',
], 'UI tag command routing')

# Public persistence must be scrubbed from the canonical plan.
require(protection,[
    'fn sanitize_protected_note_rows','for &step in ProtectedPublicPersistenceSpec::SCRUB_STEPS',
    'protected_note_scrub_assignment','protected_attachment_scrub_assignment',
    'DELETE FROM note_tags WHERE note_id = ?1','DELETE FROM note_revisions WHERE note_id = ?1',
    'DELETE FROM note_links WHERE source_note_id = ?1','DELETE FROM note_sources WHERE note_id = ?1',
], 'protected plaintext scrub')

# Core operations remain granular and editable when unlocked.
require(protection,[
    'read_vault_current_graph','read_vault_revision_chain','restore_protected_note_rows',
    'DELETE FROM vault_note_roots','DELETE FROM vault_objects',
    'pub(super) fn prune_unlocked_protected_revision_history','delete_unreachable_vault_objects',
], 'unprotect/revision/GC model')
require(ui_editor,['Encrypted note — unlocked for this session.'], 'unlocked protected editor state')

# A protected granular revision restore is not complete merely when the root
# commits: Restore admission invalidates pre-mutation publication authority, so
# the restored object graph must be published only under a fresh token.
require(ui_privacy,[
    'struct NotePrivacyPublicationScope',
    'fn note_publication_scope(',
], 'semantic note publication scope')
restore_flow=segment(ui_workflows,'pub(super) async fn restore_revision_flow','async fn fail_attachment_picker')
require(restore_flow,[
    '.restore_revision(current, request.revision_id)',
    'let publication_token = publication_scope',
    '.capture()',
    'publication_token,',
], 'post-Restore publication refresh')
post_restore=restore_flow[restore_flow.index('.restore_revision(current, request.revision_id)'):]
if 'pre_restore_publication_token' in post_restore:
    raise SystemExit('granular revision restore reuses stale pre-Restore authorization after commit')
require(ui_editor,['result.publication_token.begin_publication()'], 'restored-state final UI publication')

# Mandatory behavioral/adversarial coverage markers.
for marker in [
    'granular_protected_mutations_rewrite_only_their_object_class',
    'vault_manifest_swap_is_rollback_safe_and_missing_component_fails_closed',
    'explicit_protected_revision_maintenance_prunes_tail_and_unreachable_objects',
    'protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables',
    'protected_unprotected_protected_cycle_preserves_granular_state_and_residue_rules',
    'failed_protected_attachment_authentication_leaves_no_untracked_plaintext',
    'protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only',
    'large_protected_revision_history_does_not_expand_autosave_mutation_scope',
    'unlocked protected note accepts encrypted attachment ingestion',
    'restart after a pre-commit crash boundary must retain the previous committed root',
    'restart after manifest commit must expose the new committed root',
]:
    if marker not in protection: raise SystemExit(f'2b.4 adversarial coverage missing: {marker}')
require(objects,['encrypted Vault object must contain authenticated ciphertext','AuthenticationFailed'], 'encrypted-object corruption coverage')

# Fresh schema is executable and has only granular Vault tables.
base=raw_const(schema,'BASE_SCHEMA')
db=sqlite3.connect(':memory:'); db.executescript(base)
if db.execute('PRAGMA user_version').fetchone()[0] != 21: raise SystemExit('schema version invariant failed')
for table in ('vault_objects','vault_note_roots','vault_blob_refs'):
    if db.execute("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?",(table,)).fetchone() is None:
        raise SystemExit(f'canonical Vault table missing: {table}')

# RTF removal remains permanent.
if Path('src/portability/rtf.rs').exists(): raise SystemExit('RTF codec reappeared')
for p in [*Path('src').rglob('*.rs'),*Path('editor-src').rglob('*')]:
    if p.is_file() and re.search(r'(?i)(\.rtf\b|\bPortabilityFormat::Rtf\b|\bmod\s+rtf\b)',p.read_text(errors='ignore')):
        raise SystemExit(f'RTF import/export capability reappeared: {p}')

print('Vault 2b.4 granular protected-note cutover static/adversarial guard passed.')
PY

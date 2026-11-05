#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

for required in NATIVE_COMPILE_2B13_PLAN.md NATIVE_COMPILE_2B13_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B13.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.13 closure file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path

build_id = Path('build-id.txt').read_text().strip()

def read(path):
    return Path(path).read_text()

def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f'{label} missing: {marker}')

def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f'{label} retains forbidden marker: {marker}')

def segment(text, start, end=None):
    i=text.index(start)
    if end is None:
        return text[i:]
    j=text.find(end, i + len(start))
    return text[i:] if j < 0 else text[i:j]

editor = read('src/ui/editor.rs')
links = read('src/ui/note_links.rs')
storage_mod = read('src/storage/mod.rs')
caps = read('src/storage/capabilities.rs')
app = read('src/ui/app.rs')
runtime = read('src/security/runtime.rs')
sensitive = read('src/security/sensitive_data.rs')
backups = read('src/storage/backups.rs')
attachments = read('src/storage/attachments.rs')
locations = read('src/storage/locations.rs')
live = read('src/ui/live_editor.rs')
card = read('src/ui/note_card.rs')
metadata = read('src/ui/note_metadata.rs')
right = read('src/ui/right_panel.rs')
workflows = read('src/application/editor_workflows.rs')
ui_workflows = read('src/ui/editor_workflows.rs')
security_mod = read('src/security/mod.rs')

require(editor, ['let editor_key = format!', 'key: "{editor_key}"'], 'Dioxus editor key')
forbid(editor, ['key: format!'], 'Dioxus editor key')
require(links, ['let backlink_count = if backlinks_are_current', '"{backlink_count} backlinks"'], 'backlink RSX formatting')
forbid(links, ['"{if backlinks_are_current'], 'backlink RSX formatting')

require(storage_mod, ['AttachmentFileCommandResource, AttachmentStore'], 'storage capability export')
require(caps, ['Storage, StorageError,', 'impl AttachmentStore {'], 'storage capability imports')
attachment_store = segment(caps, 'impl AttachmentStore {', 'impl ExportStore {')
require(attachment_store, ['fn list_attachments_with_authorization(', 'fn attachment_file_command_resource('], 'attachment capability facade')
editor_store = segment(caps, 'impl EditorStore {', 'impl AttachmentStore {')
forbid(editor_store, ['fn list_attachments_with_authorization('], 'editor capability facade')

saved = segment(app, 'EditorPersistenceEvent::Saved')
require(saved, [
    'capture_note_publication(',
    '&saved_note.id,',
    'note_token.begin_publication()',
    'capture_multi_note_publication(&services.read().privacy(), list_generation)',
    'aggregate_token.begin_publication()',
], 'scoped autosave privacy publication')
forbid(saved, ['capture_library_publication(&services.read().privacy(), list_generation)'], 'obsolete autosave library-wide publication')

require(runtime, ['impl fmt::Debug for VaultPlaintextPath', 'VaultPlaintextPath([REDACTED])'], 'redacted Vault plaintext Debug')
forbid(runtime, ['#[derive(Debug)]\npub(crate) struct VaultPlaintextPath'], 'Vault plaintext Debug')
require(sensitive, ['let mut previous_no: Option<i64> = None;'], 'revision-index typing')
require(backups, ['match result {\n            Ok(_) => Ok(prepared),'], 'restore-blob result type')

profiled = segment(attachments, 'pub(super) fn finish_staged_attachment_import_profiled', 'pub(super) fn')
require(profiled, ['mut staged: StagedAttachmentImport'], 'staged attachment mutability')
require(locations, ['data_directory: data_directory.clone()'], 'backup-history ownership')
require(live, ['let publication_token = publication_token.clone();\n        spawn(async move {'], 'live-editor publication ownership')
require(card, [
    'let selection_generation = *state.note_selection_generation.peek();',
    '.wait_for_multi_note_publication_token(Some(selection_generation))',
], 'current scoped signal/publication capture')
forbid(card, ['let next_list_generation =', 'let next_selection_generation ='], 'obsolete pre-scoped signal capture')
require(metadata, ['let preview_note_id = note_id.clone();', 'let note_id = preview_note_id.clone();'], 'revision preview note ownership')
require(right, ['let open_privacy = privacy.clone();', 'let menu_privacy = privacy;', 'open_privacy.clone()', 'menu_privacy.clone()'], 'attachment-panel privacy ownership')

finalization = segment(workflows, 'pub(crate) struct AttachmentImportFinalization', '#[derive(Debug)]')
forbid(finalization, ['note_id:', 'request_id:'], 'attachment finalization dead state')
start = segment(workflows, 'pub(crate) fn start_attachment_finalization', 'pub(crate) async fn complete_attachment_finalization')
forbid(start, ['note_id: NoteId', 'request_id: String'], 'attachment finalization dead parameters')
forbid(ui_workflows, ['let note_id = current.id.clone();'], 'attachment workflow dead local')

require(security_mod, ['#[cfg(test)]\npub(crate) use plaintext::PlaintextLifecycle;'], 'test-only plaintext exports')

# Prevent exact compiler-reported bad patterns from returning.
for path in Path('src/ui').glob('*.rs'):
    text = path.read_text()
    if 'key: format!' in text:
        raise SystemExit(f'Dioxus key format macro reintroduced: {path}')
    if '"{if ' in text:
        raise SystemExit(f'inline-if Dioxus format segment reintroduced: {path}')

print('Rustnote 2b.13 native compile-closure static guard passed.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then

  bash scripts/validate-vault-2b12-aegis-privacy-disclosure-closure.sh
  bash scripts/validate-vault-2b11-atomic-privacy-authority-closure.sh
  bash scripts/validate-security-e0-e3-ownership-2a16.sh
  bash scripts/validate-security-e0-e3-residue-2a17.sh
  bash scripts/validate-security-e0-e3-ui-lifetime-2a18.sh
  bash scripts/validate-security-e0-e3-adversarial-closure-2a19.sh
fi

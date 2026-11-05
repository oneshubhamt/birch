#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking B3 editor workflow orchestration boundary..."
node <<'NODE'
const fs = require('node:fs');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required B3 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}

const editor = read('src/ui/editor.rs');
const uiWorkflows = read('src/ui/editor_workflows.rs');
const appWorkflows = read('src/application/editor_workflows.rs');
const appMod = read('src/application/mod.rs');
const uiMod = read('src/ui/mod.rs');
const context = read('src/application/context.rs');
const errors = read('src/application/errors.rs');

for (const marker of [
  'prepare_import_batch(',
  'finalize_import_batch(',
  'attachment_ids_for_request(',
  'recover_pending(',
  'probe_media_compatibility(',
  'prepare_media_compatibility(',
  'invalidate_media_compatibility(',
  'save_staged_pdf_export(',
  'commit_image_edit(',
  'import_images(',
  'persist_runtime_normalization(',
  '.restore_revision(',
  'ApplicationExecutor::start_read(',
]) {
  if (editor.includes(marker)) {
    throw new Error(`B3 workflow sequencing survived in ui/editor.rs: ${marker}`);
  }
}

for (const marker of [
  'async fn restore_revision_flow(',
  'struct RestoredRevision',
  'impl AttachmentPickerKind',
  'fn attachment_operation_active(',
]) {
  if (editor.includes(marker)) {
    throw new Error(`B3 legacy editor workflow path survived: ${marker}`);
  }
}

const editorEvalCount = (editor.match(/document::eval/g) || []).length;
if (editorEvalCount > 20) {
  throw new Error(`B3 editor component still owns too much browser choreography: document::eval=${editorEvalCount}`);
}

for (const marker of [
  'pub(crate) struct EditorWorkflowService',
  'stage_attachment_import(',
  'start_attachment_finalization(',
  'complete_attachment_finalization(',
  'media_compatibility(',
  'recover_pending_attachments(',
  'save_pdf_export(',
  'commit_image_edit(',
  'import_images(',
  'restore_revision(',
  'persist_revision_normalization(',
]) {
  if (!appWorkflows.includes(marker)) {
    throw new Error(`B3 application workflow capability is missing: ${marker}`);
  }
}
for (const forbidden of ['dioxus::', 'document::eval', 'rsx!', 'live_editor_']) {
  if (appWorkflows.includes(forbidden)) {
    throw new Error(`B3 application workflow depends on UI/browser transport: ${forbidden}`);
  }
}

for (const marker of [
  'run_attachment_import(',
  'run_media_compatibility(',
  'run_pending_attachment_recovery(',
  'run_pdf_export(',
  'run_image_edit(',
  'run_image_import(',
  'restore_revision_flow(',
]) {
  if (!uiWorkflows.includes(marker)) {
    throw new Error(`B3 bridge-facing workflow adapter is missing: ${marker}`);
  }
}
for (const forbidden of [
  'crate::storage',
  'crate::autosave',
  'crate::media_compat',
  'crate::media_server',
  'crate::native',
  'crate::platform',
  'ApplicationExecutor::read',
  'ApplicationExecutor::start_read',
  '.execute(ApplicationOperation',
]) {
  if (uiWorkflows.includes(forbidden)) {
    throw new Error(`B3 UI workflow adapter bypasses the application boundary: ${forbidden}`);
  }
}

const finalizerIndex = uiWorkflows.indexOf('start_attachment_finalization(');
let placeholderAwaitIndex = uiWorkflows.indexOf('publish_editor_bool(&publication_token, &update_script)');
if (placeholderAwaitIndex < 0) placeholderAwaitIndex = uiWorkflows.indexOf('document::eval(&update_script)');
if (finalizerIndex < 0 || placeholderAwaitIndex < 0 || finalizerIndex > placeholderAwaitIndex) {
  throw new Error('B3 attachment finalizer no longer starts before the next browser await');
}
const stateRefreshIndex = uiWorkflows.indexOf('onchanged.call(updated);', placeholderAwaitIndex);
const lookupWarningIndex = uiWorkflows.indexOf('embedding_lookup_error', stateRefreshIndex);
if (stateRefreshIndex < 0 || lookupWarningIndex < 0 || stateRefreshIndex > lookupWarningIndex) {
  throw new Error('B3 attachment state is not refreshed before embedding lookup warnings');
}

if (editor.includes('prepared[0].note_id') || uiWorkflows.includes('prepared[0].note_id')) {
  throw new Error('B3 still relies on the old prepared-import note_id leak');
}

for (const marker of [
  'pub(crate) enum EditorWorkflowError',
  'Attachment(AttachmentError)',
  'Editor(EditorError)',
  'Export(ExportError)',
  'Execution(ApplicationExecutionError)',
]) {
  if (!errors.includes(marker)) throw new Error(`B3 typed workflow error contract is missing: ${marker}`);
}
if (!uiWorkflows.includes('pub(super) enum EditorCoordinatorError')) {
  throw new Error('B3 bridge-facing coordinator error classification is missing');
}
if (/Result\s*<[^\n>]*,\s*String\s*>/.test(appWorkflows) || /Result\s*<[^\n>]*,\s*String\s*>/.test(uiWorkflows)) {
  throw new Error('B3 reintroduced Result<_, String> at the new workflow boundary');
}

for (const marker of [
  'pub(crate) mod editor_workflows;',
]) {
  if (!appMod.includes(marker)) throw new Error(`B3 application module wiring is missing: ${marker}`);
}
if (!uiMod.includes('mod editor_workflows;')) {
  throw new Error('B3 UI workflow module wiring is missing');
}
for (const marker of [
  'pub(crate) fn editor_workflows(&self, executor: ApplicationExecutor)',
  'EditorWorkflowService::new(',
]) {
  if (!context.includes(marker)) throw new Error(`B3 composition-root workflow wiring is missing: ${marker}`);
}

console.log(`B3 workflow checks passed. ui/editor.rs document::eval count: ${editorEvalCount}.`);
NODE

echo
echo "Checking first-party JavaScript and shell syntax..."
find editor-src -type f \( -name '*.js' -o -name '*.mjs' \) -not -path '*/vendor/*' -print0 \
  | xargs -0 -n1 node --check
find pdf-src -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
find scripts -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
find scripts -type f -name '*.sh' -print0 | xargs -0 -n1 zsh -n

echo
echo "Running cumulative B2/A0-A7/B1 architecture guards in static mode..."
RUSTNOTE_B2_STATIC_ONLY=1 ./scripts/validate-architecture-b2.sh

if [[ "${RUSTNOTE_B3_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture B3 static validation passed (prepared-runtime/Rust QA skipped by RUSTNOTE_B3_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete B2 acceptance suite..."
./scripts/validate-architecture-b2.sh

echo
echo "Rechecking B3 boundary after the cumulative build/QA chain..."
RUSTNOTE_B3_STATIC_ONLY=1 ./scripts/validate-architecture-b3.sh

echo
echo "Architecture B3 validation passed."

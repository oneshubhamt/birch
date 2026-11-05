#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A1 application-service boundaries..."
node <<'NODE'
const fs = require('node:fs');

function read(path) {
  if (!fs.existsSync(path)) throw new Error(`Required A1 source is missing: ${path}`);
  return fs.readFileSync(path, 'utf8');
}

const lib = read('src/lib.rs');
const appMod = read('src/application/mod.rs');
const attachmentService = read('src/application/attachments.rs');
const editorService = read('src/application/editor.rs');
const exportService = read('src/application/export.rs');
const editorUi = read('src/ui/editor.rs');
const syncUi = read('src/ui/synchronization.rs');
const backgroundUi = read('src/ui/background.rs');
const entityCommands = read('src/ui/entity_commands.rs');
const rightPanel = read('src/ui/right_panel.rs');
const applicationSources = [appMod, attachmentService, editorService, exportService].join('\n');

if (!lib.includes('mod application;')) {
  throw new Error('A1 application layer is not compiled into Rustnote');
}
for (const marker of ['pub(crate) mod attachments;', 'pub(crate) mod editor;', 'pub(crate) mod export;']) {
  if (!appMod.includes(marker)) throw new Error(`A1 application module is missing: ${marker}`);
}

for (const marker of [
  'struct AttachmentService',
  'prepare_import_batch',
  'finalize_import_batch',
  'recover_pending',
  'open_pending',
  'probe_media_compatibility',
  'prepare_media_compatibility',
  'invalidate_media_compatibility',
  'execute_file_command_by_id',
  'commit_image_edit',
  'import_images',
]) {
  if (!attachmentService.includes(marker)) throw new Error(`Attachment application workflow is missing: ${marker}`);
}
for (const marker of [
  'struct EditorService',
  'load_note',
  'synchronize_document',
  'restore_revision',
  'persist_runtime_normalization',
  'note_link_targets',
  'project_editor_document',
]) {
  if (!editorService.includes(marker)) throw new Error(`Editor application workflow is missing: ${marker}`);
}
for (const marker of ['struct ExportService', 'save_staged_pdf_export', 'PdfExportOutcome']) {
  if (!exportService.includes(marker)) throw new Error(`Export application workflow is missing: ${marker}`);
}

for (const forbidden of [
  'crate::native::',
  'crate::media_compat::',
  'prepare_attachments_blocking',
  'finalize_attachments_blocking',
  'commit_editor_image_edit_blocking',
  'import_editor_images_blocking',
  '.restore_note_revision(',
  '.reconcile_pending_attachment_ingests_for_note(',
  '.attachment_ingest_intents(',
]) {
  if (editorUi.includes(forbidden)) throw new Error(`Editor UI still owns A1 application logic: ${forbidden}`);
}
for (const forbidden of ['prepare_attachments_blocking', 'finalize_attachments_blocking']) {
  if (backgroundUi.includes(forbidden)) throw new Error(`Old attachment orchestration remains in UI background module: ${forbidden}`);
}
if (entityCommands.includes('AttachmentFileCommand') || entityCommands.includes('execute_attachment_file_command')) {
  throw new Error('Attachment file semantics must have one authoritative implementation in application::attachments');
}
if (!rightPanel.includes('attachment_service: AttachmentService') || !rightPanel.includes('.execute_file_command(')) {
  throw new Error('Attachments panel is not using the canonical AttachmentService file command');
}
for (const marker of ['.begin_note_flush(', '.synchronize_document(', '.checkpoint_note_silent(']) {
  if (!syncUi.includes(marker)) throw new Error(`Synchronization adapter is not delegating through EditorService: ${marker}`);
}
for (const forbidden of ['.get_note(', '.restore_note_revision(', '.save_note_owned_with_revision_status_scoped(']) {
  if (syncUi.includes(forbidden)) throw new Error(`Synchronization UI still owns persistence implementation: ${forbidden}`);
}
if (fs.existsSync('src/export.rs')) {
  throw new Error('Legacy root export service remains after migration to application::export');
}
for (const forbidden of ['crate::ui', 'super::ui', 'dioxus::']) {
  if (applicationSources.includes(forbidden)) {
    throw new Error(`Application layer depends back on UI/runtime presentation code: ${forbidden}`);
  }
}

console.log('A1 application-service boundary checks passed.');
NODE

if [[ "${RUSTNOTE_A1_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "A1 static validation passed (Rust tests skipped by RUSTNOTE_A1_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running the A0 behavioral safety net..."
./scripts/validate-architecture-a0.sh

echo
echo "Architecture A1 validation passed."

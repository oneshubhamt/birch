#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A1-A4 closure invariants..."
node <<'NODE'
const fs = require('node:fs');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required closure source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}

const paths = read('src/library_paths.rs');
const library = read('src/library.rs');
const capabilities = read('src/storage/capabilities.rs');
const attachments = read('src/storage/attachments.rs');
const nativeHandoff = read('src/native/handoff.rs');
const attachmentPaths = read('src/attachment_paths.rs');
const command = read('src/editor_bridge/command.rs');
const liveEditor = read('src/ui/live_editor.rs');
const sources = read('src/ui/sources.rs');
const noteList = read('src/ui/note_list_data.rs');
const synchronization = read('src/ui/synchronization.rs');
const editorService = read('src/application/editor.rs');

// A4 closure: one canonical construction site for library-derived paths.
for (const marker of [
  'const MEDIA_COMPAT_DIRECTORY_NAME: &str = "media-compat";',
  'ATTACHMENT_HANDOFF_DIRECTORY_NAME: &str = "attachment-handoff";',
  'IMAGE_RENDITION_RELATIVE_ROOT: &str = "cache/image-renditions/v2";',
  'pub(crate) fn media_compat_directory(data_directory: &Path)',
  'pub(crate) fn attachment_handoff_directory(data_directory: &Path)',
]) {
  if (!paths.includes(marker)) throw new Error(`Canonical library path contract is missing: ${marker}`);
}
for (const [file, source] of [
  ['src/library.rs', library],
  ['src/storage/capabilities.rs', capabilities],
  ['src/storage/attachments.rs', attachments],
]) {
  if (source.includes('.join("media-compat")') || source.includes('.join("attachment-handoff")')) {
    throw new Error(`Library-derived path is reconstructed outside library_paths.rs: ${file}`);
  }
}
if (nativeHandoff.includes('.join("attachment-handoff")')) {
  throw new Error('Native fallback handoff path bypasses the canonical handoff directory name');
}
if (attachmentPaths.includes('cache/image-renditions/v2')) {
  throw new Error('Attachment rendition paths duplicate the canonical image-rendition root');
}
if (!attachmentPaths.includes('crate::library_paths::IMAGE_RENDITION_RELATIVE_ROOT')) {
  throw new Error('Attachment rendition paths do not consume the canonical image-rendition root');
}

// A4 closure: narrow the highest-value UI storage consumers without repository sprawl.
for (const marker of [
  'pub(crate) struct LibraryStore',
  'pub(crate) struct NoteStore',
  'pub(crate) struct SettingsStore',
  'pub(crate) struct RuntimeStore',
  'pub(crate) struct AttachmentStore',
  'pub(crate) struct EditorStore',
  'pub(crate) struct ExportStore',
]) {
  if (!capabilities.includes(marker)) throw new Error(`Required storage capability is missing: ${marker}`);
}
if (!liveEditor.includes('AttachmentService') || liveEditor.includes('storage::')) {
  throw new Error('Live editor should depend on AttachmentService, not storage implementation types');
}
if (!sources.includes('NoteService') || sources.includes('storage::')) {
  throw new Error('Sources panel should depend on NoteService, not storage implementation types');
}
if (!noteList.includes('LibraryService') || !noteList.includes('EditorService') || noteList.includes('storage::')) {
  throw new Error('Note-list workflows should depend on application services, not storage implementation types');
}

// A2 closure: stable Rust->editor payload schemas are concrete DTOs.
const commandContract = command.slice(0, command.indexOf('pub fn serialize_editor_command'));
if (commandContract.includes('serde_json::Value')) {
  throw new Error('Stable EditorCommand/EditorInstallPayload fields still use serde_json::Value');
}
for (const marker of [
  'pub struct EditorAttachmentDescriptor',
  'pub struct EditorAttachmentImportRecovery',
  'pub struct EditorAttachmentImportItem',
  'pub struct EditorNoteLinkInsertDescriptor',
  'attachments: Vec<EditorAttachmentDescriptor>',
  'note_link_targets: Vec<NoteLinkTarget>',
]) {
  if (!command.includes(marker)) throw new Error(`Concrete editor bridge DTO is missing: ${marker}`);
}
for (const stale of ['serde_json::to_value(&runtime_entries)', 'serde_json::Value::Array(items)', 'serde_json::to_value(descriptor)']) {
  if (liveEditor.includes(stale)) throw new Error(`Live editor still dynamically shapes a stable command payload: ${stale}`);
}

// A1 closure: application owns checkpoint -> target-load sequence; UI owns safety checks.
for (const marker of [
  'pub(crate) fn switch_note(',
  'EditorPersistenceMode::Checkpoint',
  'let target_note = self.load_note(target_note_id);',
]) {
  if (!editorService.includes(marker)) throw new Error(`EditorService note-switch operation is incomplete: ${marker}`);
}
for (const marker of [
  'checkpoint_and_load_note_for_selection',
  'capture_live_editor_document',
  'verify_active_note(',
  'service.switch_note(',
]) {
  if (!synchronization.includes(marker)) throw new Error(`UI note-switch safety adapter is incomplete: ${marker}`);
}
if (!noteList.includes('checkpoint_and_load_note_for_selection(')) {
  throw new Error('Note selection still owns checkpoint/load orchestration instead of delegating it');
}
if (noteList.includes('synchronize_current_editor(') || noteList.includes('EditorService::new(')) {
  throw new Error('Legacy two-step note-switch orchestration survived in note_list_data.rs');
}

console.log('A1-A4 closure checks passed.');
NODE

if [[ "${RUSTNOTE_A14_CLOSURE_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "A1-A4 closure static validation passed."
  exit 0
fi

echo
echo "Running complete A4 acceptance suite..."
./scripts/validate-architecture-a4.sh

echo
echo "A1-A4 closure validation passed."

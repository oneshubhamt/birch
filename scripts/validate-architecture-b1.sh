#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking B1 application/storage boundary closure..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required B1 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}

function walkRust(root) {
  const result = [];
  for (const entry of fs.readdirSync(root, {withFileTypes: true})) {
    const file = path.join(root, entry.name).replaceAll('\\', '/');
    if (entry.isDirectory()) result.push(...walkRust(file));
    else if (entry.isFile() && file.endsWith('.rs')) result.push(file);
  }
  return result;
}

const uiFiles = walkRust('src/ui');
const ui = uiFiles.map(file => [file, read(file)]);
const capabilities = read('src/storage/capabilities.rs');
const storageMod = read('src/storage/mod.rs');
const context = read('src/application/context.rs');
const applicationFiles = walkRust('src/application');
const application = applicationFiles.map(file => [file, read(file)]);
const liveEditor = read('src/ui/live_editor.rs');
const startup = read('src/ui/startup.rs');
const persistence = read('src/application/persistence.rs');
const attachmentService = read('src/application/attachments.rs');
const settingsService = read('src/application/settings.rs');

for (const [file, source] of ui) {
  for (const forbidden of [
    'crate::storage',
    'storage::Storage',
    'crate::autosave',
    'AutosaveController',
    'crate::media_compat',
    'crate::media_server',
    'crate::image_rendition',
    'crate::native',
    'crate::platform',
    'rustnote_media_compat',
    'NoteSaveScope',
    'RevisionMode',
  ]) {
    if (source.includes(forbidden)) {
      throw new Error(`B1 UI implementation boundary leak in ${file}: ${forbidden}`);
    }
  }
}


if (persistence.includes('pub(crate) use crate::storage')) {
  throw new Error('B1 persistence intent is still a storage re-export');
}
for (const marker of [
  'pub(crate) enum NoteChangeScope',
  'Self::Title => NoteSaveScope::TITLE',
  'Self::Tags => NoteSaveScope::TAGS',
  'Self::State => NoteSaveScope::STATE',
  'Self::Document => NoteSaveScope::DOCUMENT',
]) {
  if (!persistence.includes(marker)) throw new Error(`B1 application persistence intent is missing: ${marker}`);
}
if (attachmentService.includes('pub(crate) use crate::storage::PreparedAttachmentImport')) {
  throw new Error('Prepared attachment import still leaks a storage DTO through the application boundary');
}
if (!attachmentService.includes('pub(crate) struct PreparedAttachmentImport') ||
    !attachmentService.includes('storage: crate::storage::PreparedAttachmentImport')) {
  throw new Error('Application-owned prepared attachment import wrapper is missing');
}
if (settingsService.includes('pub(crate) use crate::storage::{StorageLocations')) {
  throw new Error('StorageLocations still leaks directly through SettingsService');
}
if (!settingsService.includes('pub(crate) struct StorageLocations')) {
  throw new Error('Application-owned storage locations DTO is missing');
}

for (const marker of [
  'pub(crate) struct LibraryStore',
  'pub(crate) struct NoteStore',
  'pub(crate) struct SettingsStore',
  'pub(crate) struct RuntimeStore',
  'pub(crate) struct EditorStore',
  'pub(crate) struct AttachmentStore',
  'pub(crate) struct ExportStore',
  'pub(crate) fn library_store(&self)',
  'pub(crate) fn note_store(&self)',
  'pub(crate) fn settings_store(&self)',
  'pub(crate) fn runtime_store(&self)',
]) {
  if (!capabilities.includes(marker)) throw new Error(`B1 storage capability is missing: ${marker}`);
}
if (capabilities.includes('SourceStore') || capabilities.includes('source_store(&self)')) {
  throw new Error('Obsolete SourceStore survived after note/source capability consolidation');
}
for (const marker of [
  'LibraryStore', 'NoteStore', 'RuntimeStore', 'SettingsStore',
  'EditorStore', 'AttachmentStore', 'ExportStore',
]) {
  if (!storageMod.includes(marker)) throw new Error(`B1 storage capability export is missing: ${marker}`);
}

for (const [file, source] of application) {
  if (file === 'src/application/context.rs' || file.endsWith('/errors.rs')) continue;
  if (/\bstorage:\s*Storage\b/.test(source) || /fn new\(storage:\s*Storage\b/.test(source)) {
    throw new Error(`Application service still owns unrestricted Storage: ${file}`);
  }
  if (source.includes('dioxus::') || source.includes('document::eval') || source.includes('rsx!')) {
    throw new Error(`Application service depends on presentation/runtime UI code: ${file}`);
  }
}

for (const marker of [
  'LibraryService::new(storage.library_store())',
  'NoteService::new(storage.note_store())',
  'SettingsService::new(storage.settings_store())',
  'RuntimeService::new(storage.runtime_store()',
  'EditorService::new(storage.editor_store()',
  'AttachmentService::new(storage.attachment_store()',
  'ExportService::new(storage.export_store())',
]) {
  if (!context.includes(marker)) throw new Error(`Application composition root is missing: ${marker}`);
}

if (!startup.includes('ApplicationServices::open(libraries)')) {
  throw new Error('Startup does not enter through ApplicationServices');
}
if (!liveEditor.includes('attachment_service: AttachmentService')) {
  throw new Error('Live editor is not using AttachmentService for runtime attachment facts');
}
for (const forbidden of ['crate::media_compat', 'crate::media_server', 'crate::image_rendition', 'storage::']) {
  if (liveEditor.includes(forbidden)) {
    throw new Error(`Live editor still depends on attachment/media implementation detail: ${forbidden}`);
  }
}

const uiStorageMentions = ui.filter(([, source]) => source.includes('crate::storage') || source.includes('storage::Storage')).length;
const uiAutosaveMentions = ui.filter(([, source]) => source.includes('AutosaveController') || source.includes('crate::autosave')).length;
const uiMediaMentions = ui.filter(([, source]) => /crate::(?:media_compat|media_server|image_rendition|native|platform)/.test(source)).length;
if (uiStorageMentions !== 0 || uiAutosaveMentions !== 0 || uiMediaMentions !== 0) {
  throw new Error(`B1 UI dependency metrics are not closed: storage=${uiStorageMentions}, autosave=${uiAutosaveMentions}, media/native=${uiMediaMentions}`);
}

console.log(`B1 boundary checks passed across ${uiFiles.length} UI modules and ${applicationFiles.length} application modules.`);
NODE

find editor-src -type f \( -name '*.js' -o -name '*.mjs' \) -not -path '*/vendor/*' -print0 | xargs -0 -n1 node --check
find pdf-src -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
node --check scripts/build-prosemirror-editor.mjs
node --check scripts/build-pdfjs-runtime.mjs

if [[ "${RUSTNOTE_B1_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture B1 static validation passed (A0-A7/Rust QA skipped by RUSTNOTE_B1_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete A7 acceptance suite..."
./scripts/validate-architecture-a7.sh

echo
echo "Architecture B1 validation passed."

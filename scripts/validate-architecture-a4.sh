#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A4 library/storage boundaries..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required A4 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}

const library = read('src/library.rs');
const libraryPaths = read('src/library_paths.rs');
const storage = read('src/storage/mod.rs');
const capabilities = read('src/storage/capabilities.rs');
const transport = read('src/storage/transport.rs');
const protocol = read('src/attachment_protocol.rs');
const main = read('src/main.rs');
const startup = read('src/ui/startup.rs');
const applicationContext = read('src/application/context.rs');
const runtimeService = read('src/application/runtime.rs');
const mediaServer = read('src/media_server.rs');
const mediaCompat = read('src/media_compat.rs');
const nativeHandoff = read('src/native/handoff.rs');
const editorService = read('src/application/editor.rs');
const attachmentService = read('src/application/attachments.rs');
const exportService = read('src/application/export.rs');

for (const marker of [
  'pub struct LibraryContext',
  'pub struct LibraryRuntime',
  'inner: Arc<RwLock<Option<Arc<LibraryContext>>>>',
  'pub fn activate(&self, library: LibraryContext)',
  'pub fn current(&self) -> Option<Arc<LibraryContext>>',
  'pub fn transport_store(&self) -> AttachmentTransportStore',
]) {
  if (!library.includes(marker)) throw new Error(`A4 explicit library context is missing: ${marker}`);
}

for (const marker of [
  'pub struct LibraryPaths',
  'pub fn data_directory(&self)',
  'pub fn media_compat_directory(&self)',
  'pub fn attachment_handoff_directory(&self)',
  'pub fn image_rendition_directory(&self)',
]) {
  if (!libraryPaths.includes(marker)) throw new Error(`A4 canonical library path ownership is missing: ${marker}`);
}
if (!library.includes('pub use crate::library_paths::LibraryPaths;')) {
  throw new Error('LibraryContext no longer exposes the canonical LibraryPaths type');
}

for (const forbidden of [
  'ACTIVE_DATA_DIRECTORY',
  'Storage::active_data_directory',
  'Self::active_data_directory',
  'active_attachment_resource(',
]) {
  const files = walkRust('src');
  for (const file of files) {
    if (read(file).includes(forbidden)) {
      throw new Error(`A4 hidden active-library dependency survived in ${file}: ${forbidden}`);
    }
  }
}

for (const marker of [
  'mod capabilities;',
  'mod transport;',
  'pub(crate) use capabilities::{',
  'pub use transport::{',
]) {
  if (!storage.includes(marker)) throw new Error(`Storage module is missing A4 boundary: ${marker}`);
}
if (storage.includes('pub fn configure_connection(')) {
  throw new Error('SQLite connection configuration should no longer be part of Storage public API');
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
  'pub(crate) fn editor_store(&self)',
  'pub(crate) fn attachment_store(&self)',
  'pub(crate) fn export_store(&self)',
]) {
  if (!capabilities.includes(marker)) throw new Error(`A4 storage capability is missing: ${marker}`);
}
for (const [source, capability] of [
  [editorService, 'storage: EditorStore'],
  [attachmentService, 'storage: AttachmentStore'],
  [exportService, 'storage: ExportStore'],
]) {
  const productionSource = source.split('#[cfg(test)]')[0];
  if (!productionSource.includes(capability)) throw new Error(`Application service is not narrowed to ${capability}`);
  if (/\bstorage::Storage\b/.test(productionSource) || /\bStorage\s*[,}>]/.test(productionSource)) {
    throw new Error(`Application service still depends on unrestricted Storage instead of ${capability}`);
  }
}

for (const marker of [
  'pub struct AttachmentTransportStore',
  'pub struct AttachmentTransportSession',
  'pub struct AttachmentTransportResource',
  'a.sha256, a.file_name, a.mime_type',
  'SELECT annotations_json FROM pdf_annotation_pages',
  'INSERT INTO pdf_annotation_pages',
  'DELETE FROM pdf_annotation_pages',
  'pub fn pdf_annotation_rows(',
  'source_byte_limit: usize',
]) {
  if (!transport.includes(marker)) throw new Error(`A4 transport store is missing: ${marker}`);
}
for (const forbidden of ['rusqlite::', 'use rusqlite', 'Connection::open', '.query_row(', '.prepare(', 'SELECT a.sha256', 'SELECT annotations_json', 'INSERT INTO pdf_annotation_pages', 'DELETE FROM pdf_annotation_pages']) {
  if (protocol.includes(forbidden)) throw new Error(`Attachment protocol still owns SQLite persistence details: ${forbidden}`);
}

const rustFilesOutsideStorage = walkRust('src').filter(file => !file.startsWith('src/storage/'));
for (const file of rustFilesOutsideStorage) {
  const source = read(file);
  if (source.includes('use rusqlite') || source.includes('rusqlite::') || source.includes('Connection::open')) {
    throw new Error(`SQLite access escaped storage infrastructure: ${file}`);
  }
}

for (const marker of [
  'let library_runtime = rustnote::library::LibraryRuntime::new();',
  'rustnote::media_server::start(library_runtime.clone())',
  '.with_context(library_runtime)',
  'attachment_protocol::attachment_asset_response(',
  'library.as_deref()',
]) {
  if (!main.includes(marker)) throw new Error(`A4 main/runtime wiring is missing: ${marker}`);
}
for (const forbidden of [
  'media_compat::cleanup_stale_temporary_files()',
  'media_compat::prune_cache()',
  'native::cleanup_stale_attachment_handoffs()',
]) {
  if (main.includes(forbidden)) throw new Error(`Library-scoped startup work still runs before the library is known: ${forbidden}`);
}

for (const marker of [
  'let library = LibraryContext::open()',
  'libraries.activate(library)?;',
]) {
  if (!applicationContext.includes(marker)) throw new Error(`A4 application library wiring is missing: ${marker}`);
}
for (const marker of [
  'self.paths.media_compat_directory()',
  'self.paths.attachment_handoff_directory()',
]) {
  if (!runtimeService.includes(marker)) throw new Error(`A4 runtime library path wiring is missing: ${marker}`);
}
if (!startup.includes('use_context::<LibraryRuntime>()')) {
  throw new Error('A4 startup no longer receives LibraryRuntime');
}
for (const marker of [
  'pub fn start(libraries: LibraryRuntime)',
  'let Some(library) = libraries.current()',
  'let transport = library.transport_store();',
  'transport.attachment_resource(attachment_id)',
  'library.paths().media_compat_directory()',
]) {
  if (!mediaServer.includes(marker)) throw new Error(`Media server is not library-scoped: ${marker}`);
}
if (mediaServer.includes('Storage::') || mediaServer.includes('active_data_directory')) {
  throw new Error('Media server still discovers storage implicitly');
}
for (const marker of [
  'transport: &AttachmentTransportStore',
  'cache_directory: &Path',
  'cache_directory: cache_directory.to_path_buf()',
]) {
  if (!mediaCompat.includes(marker)) throw new Error(`Media compatibility is missing explicit library resources: ${marker}`);
}
if (mediaCompat.includes('Storage::active') || mediaCompat.includes('active_data_directory')) {
  throw new Error('Media compatibility still discovers the active library implicitly');
}
for (const marker of [
  'pub fn open_managed_file(',
  'handoff_root: &Path,',
  'pub fn reveal_managed_file(',
  'pub fn cleanup_stale_attachment_handoffs(handoff_root: &Path)',
]) {
  if (!nativeHandoff.includes(marker)) throw new Error(`Native handoff API is not explicitly library-scoped: ${marker}`);
}

if ((library.match(/^\s*#\[test\]/gm) || []).length < 2
    || (transport.match(/^\s*#\[test\]/gm) || []).length < 2) {
  throw new Error('A4 library/transport invariants are missing regression tests');
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

console.log('A4 library/storage boundary checks passed.');
NODE

if [[ "${RUSTNOTE_A4_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture A4 static validation passed (A3 acceptance/build/Rust tests skipped by RUSTNOTE_A4_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete A3 acceptance suite..."
./scripts/validate-architecture-a3.sh

echo
echo "Architecture A4 validation passed."

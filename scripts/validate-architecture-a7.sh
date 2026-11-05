#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A7 final architectural closure..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

const read = file => {
  if (!fs.existsSync(file)) throw new Error(`Required A7 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
};

function walk(root, suffix) {
  const out = [];
  for (const entry of fs.readdirSync(root, {withFileTypes: true})) {
    const file = path.join(root, entry.name).replaceAll('\\', '/');
    if (entry.isDirectory()) out.push(...walk(file, suffix));
    else if (entry.isFile() && (!suffix || file.endsWith(suffix))) out.push(file);
  }
  return out;
}

const rustFiles = walk('src', '.rs');
const rustSource = rustFiles.map(read).join('\n');
const nativeHandoff = read('src/native/handoff.rs');
const nativeHandoffProduction = nativeHandoff.split('#[cfg(test)]', 1)[0];
const libraryPaths = read('src/library_paths.rs');
const pdfViewer = read('pdf-src/viewer.mjs');
const pdfSearch = read('pdf-src/search.mjs');
const cargo = read('Cargo.toml');
const schema = read('src/storage/schema.rs');
const notesStorage = read('src/storage/notes.rs');
const rustToolchain = read('rust-toolchain.toml');


const componentsMatch = rustToolchain.match(/components\s*=\s*\[([\s\S]*?)\]/);
if (!componentsMatch) {
  throw new Error('rust-toolchain.toml does not declare required QA components');
}
const toolchainComponents = new Set(
  [...componentsMatch[1].matchAll(/["']([^"']+)["']/g)].map(match => match[1]),
);
for (const component of ['rustfmt', 'clippy']) {
  if (!toolchainComponents.has(component)) {
    throw new Error(`rust-toolchain.toml does not provision final A7 QA component: ${component}`);
  }
}

// Preserve the user's explicitly retained remote-sync scaffolding.
for (const marker of [
  'remote-sync = []',
]) {
  if (!cargo.includes(marker)) throw new Error(`Remote-sync Cargo scaffold was removed: ${marker}`);
}
for (const marker of [
  'CREATE TABLE sync_queue',
  'idx_sync_queue_state',
  'idx_sync_queue_pending_entity',
]) {
  if (!schema.includes(marker)) throw new Error(`Remote-sync schema scaffold was removed: ${marker}`);
}
for (const marker of [
  '#[cfg(feature = "remote-sync")]',
  'enqueue_sync_upsert',
  'enqueue_sync_delete',
  'pending_sync_count',
]) {
  if (!notesStorage.includes(marker)) throw new Error(`Remote-sync storage scaffold was removed: ${marker}`);
}

// A1-A6 migration remnants must not reappear.
for (const stale of [
  'BackgroundOperationCoordinator',
  'BackgroundOperationStatus',
  'run_read_blocking(',
  'begin_background_operation',
  'finish_background_operation',
  'ACTIVE_DATA_DIRECTORY',
  'Storage::active_data_directory',
  'active_attachment_resource(',
  'crate::models',
  '__rustnoteInstallEditor',
  '__rustnoteSyncEditor',
  '__rustnoteSetAttachmentRegistry',
  '__rustnoteCompleteImageEdit',
  '__rustnoteFormatEditor',
]) {
  if (rustSource.includes(stale)) throw new Error(`A1-A6 legacy architecture returned: ${stale}`);
}
if (fs.existsSync('src/models.rs')) throw new Error('Legacy models.rs monolith returned');
if (fs.existsSync('src/native.rs')) throw new Error('Legacy native.rs monolith returned');
if (fs.existsSync('assets/pdfjs/rustnote-pdf-viewer.mjs')) throw new Error('Legacy PDF viewer monolith returned');

// The old process-global temp handoff fallback is obsolete now that handoffs are library-scoped.
for (const stale of ['legacy_attachment_handoff_root', 'legacy attachment handoffs', 'std::env::temp_dir()']) {
  if (nativeHandoffProduction.includes(stale)) throw new Error(`Obsolete attachment-handoff compatibility path survived: ${stale}`);
}
for (const marker of [
  'cleanup_session_directories(',
  'process_session_owner(handoff_root, "attachment handoffs")',
]) {
  if (!nativeHandoffProduction.includes(marker)) {
    throw new Error(`Library-scoped handoff ownership is no longer authoritative: ${marker}`);
  }
}
if (libraryPaths.includes('pub(crate) const ATTACHMENT_HANDOFF_DIRECTORY_NAME')) {
  throw new Error('Attachment handoff directory-name implementation detail is still exposed outside library_paths');
}

// Confirmed dead APIs removed in A7 must stay removed.
const deadApiPatterns = [
  /pub fn plain_text_from_markdown\s*\(/,
  /pub fn replace_field_filter\s*\(/,
  /pub fn contextual_preview\s*\(/,
  /pub fn first_filter_value\s*\(/,
  /pub fn matches_note\s*\(/,
  /pub fn matches_summary\s*\(/,
  /pub fn flush_live_note\s*\(/,
  /pub fn flush_note_silent\s*\(/,
  /pub fn checkpoint_note\s*\(/,
  /pub fn list_revisions\s*\(/,
  /pub fn note_from_revision\s*\(/,
  /pub fn prune_backup_repository\s*\(/,
];
for (const pattern of deadApiPatterns) {
  if (pattern.test(rustSource)) throw new Error(`Confirmed dead API survived A7: ${pattern}`);
}

// A6 split regression: a helper referenced by viewer.mjs must be explicitly imported.
if (!pdfSearch.includes('export function appendSearchFragment')) {
  throw new Error('PDF search helper export is missing');
}
if (!pdfViewer.includes("import {appendSearchFragment, searchRangeForOffsets, textLayerSearchMap} from './search.mjs';")) {
  throw new Error('PDF viewer does not import appendSearchFragment from the split search module');
}
if (!pdfViewer.includes('appendSearchFragment(state, item.str);')) {
  throw new Error('PDF incremental search-index path unexpectedly changed');
}

// Guard named helper imports for the split PDF viewer. Config is intentionally namespace-imported.
const helperFiles = ['annotations.mjs', 'rendering.mjs', 'search.mjs', 'transport.mjs', 'ui.mjs', 'virtualization.mjs'];
for (const helper of helperFiles) {
  const source = read(`pdf-src/${helper}`);
  const exported = [
    ...source.matchAll(/export\s+(?:async\s+)?function\s+([A-Za-z_$][\w$]*)/g),
    ...source.matchAll(/export\s+class\s+([A-Za-z_$][\w$]*)/g),
    ...source.matchAll(/export\s+const\s+([A-Za-z_$][\w$]*)/g),
  ].map(match => match[1]);
  const importMatch = pdfViewer.match(new RegExp(`import\\s*\\{([^}]+)\\}\\s*from\\s*['"]\\./${helper.replace('.', '\\.') }['"]`, 's'));
  const imported = new Set(importMatch ? importMatch[1].split(',').map(value => value.trim().split(/\s+as\s+/).pop()).filter(Boolean) : []);
  for (const name of exported) {
    const referenced = new RegExp(`\\b${name}\\b`).test(pdfViewer);
    if (referenced && !imported.has(name)) {
      throw new Error(`PDF viewer references ${name} from ${helper} without importing it`);
    }
  }
}

// Database authority and explicit library ownership remain closed.
for (const file of rustFiles.filter(file => !file.startsWith('src/storage/'))) {
  if (read(file).includes('rusqlite::')) throw new Error(`Direct SQLite authority escaped storage: ${file}`);
}

const testCount = [...rustFiles, ...walk('crates', '.rs')]
  .reduce((count, file) => count + (read(file).match(/^\s*#\[test\]/gm) || []).length, 0);
if (testCount < 59) throw new Error(`A7 lost regression coverage: expected at least 59 Rust tests, found ${testCount}`);

console.log(`A7 closure checks passed. Rust regression tests discovered: ${testCount}`);
NODE

find editor-src -type f \( -name '*.js' -o -name '*.mjs' \) -not -path '*/vendor/*' -print0 | xargs -0 -n1 node --check
find pdf-src -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
node --check scripts/build-prosemirror-editor.mjs
node --check scripts/build-pdfjs-runtime.mjs

node --input-type=module <<'NODE'
import {appendSearchFragment} from './pdf-src/search.mjs';
const state = {text: '', segments: []};
appendSearchFragment(state, 'Rustnote');
appendSearchFragment(state, 'search');
if (state.text !== 'Rustnote search' || state.segments.length !== 0) {
  throw new Error('PDF search helper smoke test failed');
}
console.log('PDF split search helper smoke test passed.');
NODE

if [[ "${RUSTNOTE_A7_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture A7 static validation passed (full build/Rust QA skipped by RUSTNOTE_A7_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete A6 acceptance suite..."
./scripts/validate-architecture-a6.sh

echo
echo "Running final formatting check..."
cargo fmt --all -- --check

echo
echo "Running final strict Clippy across all targets/features..."
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

echo
echo "Running final all-feature workspace tests..."
cargo test --workspace --all-features --locked

echo
echo "Architecture A7 validation passed."

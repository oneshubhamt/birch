#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking B4 behavioral test architecture and B3 close-out..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required B4 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}

const editor = read('src/ui/editor.rs');
const uiWorkflows = read('src/ui/editor_workflows.rs');
const appWorkflows = read('src/application/editor_workflows.rs');
const runner = read('scripts/run-browser-behavior-tests.mjs');
const editorBehavior = read('tests/js/editor-prosemirror-behavior.test.mjs');
const bridgeBehavior = read('tests/js/editor-bridge-behavior.test.mjs');
const noteListResilience = read('tests/js/note-list-resilience-contract.test.mjs');
const pdfBehavior = read('tests/js/pdf-behavior.test.mjs');
const pdfRuntimeIntegrity = read('tests/js/pdf-runtime-integrity.test.mjs');

for (const forbidden of [
  'AttachmentService::discard_image_edit_stage',
  'AttachmentService::discard_image_import_stage',
  'live_editor_fail_image_edit_script',
  'live_editor_fail_image_import_script',
]) {
  if (editor.includes(forbidden)) {
    throw new Error(`B4 residual image rejection/cleanup choreography survived in ui/editor.rs: ${forbidden}`);
  }
}
for (const marker of [
  'pub(super) fn reject_image_edit(',
  'pub(super) fn reject_image_import(',
]) {
  if (!uiWorkflows.includes(marker)) throw new Error(`B4 UI rejection helper is missing: ${marker}`);
}
for (const marker of [
  'pub(crate) fn discard_image_edit_stage(&self',
  'pub(crate) fn discard_image_import_stages',
  'resolve_attachment_import_completion(',
]) {
  if (!appWorkflows.includes(marker)) throw new Error(`B4 workflow behavior seam is missing: ${marker}`);
}
if (appWorkflows.includes('document::eval') || appWorkflows.includes('dioxus::')) {
  throw new Error('B4 moved browser behavior into application/editor_workflows.rs');
}

for (const [file, source] of [
  ['tests/js/editor-prosemirror-behavior.test.mjs', editorBehavior],
  ['tests/js/editor-bridge-behavior.test.mjs', bridgeBehavior],
  ['tests/js/pdf-behavior.test.mjs', pdfBehavior],
  ['tests/js/pdf-runtime-integrity.test.mjs', pdfRuntimeIntegrity],
]) {
  if (!source.includes("from 'node:test'")) throw new Error(`B4 behavioral test does not use Node's built-in test runner: ${file}`);
}
for (const marker of [
  'normalizeAtomicBoundaryDocument',
  'commandForBlock',
  'commandForList',
  'insertHorizontalRule',
  'insertTable',
  'buildFindDocumentIndex',
  'findMatches',
]) {
  if (!editorBehavior.includes(marker)) throw new Error(`B4 ProseMirror behavior coverage is missing: ${marker}`);
}
for (const marker of ['emitEditorEvent', 'attachmentStatus']) {
  if (!bridgeBehavior.includes(marker)) throw new Error(`B4 bridge behavior coverage is missing: ${marker}`);
}
for (const marker of [
  'FenwickTree',
  'textLayerSearchMap',
  'searchRangeForOffsets',
  'portableAnnotationValue',
  'exportVectorPoints',
  'ENABLE_PDF_SCRIPTING',
]) {
  if (!pdfBehavior.includes(marker)) throw new Error(`B4 PDF behavior coverage is missing: ${marker}`);
}
if (!runner.includes('pdf-runtime-integrity.test.mjs')) {
  throw new Error('B4 browser behavior runner does not include PDF runtime-integrity tests');
}
if (!runner.includes("node_modules") || !runner.includes("--require-editor-deps") || !runner.includes("'--test'")) {
  throw new Error('B4 browser behavior runner lost its dependency/full-acceptance contract');
}

const rustBehaviorTests = [
  'attachment_completion_preserves_refreshed_state_when_lookup_fails',
  'media_compatibility_preserves_action_specific_failure_status',
  'attachment_finalization_runs_independently_before_completion_is_awaited',
];
for (const testName of rustBehaviorTests) {
  if (!appWorkflows.includes(`fn ${testName}()`)) throw new Error(`B4 workflow behavior test is missing: ${testName}`);
}

const jsTestCount = [editorBehavior, bridgeBehavior, noteListResilience, pdfBehavior]
  .reduce((count, source) => count + (source.match(/\btest\s*\(/g) || []).length, 0);
if (jsTestCount < 12) throw new Error(`B4 JavaScript behavioral coverage regressed: expected at least 12 tests, found ${jsTestCount}`);

function walk(root, suffix) {
  const out = [];
  for (const entry of fs.readdirSync(root, {withFileTypes: true})) {
    const file = path.join(root, entry.name).replaceAll('\\', '/');
    if (entry.isDirectory()) out.push(...walk(file, suffix));
    else if (entry.isFile() && file.endsWith(suffix)) out.push(file);
  }
  return out;
}
const rustFiles = [...walk('src', '.rs'), ...walk('crates', '.rs')];
const rustTestCount = rustFiles.reduce((count, file) => count + (read(file).match(/^\s*#\[test\]/gm) || []).length, 0);
if (rustTestCount < 64) throw new Error(`B4 Rust behavioral coverage regressed: expected at least 64 tests, found ${rustTestCount}`);

console.log(`B4 behavior guards passed. Rust tests discovered: ${rustTestCount}; JS behavior tests: ${jsTestCount}.`);
NODE

echo
echo "Checking B4 JavaScript test syntax..."
find tests/js -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
node --check scripts/run-browser-behavior-tests.mjs

echo
echo "Running dependency-free browser behavior tests available in this source tree..."
node scripts/run-browser-behavior-tests.mjs

echo
echo "Running cumulative B3 architecture guards in static mode..."
RUSTNOTE_B3_STATIC_ONLY=1 ./scripts/validate-architecture-b3.sh

if [[ "${RUSTNOTE_B4_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture B4 static validation passed (full ProseMirror/Rust/prepared-runtime QA skipped by RUSTNOTE_B4_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete B3 acceptance suite..."
./scripts/validate-architecture-b3.sh

echo
echo "Running complete editor/PDF behavioral JavaScript suite..."
node scripts/run-browser-behavior-tests.mjs --require-editor-deps

echo
echo "Rechecking B4 boundary after cumulative build/QA..."
RUSTNOTE_B4_STATIC_ONLY=1 ./scripts/validate-architecture-b4.sh

echo
echo "Architecture B4 validation passed."

#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking B5 cohesive PDF decomposition and typed PDF export errors..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required B5 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}

const viewer = read('pdf-src/viewer.mjs');
const previewCache = read('pdf-src/preview_cache.mjs');
const sidebar = read('pdf-src/sidebar.mjs');
const scheduler = read('pdf-src/render_scheduler.mjs');
const pdfBehavior = read('tests/js/pdf-behavior.test.mjs');
const pdfRuntimeIntegrity = read('tests/js/pdf-runtime-integrity.test.mjs');
const pdfLayoutIntegrity = read('tests/js/pdf-layout-integrity.test.mjs');
const nativeStaging = read('src/native/staging.rs');
const applicationErrors = read('src/application/errors.rs');
const applicationExport = read('src/application/export.rs');
const pdfReadme = read('pdf-src/README.md');
const preparedRuntimeValidator = read('scripts/validate-prepared-runtime.sh');

for (const [file, markers] of Object.entries({
  'pdf-src/preview_cache.mjs': ['PreviewRasterCache', 'updateAdaptiveBudget', 'evictToBudget', 'canvasBackingBytes'],
  'pdf-src/sidebar.mjs': ['thumbnailWindowBounds', 'annotationWindowBounds', 'annotationSummary', 'renderOutlineItems', 'createThumbnailRowElement', 'createAnnotationRowElement'],
  'pdf-src/render_scheduler.mjs': ['readingWindow', 'renderTargetPageNumbers', 'pageRenderPriority', 'nextRenderAction'],
})) {
  const source = read(file);
  for (const marker of markers) {
    if (!source.includes(marker)) throw new Error(`B5 PDF responsibility is missing from ${file}: ${marker}`);
  }
}

for (const importContract of [
  "from './preview_cache.mjs'",
  "from './sidebar.mjs'",
  "from './render_scheduler.mjs'",
]) {
  if (!viewer.includes(importContract)) throw new Error(`B5 viewer import is missing: ${importContract}`);
}
for (const forbidden of [
  'const previewCache = new Map()',
  'updateAdaptivePreviewBudget',
  'function medianPositive(values)',
  'function memoryOperatingBudget(',
  'function annotationKindLabel(',
  'function renderOutlineItems(items)',
  'function thumbnailPriority(',
]) {
  if (viewer.includes(forbidden)) throw new Error(`B5 extracted PDF responsibility leaked back into viewer.mjs: ${forbidden}`);
}
if (!viewer.includes('const action = nextRenderAction({')) {
  throw new Error('B5 viewer no longer delegates raster-lane ordering to render_scheduler.mjs');
}
for (const marker of [
  'viewer_source_inputs',
  'viewer_source_sha256',
  'Prepared PDF modular-source checksum map mismatch',
]) {
  if (!preparedRuntimeValidator.includes(marker)) {
    throw new Error(`Prepared-runtime validation is missing B5 artifact-integrity contract: ${marker}`);
  }
}
for (const forbidden of [
  'visibleDetailCandidate({criticalOnly',
  'takeNextReadingPageRender()',
  'Full PDF page renders must seed the shared Preview Cache',
  'sourceSection(',
]) {
  if (preparedRuntimeValidator.includes(forbidden)) {
    throw new Error(`Prepared-runtime validation must not duplicate B5 source behavior: ${forbidden}`);
  }
}
if (!viewer.includes('const previewCache = new PreviewRasterCache({')) {
  throw new Error('B5 viewer no longer delegates preview cache ownership');
}
const viewerLines = viewer.split(/\r?\n/).length;
if (viewerLines > 4250) {
  throw new Error(`B5 PDF controller regressed past the bounded decomposition target: ${viewerLines} lines`);
}

const pdfFiles = fs.readdirSync('pdf-src').filter(name => name.endsWith('.mjs')).sort();
const graph = new Map(pdfFiles.map(file => [file, []]));
for (const file of pdfFiles) {
  const source = read(`pdf-src/${file}`);
  for (const match of source.matchAll(/from\s+['"]\.\/([^'"]+)['"]/g)) {
    if (!graph.has(match[1])) throw new Error(`B5 PDF module import target is missing: ${file} -> ${match[1]}`);
    graph.get(file).push(match[1]);
  }
}
const visiting = new Set();
const visited = new Set();
function visit(file) {
  if (visiting.has(file)) throw new Error(`B5 PDF module cycle detected at ${file}`);
  if (visited.has(file)) return;
  visiting.add(file);
  for (const target of graph.get(file)) visit(target);
  visiting.delete(file);
  visited.add(file);
}
for (const file of pdfFiles) visit(file);

for (const marker of [
  'preview raster cache owns LRU accounting',
  'sidebar virtualization and annotation navigation helpers',
  'render target planning respects reading direction',
  'render scheduler evaluates the single raster lane',
]) {
  if (!pdfBehavior.includes(marker)) throw new Error(`B5 PDF behavioral coverage is missing: ${marker}`);
}
const pdfTestCount = (pdfBehavior.match(/\btest\s*\(/g) || []).length;
if (pdfTestCount < 8) throw new Error(`B5 PDF behavior coverage regressed: expected at least 8 tests, found ${pdfTestCount}`);
const pdfRuntimeIntegrityTestCount = (pdfRuntimeIntegrity.match(/\btest\s*\(/g) || []).length;
for (const marker of [
  'PDF at or below whole-file threshold uses the one-shot transport path',
  'PDF above whole-file threshold uses range transport with one bounded initial block',
  'transport module owns initial acquisition',
  'initial render memory accounting continues through PreviewRasterCache',
  'Previous Next and direct page navigation',
  'viewer source has no known dangling viewer-owned PDF identifiers',
  'preview close releases range/session state',
  'preview failures preserve password document transport and viewer categories',
]) {
  if (!pdfRuntimeIntegrity.includes(marker)) throw new Error(`PDF runtime-integrity coverage is missing: ${marker}`);
}
if (pdfRuntimeIntegrityTestCount < 8) {
  throw new Error(`PDF runtime-integrity coverage regressed: expected at least 8 tests, found ${pdfRuntimeIntegrityTestCount}`);
}
const pdfLayoutIntegrityTestCount = (pdfLayoutIntegrity.match(/\btest\s*\(/g) || []).length;
for (const marker of [
  'primary PDF toolbar remains sticky in base and narrow embedded layouts',
  'markup toolbar remains sticky below measured primary toolbar height',
  'sidebar remains an in-layout sticky column rather than a narrow-width overlay',
  'multi-page documents open with thumbnails available while one-page documents stay compact',
  'viewer chrome geometry has one measured authority shared by markup and sidebar offsets',
  'search markup resize and focus transitions resynchronize viewer chrome',
  'narrow embedded layout compresses horizontally without dismantling reader chrome',
  'thumbnail contents and annotations views remain present in the PDF sidebar contract',
]) {
  if (!pdfLayoutIntegrity.includes(marker)) throw new Error(`PDF layout-integrity coverage is missing: ${marker}`);
}
if (pdfLayoutIntegrityTestCount < 8) {
  throw new Error(`PDF layout-integrity coverage regressed: expected at least 8 tests, found ${pdfLayoutIntegrityTestCount}`);
}

for (const marker of [
  'pub enum PdfExportStagingError',
  'Result<String, PdfExportStagingError>',
  'Result<(), PdfExportStagingError>',
  'pdf_export_staging_errors_preserve_categories_and_messages',
]) {
  if (!nativeStaging.includes(marker)) throw new Error(`B5 typed PDF staging boundary is missing: ${marker}`);
}
if (!applicationErrors.includes('PdfStaging(PdfExportStagingError)')) {
  throw new Error('B5 ExportError does not preserve the typed PDF staging error');
}
if (applicationErrors.includes('pub(crate) enum ExportError {\n    Storage(StorageError),\n    AttachmentNotFound,\n    Native(String),')) {
  throw new Error('B5 left the PDF export boundary flattened into ExportError::Native(String)');
}
if (!applicationExport.includes('.map_err(ExportError::from)')) {
  throw new Error('B5 application export path does not preserve the typed native staging error');
}
for (const marker of ['errors.mjs', 'preview_cache.mjs', 'render_scheduler.mjs', 'sidebar.mjs', 'transport_policy.mjs']) {
  if (!pdfReadme.includes(marker)) throw new Error(`B5 PDF source documentation is missing ${marker}`);
}

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
if (rustTestCount < 65) throw new Error(`B5 Rust regression coverage regressed: expected at least 65 tests, found ${rustTestCount}`);

console.log(`B5 guards passed. PDF modules: ${pdfFiles.length}; viewer controller: ${viewerLines} lines; PDF behavior tests: ${pdfTestCount}; runtime-integrity tests: ${pdfRuntimeIntegrityTestCount}; layout-integrity tests: ${pdfLayoutIntegrityTestCount}; Rust tests: ${rustTestCount}.`);
NODE

echo
echo "Checking B5 JavaScript and shell syntax..."
find pdf-src tests/js -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
node --check scripts/run-browser-behavior-tests.mjs
zsh -n scripts/validate-architecture-b5.sh

echo
echo "Running B5 dependency-free PDF behavioral tests..."
node --test tests/js/pdf-behavior.test.mjs tests/js/pdf-runtime-integrity.test.mjs tests/js/pdf-layout-integrity.test.mjs

echo
echo "Running cumulative B4 architecture guards in static mode..."
RUSTNOTE_B4_STATIC_ONLY=1 ./scripts/validate-architecture-b4.sh

if [[ "${RUSTNOTE_B5_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture B5 static validation passed (full prepared-runtime/Rust/editor QA skipped by RUSTNOTE_B5_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete B4 acceptance suite..."
./scripts/validate-architecture-b4.sh

echo
echo "Rechecking B5 after cumulative build/QA..."
RUSTNOTE_B5_STATIC_ONLY=1 ./scripts/validate-architecture-b5.sh

echo
echo "Architecture B5 validation passed."

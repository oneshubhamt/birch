#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A6 structural decomposition..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

const read = file => {
  if (!fs.existsSync(file)) throw new Error(`Required A6 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
};

const lib = read('src/lib.rs');
const domainFiles = [
  'attachment.rs', 'backup.rs', 'note.rs', 'notebook.rs',
  'query.rs', 'revision.rs', 'source.rs',
];
if (fs.existsSync('src/models.rs') || lib.includes('pub mod models;') || !lib.includes('pub mod domain;')) {
  throw new Error('A6 domain split retained the models.rs monolith or failed to register domain/');
}
for (const file of domainFiles) read(`src/domain/${file}`);
const domainSource = domainFiles.map(file => read(`src/domain/${file}`)).join('\n');
for (const marker of [
  'pub struct NoteId', 'pub struct Note', 'pub struct NoteSummary',
  'pub struct Notebook', 'pub struct Attachment', 'pub struct Source',
  'pub struct NoteRevision', 'pub struct ArchiveManifest', 'pub struct NoteListQuery',
  'pub struct NoteIdError', 'pub struct SourceIdError',
]) {
  if (!domainSource.includes(marker)) throw new Error(`A6 domain model is missing: ${marker}`);
}
for (const file of walk('src', '.rs')) {
  const source = read(file);
  if (source.includes('crate::models') || /\bmodels::/.test(source)) {
    throw new Error(`Legacy models namespace survived A6 in ${file}`);
  }
}
const domainTypeCount = (domainSource.match(/^pub\s+(?:struct|enum|type)\s+[A-Za-z_]\w*/gm) || []).length;
if (domainTypeCount < 41) throw new Error(`A6 domain declarations regressed below the A5 baseline: ${domainTypeCount}`);

if (fs.existsSync('src/native.rs')) throw new Error('A6 retained the broad src/native.rs monolith');
const nativeFiles = ['mod.rs', 'handoff.rs', 'output.rs', 'rendition.rs', 'staging.rs'];
const nativeSources = Object.fromEntries(nativeFiles.map(file => [file, read(`src/native/${file}`)]));
for (const marker of ['open_managed_file', 'reveal_managed_file', 'save_managed_file', 'cleanup_stale_attachment_handoffs']) {
  if (!nativeSources['handoff.rs'].includes(`fn ${marker}`)) throw new Error(`Native handoff responsibility is misplaced/missing: ${marker}`);
}
for (const marker of ['stage_image_edit', 'claim_staged_image_import', 'stage_pdf_export', 'save_staged_pdf_export', 'cleanup_stale_pdf_exports']) {
  if (!nativeSources['staging.rs'].includes(`fn ${marker}`)) throw new Error(`Native staging responsibility is misplaced/missing: ${marker}`);
}
for (const marker of ['create_compatible_image_rendition', 'create_screen_image_rendition']) {
  if (!nativeSources['rendition.rs'].includes(`fn ${marker}`)) throw new Error(`Native rendition responsibility is misplaced/missing: ${marker}`);
}
for (const marker of ['recover_stale_output_transactions']) {
  if (!nativeSources['output.rs'].includes(`fn ${marker}`)) throw new Error(`Native output-ownership responsibility is misplaced/missing: ${marker}`);
}
const publicNativeFunctions = nativeFiles.reduce((count, file) => count + (nativeSources[file].match(/^pub\s+fn\s+[A-Za-z_]\w*/gm) || []).length, 0);
if (publicNativeFunctions !== 22) throw new Error(`Expected the current 22 native operations after output-ownership consolidation, found ${publicNativeFunctions}`);
for (const file of walk('src', '.rs')) {
  const source = read(file);
  if (/(?:crate|rustnote)::native::[A-Za-z_]\w*\s*\(/.test(source)) {
    throw new Error(`A6 caller bypasses explicit native responsibility module: ${file}`);
  }
}

if (fs.existsSync('assets/pdfjs/rustnote-pdf-viewer.mjs')) {
  throw new Error('A6 retained the 5k-line PDF viewer monolith under assets/');
}
const pdfFiles = fs.readdirSync('pdf-src').filter(name => name.endsWith('.mjs')).sort();
for (const required of ['annotations.mjs','config.mjs','errors.mjs','rendering.mjs','search.mjs','transport.mjs','transport_policy.mjs','ui.mjs','viewer.mjs','virtualization.mjs']) {
  if (!pdfFiles.includes(required)) throw new Error(`A6 PDF source module is missing: ${required}`);
}
const pdfSources = Object.fromEntries(pdfFiles.map(file => [file, read(`pdf-src/${file}`)]));
const viewerLines = pdfSources['viewer.mjs'].split(/\r?\n/).length;
if (viewerLines > 4700) throw new Error(`A6 PDF controller regressed toward the old monolith: ${viewerLines} lines`);
for (const [file, markers] of Object.entries({
  'transport.mjs': ['RustnotePdfRangeTransport', 'preparePdfDocumentTransport', 'fetchPdfWholeDocument', 'fetchPdfBlock'],
  'transport_policy.mjs': ['pdfTransportPlan'],
  'errors.mjs': ['classifyPdfPreviewError'],
  'rendering.mjs': ['RustnotePdfPageView', 'PDFJS_TEXT_LAYER_DISABLED'],
  'virtualization.mjs': ['FenwickTree'],
  'annotations.mjs': ['portableAnnotationValue', 'annotationPdfRect'],
  'search.mjs': ['textLayerSearchMap', 'searchRangeForOffsets'],
  'ui.mjs': ['elements', 'setStatus'],
})) {
  for (const marker of markers) if (!pdfSources[file].includes(marker)) throw new Error(`A6 PDF responsibility missing from ${file}: ${marker}`);
}

const localGraph = new Map(pdfFiles.map(file => [file, []]));
for (const file of pdfFiles) {
  for (const match of pdfSources[file].matchAll(/from\s+['"]\.\/([^'"]+)['"]/g)) {
    const target = match[1];
    if (!localGraph.has(target)) throw new Error(`PDF source imports missing local module ${target} from ${file}`);
    localGraph.get(file).push(target);
  }
}
const visiting = new Set();
const visited = new Set();
function visit(file) {
  if (visiting.has(file)) throw new Error(`PDF source module cycle detected at ${file}`);
  if (visited.has(file)) return;
  visiting.add(file);
  for (const target of localGraph.get(file)) visit(target);
  visiting.delete(file);
  visited.add(file);
}
for (const file of pdfFiles) visit(file);

const pdfBuild = read('scripts/build-pdfjs-runtime.mjs');
for (const marker of [
  "const viewerSourceDirectory = path.join(root, 'pdf-src');",
  "const viewerEntry = path.join(viewerSourceDirectory, 'viewer.mjs');",
  'viewer_source_inputs:',
  "filter(name => name.endsWith('.mjs'))",
  'await bundle(viewerEntry, viewerOutput, [pdfjsResolvers]);',
]) {
  if (!pdfBuild.includes(marker)) throw new Error(`PDF build pipeline is missing A6 modular-source contract: ${marker}`);
}
const preparedValidator = read('scripts/validate-prepared-runtime.sh');
for (const marker of ['pdfSourceFiles', 'viewer_source_inputs', 'Prepared PDF modular-source checksum map mismatch', 'viewer_source_sha256']) {
  if (!preparedValidator.includes(marker)) throw new Error(`Prepared runtime validation is missing A6 PDF source-integrity contract: ${marker}`);
}
if (preparedValidator.includes('visibleDetailCandidate({criticalOnly')
    || preparedValidator.includes('Full PDF page renders must seed the shared Preview Cache')) {
  throw new Error('Prepared-runtime validation must not duplicate PDF source-behavior assertions after A6/B5 decomposition');
}

console.log(`A6 decomposition checks passed: ${domainTypeCount} domain types, ${publicNativeFunctions} native operations, ${pdfFiles.length} PDF modules, ${viewerLines} viewer-controller lines.`);

function walk(root, suffix) {
  const result = [];
  for (const entry of fs.readdirSync(root, {withFileTypes: true})) {
    const file = path.join(root, entry.name).replaceAll('\\', '/');
    if (entry.isDirectory()) result.push(...walk(file, suffix));
    else if (entry.isFile() && file.endsWith(suffix)) result.push(file);
  }
  return result;
}
NODE

find pdf-src -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
node --check scripts/build-pdfjs-runtime.mjs

if [[ "${RUSTNOTE_A6_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture A6 static validation passed (A5 acceptance/build/Rust tests skipped by RUSTNOTE_A6_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete A5 acceptance suite..."
./scripts/validate-architecture-a5.sh

echo
echo "Validating the prepared modular PDF runtime..."
./scripts/validate-prepared-runtime.sh

echo
echo "Architecture A6 validation passed."

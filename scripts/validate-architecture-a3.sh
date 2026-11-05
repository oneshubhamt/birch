#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A3 modular ProseMirror source..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

const sourceInputs = [
  'editor-src/prosemirror-editor.js',
  'editor-src/config.js',
  'editor-src/ui/menu.js',
  'editor-src/bridge/events.js',
  'editor-src/runtime/state.js',
  'editor-src/attachments/import.js',
  'editor-src/document/spec.js',
  'editor-src/document/validation.js',
  'editor-src/document/schema.js',
  'editor-src/editor/clipboard.js',
  'editor-src/editor/core.js',
  'editor-src/editor/interactions.js',
  'editor-src/editor/list_system.js',
  'editor-src/commands/index.js',
  'editor-src/attachments/view.js',
  'editor-src/runtime/controller.js',
  'editor-src/bridge/host.js',
];

const read = file => {
  if (!fs.existsSync(file)) throw new Error(`Required A3 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
};
const sources = Object.fromEntries(sourceInputs.map(file => [file, read(file)]));
const entry = sources['editor-src/prosemirror-editor.js'];
const nonBlankEntryLines = entry.split(/\r?\n/).filter(line => line.trim()).length;
if (nonBlankEntryLines > 3 || !entry.includes("import './bridge/host.js';")) {
  throw new Error('A3 entry must remain a tiny side-effect import of bridge/host.js');
}
for (const forbidden of ['class Controller', 'class AttachmentView', 'function dispatchEditorCommand', 'const nodes =']) {
  if (entry.includes(forbidden)) throw new Error(`Runtime implementation leaked back into the A3 entry: ${forbidden}`);
}

const ownership = [
  ['editor-src/config.js', ['const EDITOR_BRIDGE_PROTOCOL_VERSION = 1', 'const INSERT_COMMANDS', 'const SCIENTIFIC_NOTATION_REGISTRY']],
  ['editor-src/ui/menu.js', ['function configureFloatingSurface', 'function handleCommandMenuKeyDown']],
  ['editor-src/bridge/events.js', ["emitEditorEvent('attachment_command'", "emitEditorEvent('note_link_command'", 'function emitEditorEvent']],
  ['editor-src/runtime/state.js', ['let activeEditorController = null', 'function setEditorRuntimeError', 'function clearEditorRuntimeError']],
  ['editor-src/attachments/import.js', ['async function stageImageImportFile', 'function attachmentImportPlugin', 'function attachmentImportPlaceholderDom']],
  ['editor-src/document/spec.js', ['const DOCUMENT_SPEC =', 'function validateSpecAttrs', 'function withNodeContract']],
  ['editor-src/document/validation.js', ['function validateCanonicalEnvelope', 'const NODE_VALIDATORS', 'const MARK_VALIDATORS']],
  ['editor-src/document/schema.js', ['const schema = new Schema', 'function parseEnvelope', 'const nodeImplementations']],
  ['editor-src/editor/clipboard.js', ['function editorClipboardProvenancePlugin', 'function stripUntrustedEditorLayoutSlice', 'function writeEditorClipboard']],
  ['editor-src/editor/core.js', ['function normalizeAtomicBoundaryDocument', 'function plugins()', 'class ScientificMathView', 'function renderScientificSource']],
  ['editor-src/editor/interactions.js', ['function continueEmDashSequence', 'function editorBoundaryKeymap']],
  ['editor-src/editor/list_system.js', ['const splitNativeListItem = chainCommands', 'function liftSelectedListItems', 'function backspaceParagraphAfterList', 'function wrapInListJoiningAdjacent', 'function listKeymap']],
  ['editor-src/commands/index.js', ['function commandForBlock', 'function setFontFamily', 'function buildFindDocumentIndex', 'function insertTable']],
  ['editor-src/attachments/view.js', ['class AttachmentView', 'function pdfViewerDocumentHtml', 'function sanitizeImageEditRecipe']],
  ['editor-src/runtime/controller.js', ['class Controller', 'replaceEditorState(serialized, generation)', 'requestPendingAttachmentRecovery()']],
  ['editor-src/bridge/host.js', ['function dispatchEditorCommand', 'window.__rustnoteEditorBridge = Object.freeze({', "case 'install'", "case 'insert_link'"]],
];
for (const [file, markers] of ownership) {
  for (const marker of markers) {
    if (!sources[file].includes(marker)) throw new Error(`A3 ownership contract is missing from ${file}: ${marker}`);
  }
}

const legacyGlobals = [
  '__rustnoteInstallEditor', '__rustnoteSyncEditor', '__rustnoteSetAttachmentRegistry',
  '__rustnoteCompleteImageImport', '__rustnoteCompleteImageEdit', '__rustnoteFormatEditor',
  '__rustnoteEditorController', '__rustnoteEditorLastError', '__rustnoteEditorRuntimeMarker',
];
const combined = sourceInputs.map(file => sources[file]).join('\n');
for (const legacy of legacyGlobals) {
  if (combined.includes(legacy)) throw new Error(`Legacy editor bridge global survived A3: ${legacy}`);
}
const publicBridgeAssignments = (combined.match(/window\.__rustnoteEditorBridge\s*=/g) || []).length;
if (publicBridgeAssignments !== 1) {
  throw new Error(`Expected one public editor bridge namespace assignment, found ${publicBridgeAssignments}`);
}

const sourceLineCounts = Object.fromEntries(sourceInputs.map(file => [file, sources[file].split(/\r?\n/).length]));
for (const [file, lines] of Object.entries(sourceLineCounts)) {
  if (file !== 'editor-src/prosemirror-editor.js' && lines > 5500) {
    throw new Error(`A3 source module is still excessively monolithic (${lines} lines): ${file}`);
  }
}
if (sourceLineCounts['editor-src/runtime/controller.js'] >= 5000
    || sourceLineCounts['editor-src/editor/core.js'] >= 4500
    || sourceLineCounts['editor-src/commands/index.js'] >= 1200) {
  throw new Error('A3 coarse decomposition regressed toward the previous monolith');
}

function resolveLocal(fromFile, specifier) {
  return path.posix.normalize(path.posix.join(path.posix.dirname(fromFile), specifier));
}
function parseNamedExports(source) {
  const exported = new Set();
  for (const match of source.matchAll(/export\s*\{([^}]*)\}\s*;/gs)) {
    for (const item of match[1].split(',')) {
      const raw = item.trim();
      if (!raw) continue;
      exported.add(raw.split(/\s+as\s+/)[0].trim());
    }
  }
  return exported;
}
const exportSets = Object.fromEntries(sourceInputs.map(file => [file, parseNamedExports(sources[file])]));
const graph = new Map(sourceInputs.map(file => [file, []]));
for (const file of sourceInputs) {
  const source = sources[file];
  for (const match of source.matchAll(/import\s+\{([^}]*)\}\s+from\s+["'](\.[^"']+)["']\s*;/gs)) {
    const target = resolveLocal(file, match[2]);
    if (!sourceInputs.includes(target)) continue;
    graph.get(file).push(target);
    const exported = exportSets[target];
    for (const item of match[1].split(',')) {
      const requested = item.trim().split(/\s+as\s+/)[0].trim();
      if (requested && !exported.has(requested)) {
        throw new Error(`${file} imports ${requested} from ${target}, but ${target} does not export it`);
      }
    }
  }
  for (const match of source.matchAll(/import\s+["'](\.[^"']+)["']\s*;/g)) {
    const target = resolveLocal(file, match[1]);
    if (sourceInputs.includes(target)) graph.get(file).push(target);
  }
}
const visiting = new Set();
const visited = new Set();
function visit(file, stack = []) {
  if (visiting.has(file)) throw new Error(`A3 local editor import cycle: ${[...stack, file].join(' -> ')}`);
  if (visited.has(file)) return;
  visiting.add(file);
  for (const dependency of graph.get(file) || []) visit(dependency, [...stack, file]);
  visiting.delete(file);
  visited.add(file);
}
for (const file of graph.keys()) visit(file);

if (sources['editor-src/editor/core.js'].includes('from "../commands/index.js"')
    || sources['editor-src/editor/core.js'].includes("from '../commands/index.js'")) {
  throw new Error('Editor core must not depend upward on the command layer');
}
if (!sources['editor-src/commands/index.js'].includes('../editor/core.js')) {
  throw new Error('Command layer must explicitly consume shared editor-core primitives');
}
if (!sources['editor-src/runtime/controller.js'].includes('../commands/index.js')
    || !sources['editor-src/runtime/controller.js'].includes('../attachments/view.js')) {
  throw new Error('Controller is not consuming the extracted A3 command/attachment modules');
}

const buildScript = read('scripts/build-prosemirror-editor.mjs');
const runtimeValidator = read('scripts/validate-prepared-runtime.sh');
const runtimeInputs = JSON.parse(read('editor-src/runtime-inputs.json'));
for (const requiredInput of [
  'editor-src/editor/clipboard.js',
  'editor-src/editor/interactions.js',
  'editor-src/editor/list_system.js',
  'editor-src/commands/index.js',
  'editor-src/runtime/controller.js',
]) {
  if (!runtimeInputs.editor_source_inputs?.includes(requiredInput)) {
    throw new Error(`A3 canonical runtime inputs omit modular source: ${requiredInput}`);
  }
}
for (const marker of [
  "canonicalRuntimeInputs('editor_source_inputs')",
  'editor_source_inputs: Object.fromEntries(',
  '...editorSources.map(input => input.bytes)',
  'runtime_inputs_sha256: sha256(runtimeInputsBytes)',
]) {
  if (!buildScript.includes(marker)) throw new Error(`A3 build pipeline is missing modular-source contract: ${marker}`);
}
for (const marker of [
  "const runtimeInputsPath = 'editor-src/runtime-inputs.json'",
  'editorManifest.editor_source_inputs',
  'editorManifest.runtime_inputs_sha256',
  'editorManifest.runtime_source_sha256',
  'expectedEditorSourceInputs',
]) {
  if (!runtimeValidator.includes(marker)) throw new Error(`Prepared-runtime validator is missing A3 modular-source integrity contract: ${marker}`);
}
if (runtimeValidator.includes('text cannot contain overlapping link types')
    || runtimeValidator.includes('Media annotation document contract is missing')) {
  throw new Error('Prepared-runtime validation must not contain feature/source wording assertions');
}

console.log('A3 modular source checks passed.');
console.log(`Entry lines: ${sourceLineCounts['editor-src/prosemirror-editor.js']}`);
console.log(`Largest module: ${Object.entries(sourceLineCounts).filter(([f]) => f !== 'editor-src/prosemirror-editor.js').sort((a,b) => b[1]-a[1])[0].join(' = ')} lines`);
NODE

find editor-src -type f \( -name '*.js' -o -name '*.mjs' \) -not -path '*/vendor/*' -print0 | xargs -0 -n1 node --check
node --check scripts/build-prosemirror-editor.mjs

if [[ "${RUSTNOTE_A3_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture A3 static validation passed (A2 acceptance/build/Rust tests skipped by RUSTNOTE_A3_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete A2 acceptance suite..."
./scripts/validate-architecture-a2.sh

echo
echo "Architecture A3 validation passed."

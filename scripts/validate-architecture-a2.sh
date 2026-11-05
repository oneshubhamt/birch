#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A2 typed editor bridge..."
node <<'NODE'
const fs = require('node:fs');

function read(path) {
  if (!fs.existsSync(path)) throw new Error(`Required A2 source is missing: ${path}`);
  return fs.readFileSync(path, 'utf8');
}

const lib = read('src/lib.rs');
const command = read('src/editor_bridge/command.rs');
const event = read('src/editor_bridge/event.rs');
const transport = read('src/editor_bridge/transport.rs');
const liveEditor = read('src/ui/live_editor.rs');
const runtimeInputs = JSON.parse(read('editor-src/runtime-inputs.json'));
const editorSourceInputs = runtimeInputs.editor_source_inputs;
if (!Array.isArray(editorSourceInputs) || editorSourceInputs.length === 0) {
  throw new Error('A2 canonical editor runtime input manifest is invalid');
}
const editorRuntime = editorSourceInputs.map(read).join('\n');
const buildScript = read('scripts/build-prosemirror-editor.mjs');

if (!lib.includes('pub mod editor_bridge;')) {
  throw new Error('A2 editor_bridge module is not compiled into Rustnote');
}
for (const marker of [
  'pub enum EditorCommand',
  'Install(EditorInstallPayload)',
  'CompleteAttachmentPicker',
  'CompleteImageEdit',
  'SetNoteLinkRegistry',
  'ValidateState',
  'ReplaceState',
  'InsertLink',
]) {
  if (!command.includes(marker)) throw new Error(`Typed editor command contract is missing: ${marker}`);
}
if (!/ReadState\s*\{\s*dispatch_input\s*:\s*bool\s*,?\s*\}/m.test(command)) {
  throw new Error('Typed editor command contract is missing: ReadState { dispatch_input: bool }');
}
for (const marker of [
  'EDITOR_BRIDGE_PROTOCOL_VERSION: u16 = 1',
  'pub enum EditorEvent',
  'DocumentChanged(EditorDocumentUpdate)',
  'AttachmentCommand(EditorAttachmentCommand)',
  'NoteLinkCommand(EditorNoteLinkCommand)',
  'decode_editor_event',
]) {
  if (!event.includes(marker)) throw new Error(`Typed editor event contract is missing: ${marker}`);
}
for (const marker of [
  'window.{EDITOR_BRIDGE_GLOBAL}?.dispatch',
  'dispatch_string_script',
  'dispatch_usize_script',
  'wait_for_generation_then_dispatch_script',
]) {
  if (!transport.includes(marker)) throw new Error(`Typed editor transport contract is missing: ${marker}`);
}
if (!liveEditor.includes('id: "note-editor-event-bridge"')) {
  throw new Error('Live editor is missing the single typed event bridge element');
}
for (const staleBridgeId of [
  'note-editor-state-bridge',
  'note-editor-attachment-bridge',
  'note-editor-note-link-bridge',
]) {
  if (liveEditor.includes(staleBridgeId) || editorRuntime.includes(staleBridgeId)) {
    throw new Error(`Legacy editor bridge element survived A2: ${staleBridgeId}`);
  }
}
for (const marker of [
  "const EDITOR_BRIDGE_PROTOCOL_VERSION = 1",
  "const EVENT_BRIDGE_ID = 'note-editor-event-bridge'",
  "emitEditorEvent('document_changed'",
  "emitEditorEvent('attachment_command'",
  "emitEditorEvent('note_link_command'",
  'function dispatchEditorCommand(envelope)',
  'window.__rustnoteEditorBridge = Object.freeze({',
  'dispatch: envelope => dispatchEditorCommand(envelope)',
  'error: () => editorLastError',
  'isReady: generation =>',
]) {
  if (!editorRuntime.includes(marker)) throw new Error(`Editor runtime typed bridge is missing: ${marker}`);
}
const legacyGlobals = [
  '__rustnoteInstallEditor',
  '__rustnoteSyncEditor',
  '__rustnoteSetEditorEditable',
  '__rustnoteSetAttachmentRegistry',
  '__rustnoteRemoveAttachmentFromDocument',
  '__rustnoteRecoverAttachmentPickerImports',
  '__rustnoteSetNoteLinkRegistry',
  '__rustnoteMediaCompatibilityResult',
  '__rustnoteCompleteNoteLinkPicker',
  '__rustnoteFailNoteLinkPicker',
  '__rustnoteBeginAttachmentPickerImport',
  '__rustnoteUpdateAttachmentPickerImport',
  '__rustnoteCompleteAttachmentPicker',
  '__rustnoteFailAttachmentPicker',
  '__rustnoteCompletePdfExport',
  '__rustnoteCompleteImageImport',
  '__rustnoteFailImageImport',
  '__rustnoteCompleteImageEdit',
  '__rustnoteFailImageEdit',
  '__rustnoteDisposeEditor',
  '__rustnoteValidateEditorState',
  '__rustnoteReplaceEditorState',
  '__rustnoteFormatEditor',
  '__rustnoteSetTypography',
  '__rustnoteAdjustFontSize',
  '__rustnoteRefreshEditorSurface',
  '__rustnoteOpenInsertMenu',
  '__rustnoteFindEditor',
  '__rustnoteFocusEditor',
  '__rustnoteToggleEditorFocus',
  '__rustnoteExitFocus',
  '__rustnoteToggleWindowFullscreen',
  '__rustnoteCaptureLinkSelection',
  '__rustnoteInsertLink',
  '__rustnoteEditorController',
  '__rustnoteEditorLastError',
  '__rustnoteEditorRuntimeMarker',
];
const rustBridgeSurface = [
  read('src/editor_bridge/command.rs'),
  read('src/editor_bridge/event.rs'),
  read('src/editor_bridge/transport.rs'),
  liveEditor,
].join('\n');
for (const legacy of legacyGlobals) {
  if (editorRuntime.includes(legacy) || rustBridgeSurface.includes(legacy)) {
    throw new Error(`Legacy editor bridge global survived A2: ${legacy}`);
  }
}
const expectedCommands = [
  'install', 'read_state', 'set_editable', 'set_attachment_registry',
  'remove_attachment_from_document', 'recover_attachment_imports', 'set_note_link_registry',
  'media_compatibility_result', 'complete_note_link_picker', 'fail_note_link_picker',
  'begin_attachment_import', 'update_attachment_import', 'complete_attachment_picker',
  'fail_attachment_picker', 'complete_pdf_export', 'complete_image_import', 'fail_image_import',
  'complete_image_edit', 'fail_image_edit', 'dispose', 'validate_state', 'replace_state',
  'format', 'set_typography', 'apply_font_size_input', 'adjust_font_size', 'refresh_surface', 'focus_editor',
  'open_insert_menu', 'find', 'toggle_editor_focus', 'exit_focus',
  'toggle_window_fullscreen', 'capture_link_selection', 'insert_link',
];
for (const name of expectedCommands) {
  if (!editorRuntime.includes(`case '${name}'`)) {
    throw new Error(`JavaScript dispatcher is missing typed command: ${name}`);
  }
}
const assignmentCount = (editorRuntime.match(/window\.__rustnoteEditorBridge\s*=/g) || []).length;
if (assignmentCount !== 1) {
  throw new Error(`Expected exactly one editor bridge namespace assignment, found ${assignmentCount}`);
}
if (!buildScript.includes('__rustnoteEditorBridge.dispatch')) {
  throw new Error('Prepared editor smoke validation does not target the A2 bridge namespace');
}
for (const marker of [
  "bridgeHostSource.matchAll(/case\\s+['\"]([^'\"]+)['\"]\\s*:/g)",
  'bundleText.includes(singleQuoted)',
  'bundleText.includes(doubleQuoted)',
]) {
  if (!buildScript.includes(marker)) {
    throw new Error(`Prepared editor bundle command validation is not quote-style independent: ${marker}`);
  }
}
if (buildScript.includes("case 'begin_attachment_import'")
    || buildScript.includes("case 'media_compatibility_result'")) {
  throw new Error('Prepared editor bundle validation must not depend on esbuild preserving source quote style');
}
for (const marker of [
  'struct EditorImageImportPayload',
  'struct EditorImageEditPayload',
  'pub enum EditorAttachmentFileAction',
]) {
  if (!event.includes(marker)) throw new Error(`Editor event payload ownership is missing from editor_bridge: ${marker}`);
  if (liveEditor.includes(marker)) throw new Error(`Editor bridge payload type is still duplicated in UI: ${marker}`);
}

console.log('A2 typed editor bridge checks passed.');
NODE

find editor-src -type f \( -name '*.js' -o -name '*.mjs' \) -not -path '*/vendor/*' -print0 | xargs -0 -n1 node --check
node --check scripts/build-prosemirror-editor.mjs

if [[ "${RUSTNOTE_A2_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture A2 static validation passed (dependency install/build/Rust tests skipped by RUSTNOTE_A2_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running A1 acceptance suite..."
./scripts/validate-architecture-a1.sh

echo
echo "Rebuilding the prepared ProseMirror runtime from the A2 source..."
node scripts/build-prosemirror-editor.mjs

echo
echo "Validating prepared browser runtimes..."
./scripts/validate-prepared-runtime.sh

echo
echo "Architecture A2 validation passed."

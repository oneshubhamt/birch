#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking A5 application execution/error boundaries..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required A5 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}
function walk(root, suffix) {
  const result = [];
  for (const entry of fs.readdirSync(root, {withFileTypes: true})) {
    const file = path.join(root, entry.name).replaceAll('\\', '/');
    if (entry.isDirectory()) result.push(...walk(file, suffix));
    else if (entry.isFile() && file.endsWith(suffix)) result.push(file);
  }
  return result;
}

const appMod = read('src/application/mod.rs');
const executor = read('src/application/executor.rs');
const errors = read('src/application/errors.rs');
const backgroundUi = read('src/ui/background.rs');
const settings = read('src/ui/settings.rs');
const editorUi = read('src/ui/editor.rs');
const editorService = read('src/application/editor.rs');
const attachmentService = read('src/application/attachments.rs');
const exportService = read('src/application/export.rs');
const autosave = read('src/autosave.rs');
const journal = read('src/autosave/emergency_journal.rs');

for (const marker of ['pub(crate) mod errors;', 'pub(crate) mod executor;']) {
  if (!appMod.includes(marker)) throw new Error(`A5 application module is missing: ${marker}`);
}
for (const marker of [
  'pub(crate) struct ApplicationExecutor',
  'pub(crate) async fn read<',
  'pub(crate) async fn execute<',
  'ExecutionClass::Mutation',
  'ExecutionClass::ExclusiveMutation',
  'ExecutionClass::Maintenance',
  'pub(crate) fn pause_editing(&self)',
  'ApplicationExecutionError::QueueFull',
]) {
  if (!executor.includes(marker)) throw new Error(`ApplicationExecutor contract is incomplete: ${marker}`);
}
if (executor.includes('dioxus::') || executor.includes('Signal<') || executor.includes('EventHandler<')) {
  throw new Error('ApplicationExecutor must not depend on Dioxus presentation state');
}
for (const marker of ['EditorError', 'AttachmentError', 'ExportError']) {
  if (!errors.includes(`enum ${marker}`)) throw new Error(`Structured service error is missing: ${marker}`);
  if (!errors.includes(`Execution(ApplicationExecutionError)`)) {
    throw new Error('Service errors must preserve application-execution failures');
  }
}
for (const source of [editorService, attachmentService, exportService]) {
  // Reject public service signatures whose error type is literally String. Vec<String>
  // and warning fields remain legitimate result values.
  const lines = source.split(/\r?\n/);
  for (const line of lines) {
    if (/pub\(crate\).*->\s*Result<.*,[ ]*String>/.test(line)) {
      throw new Error(`Application service boundary regressed to Result<_, String>: ${line.trim()}`);
    }
  }
}
if (!editorService.includes('Result<Option<Note>, EditorError>')
    || !attachmentService.includes('Result<Vec<Attachment>, AttachmentError>')
    || !exportService.includes('Result<(), ExportError>')
    || !exportService.includes('Result<PdfExportOutcome, ExportError>')) {
  throw new Error('A5 typed service-result contracts are incomplete');
}

const rustFiles = walk('src', '.rs');
const spawnBlockingOwners = rustFiles.filter(file => read(file).includes('tokio::task::spawn_blocking'));
if (spawnBlockingOwners.length !== 1 || spawnBlockingOwners[0] !== 'src/application/executor.rs') {
  throw new Error(`spawn_blocking must be owned only by ApplicationExecutor; found: ${spawnBlockingOwners.join(', ')}`);
}
for (const stale of [
  'BackgroundOperationCoordinator', 'BackgroundOperationStatus',
  'run_blocking(', 'run_read_blocking(',
  'begin_background_operation(', 'finish_background_operation(',
]) {
  for (const file of rustFiles) {
    if (file === 'src/storage/schema.rs' || file.startsWith('src/storage/attachments.rs')) continue;
    if (read(file).includes(stale)) throw new Error(`Legacy execution pathway survived A5 in ${file}: ${stale}`);
  }
}
for (const forbidden of ['Mutex<', 'Condvar', 'spawn_blocking', 'tokio::spawn']) {
  if (backgroundUi.includes(forbidden)) throw new Error(`UI background viewport still owns execution machinery: ${forbidden}`);
}
if (!backgroundUi.includes('BackgroundOperationViewport') || !backgroundUi.includes('Signal<ApplicationExecutionStatus>')) {
  throw new Error('UI background module must be rendering-only after A5');
}
if (settings.includes('active_operation: Signal<Option<ApplicationOperation>>')
    || settings.includes('use_signal(|| None::<ApplicationOperation>)')) {
  throw new Error('Settings retains a duplicate local application-operation busy state');
}
if (editorUi.includes('let mut attachment_operation = use_signal')
    || editorUi.includes('let attachment_operation = use_signal')) {
  throw new Error('Editor retains a duplicate local attachment-operation busy state');
}
if (!settings.includes('pause_editing()') || !editorUi.includes('pause_editing()')) {
  throw new Error('Exclusive restore flows are not using executor-owned RAII editing pauses');
}

// Autosave remains intentionally specialized rather than being forced into the generic executor.
if (autosave.includes('ApplicationExecutor') || journal.includes('ApplicationExecutor')) {
  throw new Error('A5 must not route autosave/emergency journal through ApplicationExecutor');
}
if (!autosave.includes('thread::Builder::new()') || !journal.includes('thread::Builder::new()')) {
  throw new Error('Specialized autosave/emergency-journal worker architecture was unexpectedly replaced');
}

const workspaceRustFiles = [...rustFiles, ...walk('crates', '.rs')];
const testCount = workspaceRustFiles.reduce((count, file) => count + (read(file).match(/^\s*#\[test\]/gm) || []).length, 0);
if (testCount < 59) throw new Error(`Expected at least 59 workspace Rust regression tests after A5, found ${testCount}`);

console.log('A5 application execution/error checks passed.');
console.log(`Workspace Rust regression tests discovered: ${testCount}`);
NODE

if [[ "${RUSTNOTE_A5_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture A5 static validation passed (cumulative Rust/build acceptance skipped by RUSTNOTE_A5_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running cumulative A1-A4 closure acceptance suite..."
./scripts/validate-architecture-a1-a4-closure.sh

echo
echo "Running A5 workspace check..."
cargo check --workspace --locked

echo
echo "Architecture A5 validation passed."

#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking B6 typed-error closure and storage-owned search SQL..."
node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

function read(file) {
  if (!fs.existsSync(file)) throw new Error(`Required B6 source is missing: ${file}`);
  return fs.readFileSync(file, 'utf8');
}

function walk(root, suffix) {
  const out = [];
  if (!fs.existsSync(root)) return out;
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const file = path.join(root, entry.name).replaceAll('\\', '/');
    if (entry.isDirectory()) out.push(...walk(file, suffix));
    else if (entry.isFile() && file.endsWith(suffix)) out.push(file);
  }
  return out;
}

// Parse Result<..., ...> generic arguments well enough to distinguish a String
// return value from String nested inside the success type (Vec<String>, tuples, etc.).
function directStringResultTypes(source) {
  const found = [];
  let position = 0;
  while ((position = source.indexOf('Result', position)) !== -1) {
    let cursor = position + 'Result'.length;
    while (/\s/.test(source[cursor] || '')) cursor += 1;
    if (source[cursor] !== '<') {
      position = cursor;
      continue;
    }
    let depth = 0;
    let comma = -1;
    let end = -1;
    for (let index = cursor; index < source.length; index += 1) {
      const char = source[index];
      if (char === '<') depth += 1;
      else if (char === '>') {
        depth -= 1;
        if (depth === 0) {
          end = index;
          break;
        }
      } else if (char === ',' && depth === 1 && comma < 0) {
        comma = index;
      }
    }
    if (end > 0 && comma > 0) {
      const errorType = source
        .slice(comma + 1, end)
        .replace(/\/\*[\s\S]*?\*\//g, '')
        .replace(/\/\/[^\n]*/g, '')
        .replace(/\s+/g, '');
      if (['String', 'std::string::String', 'alloc::string::String'].includes(errorType)) {
        found.push(source.slice(position, end + 1));
      }
    }
    position = end > 0 ? end + 1 : cursor + 1;
  }
  return found;
}

const rustFiles = [...walk('src', '.rs'), ...walk('crates', '.rs')];
const directStringResults = [];
for (const file of rustFiles) {
  for (const signature of directStringResultTypes(read(file))) {
    directStringResults.push(`${file}: ${signature.replace(/\s+/g, ' ').slice(0, 180)}`);
  }
}
if (directStringResults.length !== 0) {
  throw new Error(`B6 direct Result<_, String> APIs survived:\n${directStringResults.join('\n')}`);
}

const executor = read('src/application/executor.rs');
if (executor.includes('impl ExecutionFailure for String')) {
  throw new Error('B6 executor still permits String to erase worker error categories');
}

const typedContracts = {
  'src/editor_document.rs': ['pub enum DocumentValidationError', 'DocumentValidationResult<T>'],
  'src/autosave.rs': ['pub(crate) enum AutosaveError', 'AutosaveResult<T>'],
  'src/autosave/emergency_journal.rs': ['pub(crate) enum EmergencyJournalError', 'JournalResult<T>'],
  'src/platform/mod.rs': ['pub enum PlatformError', 'PlatformResult<T>'],
  'src/native/mod.rs': ['pub enum NativeError', 'NativeResult<T>'],
  'src/media_compat.rs': ['pub(crate) enum MediaCompatibilityError', 'MediaCompatibilityResult<T>'],
  'src/library.rs': ['pub enum LibraryRuntimeError'],
  'src/editor_bridge/event.rs': ['pub enum EditorBridgeDecodeError'],
  'src/application/context.rs': ['pub(crate) enum ApplicationOpenError'],
  'src/application/errors.rs': ['pub(crate) enum DataAccessError', 'DataResult<T>', 'PersistenceListenerRestarted'],
  'src/ui/synchronization.rs': ['pub(super) enum SynchronizationError', 'SynchronizationResult<T>'],
  'src/domain/note.rs': ['pub struct NoteIdError', 'Result<Self, NoteIdError>'],
  'src/domain/source.rs': ['pub struct SourceIdError', 'Result<Self, SourceIdError>'],
};
for (const [file, markers] of Object.entries(typedContracts)) {
  const source = read(file);
  for (const marker of markers) {
    if (!source.includes(marker)) throw new Error(`B6 typed error contract is missing from ${file}: ${marker}`);
  }
}

for (const file of [
  'src/application/library.rs',
  'src/application/notes.rs',
  'src/application/settings.rs',
  'src/application/runtime.rs',
]) {
  const source = read(file);
  if (!source.includes('DataResult<') || !source.includes('DataAccessError')) {
    throw new Error(`B6 application data service does not expose the typed application boundary: ${file}`);
  }
  if (source.includes('StorageResult')) {
    throw new Error(`B6 application data service still leaks StorageResult: ${file}`);
  }
}

for (const file of walk('src/ui', '.rs')) {
  const source = read(file);
  for (const forbidden of ['crate::storage', 'storage::Storage', 'StorageError', 'StorageResult']) {
    if (source.includes(forbidden)) {
      throw new Error(`B6 typed-error work reintroduced a storage implementation dependency in ${file}: ${forbidden}`);
    }
  }
}
const taskError = read('src/ui/task_error.rs');
for (const marker of ['enum DataTaskError', 'Data(DataAccessError)', 'Execution(ApplicationExecutionError)']) {
  if (!taskError.includes(marker)) throw new Error(`B6 UI data-task boundary is incomplete: ${marker}`);
}

const search = read('src/search.rs');
for (const forbidden of [
  'notes_fts',
  'notes_fts MATCH',
  'SELECT note_id FROM notes_fts',
  'attachment_names :',
  'title :',
  'body :',
  'tags :',
  'candidate_sql_predicate',
  'preview_fts_expression',
]) {
  if (search.includes(forbidden)) {
    throw new Error(`B6 search AST leaked storage/FTS knowledge: ${forbidden}`);
  }
}
const storageSearch = read('src/storage/search_query.rs');
if (!storageSearch.includes('r#\"title:\"Civil Services\" body:ethics -tag:draft OR -attachment:audio\"#')) {
  throw new Error('B6 storage FTS behavior test regressed to escaped raw-string phrase syntax');
}
for (const marker of [
  'preview_fts_expression',
  'candidate_sql_predicate',
  'notes_fts MATCH',
  'attachment_names :',
  'all_negative_branch_uses_indexed_not_in_predicate',
]) {
  if (!storageSearch.includes(marker)) throw new Error(`B6 storage-owned search translation is incomplete: ${marker}`);
}
const storageMod = read('src/storage/mod.rs');
if (!storageMod.includes('mod search_query;')) throw new Error('B6 storage search translation module is not registered');
for (const file of ['src/storage/notes.rs', 'src/storage/note_candidates.rs']) {
  if (!read(file).includes('super::search_query::')) {
    throw new Error(`B6 storage search consumer bypasses the canonical translation module: ${file}`);
  }
}

const domainNote = read('src/domain/note.rs');
const domainSource = read('src/domain/source.rs');
if (!domainNote.includes('identifier_parse_preserves_typed_validation_error') ||
    !domainSource.includes('identifier_parse_preserves_typed_validation_error')) {
  throw new Error('B6 typed identifier regression tests are missing');
}

const preparedRuntimeValidator = read('scripts/validate-prepared-runtime.sh');
const editorBuildScript = read('scripts/build-prosemirror-editor.mjs');
for (const forbidden of [
  'text cannot contain overlapping link types',
  'Media annotation document contract is missing',
  'Full PDF page renders must seed the shared Preview Cache',
  'Preview scheduler priority contract is invalid',
  'Bookmark added at',
  'Range saved:',
]) {
  if (preparedRuntimeValidator.includes(forbidden)) {
    throw new Error(`B6 prepared-runtime validator regressed to brittle feature/source wording checks: ${forbidden}`);
  }
}
for (const forbidden of [
  'Editor contextual-interaction source contract is missing',
  'Image edit binary-staging source contract is missing',
  'Bookmark added at',
  'Range saved:',
]) {
  if (editorBuildScript.includes(forbidden)) {
    throw new Error(`B6 editor build script regressed to feature/source marker validation: ${forbidden}`);
  }
}
if (!preparedRuntimeValidator.includes('Prepared editor modular-source checksum map mismatch')
    || !preparedRuntimeValidator.includes('Prepared PDF modular-source checksum map mismatch')
    || !preparedRuntimeValidator.includes('vm.runInNewContext(editorBundle')) {
  throw new Error('B6 prepared-runtime validator must remain artifact/checksum/executable-smoke oriented');
}

const noteMetadata = read('src/ui/note_metadata.rs');
if (!noteMetadata.includes('Err(error) => preview_error.set(Some((revision_id.clone(), error.to_string())))')) {
  throw new Error('B6 revision preview errors must be rendered to text only at the UI signal boundary');
}

const autosave = read('src/autosave.rs');
if (!autosave.includes('pub(crate) struct AutosaveController') ||
    !autosave.includes('pub(crate) enum AutosaveEvent')) {
  throw new Error('B6 autosave typed boundary must remain crate-scoped');
}
if (/^\s{4}pub fn (?:try_new|schedule|flush_note|flush_pending|flush_pending_live|flush_pending_silent|checkpoint_note_silent|checkpoint_all|discard_note|reset_library|is_current_note_version)\b/m.test(autosave)) {
  throw new Error('B6 autosave controller API leaked beyond its crate-scoped typed error boundary');
}
if (!autosave.includes('#[cfg(test)]\n    pub(crate) fn schedule(&self, note: Note)') ||
    autosave.includes('pub(crate) fn flush_note(&self, note: Note)')) {
  throw new Error('B6 autosave dead-code closure regressed: unscoped test helper or unused flush_note facade present');
}

const attachmentProtocol = read('src/attachment_protocol.rs');
if (attachmentProtocol.includes('attachment_json_error(400, &error)') ||
    !attachmentProtocol.includes('attachment_json_error(400, &error.to_string())')) {
  throw new Error('B6 native staging errors must be rendered to text at the attachment protocol response boundary');
}

if (read('src/application/notes.rs').includes('fn prune_revision_history(&self)')) {
  throw new Error('B6 dead NoteService revision-pruning facade was reintroduced');
}

const librarySource = read('src/library.rs');
if (!librarySource.includes('path::PathBuf')) {
  throw new Error('B6 library test support lost its explicit PathBuf import');
}

const editorCommandSource = read('src/editor_bridge/command.rs');
if (!editorCommandSource.includes('descriptor: Box<EditorAttachmentDescriptor>')) {
  throw new Error('B6 typed editor command regressed to an oversized CompleteImageEdit enum payload');
}

const editorDocumentSource = read('src/editor_document.rs');
if (!editorDocumentSource.includes('mark_types.contains(&candidate.as_str())')) {
  throw new Error('B6 editor document validation regressed to the manual-contains Clippy pattern');
}

const editorWorkflows = read('src/ui/editor_workflows.rs');
for (const marker of [
  'struct AttachmentWorkflowPresentation',
  'presentation: AttachmentWorkflowPresentation',
]) {
  if (!editorWorkflows.includes(marker)) {
    throw new Error(`B6 editor workflow presentation boundary is missing: ${marker}`);
  }
}
for (const oversizedSignature of [
  'run_attachment_import(\n    workflows: EditorWorkflowService,\n    attachment_service:',
  'run_pending_attachment_recovery(\n    workflows: EditorWorkflowService,\n    attachment_service:',
]) {
  if (editorWorkflows.includes(oversizedSignature)) {
    throw new Error('B6 editor workflow regressed to the pre-Clippy oversized argument surface');
  }
}

const a2Validator = read('scripts/validate-architecture-a2.sh');
if (!a2Validator.includes('/ReadState\\s*\\{\\s*dispatch_input\\s*:\\s*bool\\s*,?\\s*\\}/m.test(command)')) {
  throw new Error('B6 A2 ReadState validation regressed to rustfmt-sensitive source-shape matching');
}

const applicationEditorWorkflows = read('src/application/editor_workflows.rs');
for (const marker of [
  '\"type\": \"attachment_pending\"',
  '\"requestId\": \"request-b4\"',
  'pending_note.replace_document(',
]) {
  if (!applicationEditorWorkflows.includes(marker)) {
    throw new Error(`B6 detached-finalization behavior test lost its durable request precondition: ${marker}`);
  }
}

const rustTestCount = rustFiles.reduce(
  (count, file) => count + (read(file).match(/^\s*#\[test\]/gm) || []).length,
  0,
);
if (rustTestCount < 69) {
  throw new Error(`B6 Rust regression coverage regressed: expected at least 69 tests, found ${rustTestCount}`);
}

console.log(`B6 guards passed. Direct Result<_, String> APIs: 0; Rust tests discovered: ${rustTestCount}.`);
NODE

echo
echo "Checking B6 JavaScript and shell syntax..."
find editor-src -type f \( -name '*.js' -o -name '*.mjs' \) -not -path '*/vendor/*' -print0 | xargs -0 -n1 node --check
find pdf-src tests/js -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
zsh -n scripts/validate-architecture-b6.sh

echo
echo "Running B6 dependency-free PDF behavioral tests..."
node --test tests/js/pdf-behavior.test.mjs tests/js/pdf-runtime-integrity.test.mjs

echo
echo "Running cumulative B5 architecture guards in static mode..."
RUSTNOTE_B5_STATIC_ONLY=1 ./scripts/validate-architecture-b5.sh

if [[ "${RUSTNOTE_B6_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture B6 static validation passed (full prepared-runtime/Rust/editor QA skipped by RUSTNOTE_B6_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete B5 acceptance suite..."
./scripts/validate-architecture-b5.sh

echo
echo "Rechecking B6 after cumulative build/QA..."
RUSTNOTE_B6_STATIC_ONLY=1 ./scripts/validate-architecture-b6.sh

echo
echo "Architecture B6 validation passed."

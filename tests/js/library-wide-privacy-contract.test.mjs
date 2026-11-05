import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {assertRustOrder} from './source-contract-helpers.mjs';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');
const coordinator = read('src/security/privacy_coordinator.rs');
const appPrivacy = read('src/application/privacy.rs');
const executor = read('src/application/executor.rs');
const sync = read('src/ui/synchronization.rs');
const app = read('src/ui/app.rs');
const shutdown = read('src/ui/shutdown.rs');
const settings = read('src/ui/settings.rs');
const appRuntime = read('src/application/runtime.rs');
const appEditor = read('src/application/editor.rs');
const appExport = read('src/application/export.rs');
const backups = read('src/storage/backups.rs');
const repository = read('src/storage/backup_repository.rs');
const uiPortability = read('src/ui/portability.rs');
const appPortability = read('src/application/portability.rs');
const storagePortability = read('src/storage/portability.rs');
const capabilities = read('src/storage/capabilities.rs');
const derivative = read('src/security/derivative.rs');

function segment(text, start, end) {
  const a = text.indexOf(start);
  assert.notEqual(a, -1, `missing start marker: ${start}`);
  const b = text.indexOf(end, a + start.length);
  assert.notEqual(b, -1, `missing end marker: ${end}`);
  return text.slice(a, b);
}

const helper = segment(
  sync,
  'pub(super) async fn synchronize_then_begin_whole_library_operation(',
  '#[derive(Clone, Debug, PartialEq, Eq)]',
);
const autoBackup = segment(app, 'let automatic_backup_runtime', 'let notebook_name');
const archive = segment(settings, 'title: "Export complete archive"', 'title: "Restore complete archive"');
const restore = segment(settings, 'let restore_source = pending.source.clone();', '"Restoring…"');

test('whole-library durability has a dedicated non-publication privacy lease', () => {
  assert.match(coordinator, /pub\(crate\) struct WholeLibraryOperationLease/);
  assert.match(coordinator, /pub\(crate\) fn begin_whole_library_operation\(/);
  assert.match(appPrivacy, /pub\(crate\) struct PrivacyOperationService/);
  assert.match(appPrivacy, /pub\(crate\) struct WholeLibraryOperationLease/);
  assert.doesNotMatch(helper, /begin_publication\(\)/);
});

test('serialized executor turn supports staged work before privacy authority and normal execute reuses it', () => {
  assert.match(executor, /pub\(crate\) struct ApplicationExecutionTurn/);
  assert.match(executor, /pub\(crate\) async fn begin_operation\(/);
  assert.match(executor, /pub\(crate\) async fn stage<T, E, F>/);
  assert.match(executor, /\.begin_operation\(operation\)[\s\S]*?turn\.execute\(work\)\.await/);
  assert.match(executor, /impl Drop for ApplicationExecutionTurn/);
});

test('whole-library helper requires executor ownership, then synchronizes source before fresh privacy capture', () => {
  assert.match(helper, /_operation_turn: &ApplicationExecutionTurn/);
  const synchronize = helper.indexOf('synchronize_current_editor_scoped(');
  const generation = helper.indexOf('let synchronized_selection_generation = *note_selection_generation.peek();');
  const stageTwoCall = helper.indexOf('begin_whole_library_operation_for_synchronized_selection(');
  assert.ok(synchronize >= 0 && generation > synchronize && stageTwoCall > generation);

  const stageTwo = segment(
    sync,
    'pub(super) async fn begin_whole_library_operation_for_synchronized_selection(',
    '#[derive(Clone, Debug, PartialEq, Eq)]',
  );
  assert.match(stageTwo, /let mut authority_was_delayed = false/);
  assert.match(stageTwo, /if \*note_selection_generation\.peek\(\) != synchronized_selection_generation[\s\S]*?return Ok\(None\)/);
  assert.match(stageTwo, /operation_privacy\.begin_whole_library_operation\(\)\?/);
  assert.match(stageTwo, /Some\(lease\)[\s\S]*?\|\| authority_was_delayed[\s\S]*?drop\(lease\)[\s\S]*?return Ok\(None\)/);
  assert.match(stageTwo, /None =>[\s\S]*?authority_was_delayed = true/);
  assert.match(stageTwo, /return Ok\(Some\(lease\)\)/);
});

test('backup owns executor turn before synchronizing and capturing the library lease', () => {
  assertRustOrder(autoBackup, [
    '.begin_operation(operation)',
    'synchronize_then_begin_whole_library_operation(',
    'backup_turn.execute(move ||',
  ]);
  assert.match(autoBackup, /&backup_turn/);
  assert.match(autoBackup, /SynchronizationMode::Persist/);
  assert.match(autoBackup, /maybe_create_automatic_backup\(&backup_lease\)/);
  assert.doesNotMatch(autoBackup, /wait_for_multi_note_publication_token/);
  assert.doesNotMatch(autoBackup, /_backup_publication_lease/);
});

test('complete archive owns executor turn before source sync and fresh library authority', () => {
  const turn = archive.indexOf('.begin_operation(operation)');
  const syncPos = archive.indexOf('synchronize_then_begin_whole_library_operation(');
  const execute = archive.indexOf('.execute(move ||');
  assert.ok(turn >= 0 && syncPos > turn && execute > syncPos);
  assert.match(archive, /&operation_turn/);
  assert.match(archive, /SynchronizationMode::Persist/);
  assert.match(archive, /export_archive\(&archive_lease, &path\)/);
  assert.doesNotMatch(archive, /capture_multi_note_publication\(/);
  assert.doesNotMatch(archive, /begin_publication\(\)/);
});

test('shutdown serializes source sync and checkpoint_all before capturing the WAL durability lease', () => {
  const turn = shutdown.indexOf('.begin_operation(ApplicationOperation::DurabilityCheckpoint)');
  const syncPos = shutdown.indexOf('synchronize_current_editor_scoped(');
  const checkpoint = shutdown.indexOf('.checkpoint_all()');
  const lease = shutdown.indexOf('begin_whole_library_operation_for_synchronized_selection(');
  const execute = shutdown.indexOf('.execute(move ||');
  const durability = shutdown.indexOf('durability_checkpoint(&library_operation_lease)');
  assert.ok(turn >= 0 && syncPos > turn && checkpoint > syncPos && lease > checkpoint && execute > lease && durability > execute);
  assert.match(shutdown, /\.stage\(move \|\|/);
  assert.match(shutdown, /SynchronizationMode::Checkpoint/);
  assert.doesNotMatch(shutdown, /checkpoint_all\(&library_operation_lease\)/);
  assert.doesNotMatch(shutdown, /capture_multi_note_publication\(/);
});

test('true whole-library APIs require the typed lease while checkpoint_all remains note-local', () => {
  assert.match(appRuntime, /maybe_create_automatic_backup\([\s\S]*?_privacy_lease: &WholeLibraryOperationLease/);
  assert.match(appRuntime, /durability_checkpoint\([\s\S]*?_privacy_lease: &WholeLibraryOperationLease/);
  assert.match(appExport, /export_archive\([\s\S]*?_privacy_lease: &WholeLibraryOperationLease/);
  assert.match(appEditor, /pub\(crate\) fn checkpoint_all\(&self\)/);
  assert.doesNotMatch(appEditor, /checkpoint_all\([\s\S]{0,160}?WholeLibraryOperationLease/);
});

test('Lock and both Restore admission paths exclude already-started whole-library operations', () => {
  assert.match(coordinator, /fn whole_library_operation_serializes_against_note_privacy_transition\(\)/);
  assert.match(coordinator, /fn whole_library_operation_serializes_against_protected_note_restore\(\)/);
  assert.match(coordinator, /fn whole_library_operation_serializes_against_library_restore\(\)/);
  assert.ok(
    coordinator.split('state = self.wait_for_whole_library_operations_to_drain(state);').length - 1 >= 3,
    'all privacy transition admission variants must drain active whole-library operations',
  );
});

test('Restore owns executor turn before source sync, then uses exclusive library revocation', () => {
  const turn = restore.indexOf('.begin_operation(operation)');
  const sourceSync = restore.indexOf('synchronize_current_editor_scoped(');
  const uiRevoke = restore.indexOf('begin_library_ui_revocation()');
  const execute = restore.indexOf('.execute(move ||');
  const replace = restore.indexOf('restore_complete_archive(&path)');
  const release = restore.indexOf('drop(privacy_revocation);');
  const fresh = restore.indexOf('let _post_restore_publication_lease = loop');
  assert.ok(turn >= 0 && sourceSync > turn && uiRevoke > sourceSync && execute > uiRevoke && replace > execute && release > replace && fresh > release);
  assert.doesNotMatch(restore, /synchronize_then_begin_whole_library_operation\(/);
  assert.match(backups, /begin_library_privacy_transition\([\s\S]*?LibraryPrivacyTransition::Restore/);
  assert.match(repository, /begin_library_privacy_transition\([\s\S]*?LibraryPrivacyTransition::Restore/);
});

test('failed Restore cannot strand the UI after library revocation', () => {
  assert.match(restore, /let _post_restore_publication_lease = loop[\s\S]*?capture_multi_note_publication\([\s\S]*?token\.begin_publication\(\)/);
  const recoveryCalls = restore.match(/onrestored\.call\(\(\)\);/g) ?? [];
  assert.ok(recoveryCalls.length >= 2, 'success and failure must both trigger canonical UI reconstruction');
});

test('Library portability export owns executor turn, source sync, and typed whole-library authority', () => {
  const libraryExport = uiPortability.slice(uiPortability.indexOf('fn LibraryExportAction('));
  const turn = libraryExport.indexOf('background.begin_operation(operation)');
  const syncPos = libraryExport.indexOf('synchronize_then_begin_whole_library_operation(');
  const exportPos = libraryExport.indexOf('.export_library_scope(');
  assert.ok(turn >= 0 && syncPos > turn && exportPos > syncPos);
  assert.match(libraryExport, /&operation_turn/);
  assert.match(libraryExport, /SynchronizationMode::Persist/);
  assert.match(libraryExport, /&library_lease/);
  assert.doesNotMatch(libraryExport, /PortabilityScope::Library/);

  assert.match(appPortability, /pub\(crate\) async fn export_library_scope\([\s\S]*?operation_turn: ApplicationExecutionTurn[\s\S]*?_privacy_lease: &WholeLibraryOperationLease/);
  assert.match(appPortability, /PortabilityScope::Library => Err\([\s\S]*?requires whole-library privacy authority/);
  assert.match(capabilities, /pub\(crate\) fn export_portable_library_scope/);
});

test('Notebook portability export stabilizes its exact multi-note scope before emitting plaintext', () => {
  const notebookExport = segment(
    storagePortability,
    'pub(super) fn export_portable_notebook_scope_authorized(',
    'fn export_portable_scope_locked(',
  );
  const candidate = notebookExport.indexOf('let candidate_ids =');
  const permit = notebookExport.indexOf('acquire_multi_note_plaintext_operation(&candidate_ids)');
  const lock = notebookExport.indexOf('let _operation_guard = self.lock_operations()?;', permit);
  const recheck = notebookExport.indexOf('let stabilized_ids = portable_scope_note_ids', lock);
  const compare = notebookExport.indexOf('if stabilized_ids != candidate_ids', recheck);
  const protectedCheck = notebookExport.indexOf('ensure_portability_scope_unprotected', compare);
  const exportPos = notebookExport.indexOf('export_portable_scope_locked', protectedCheck);
  assert.ok(candidate >= 0 && permit > candidate && lock > permit && recheck > lock && compare > recheck && protectedCheck > compare && exportPos > protectedCheck);
  assert.match(appPortability, /PortabilityScope::Notebook\(notebook_id\)[\s\S]*?export_portable_notebook_scope/);
});

test('multi-note plaintext lifetime blocks only included note transitions', () => {
  assert.match(derivative, /pub\(crate\) fn acquire_notes\([\s\S]*?state\.transitioning\.contains\(note_id\)/);
  assert.match(coordinator, /pub\(crate\) struct MultiNotePlaintextOperationPermit/);
  assert.match(coordinator, /pub\(crate\) fn acquire_multi_note_plaintext_operation\([\s\S]*?wait_for_notes_admission_clear\(&ids\)[\s\S]*?plaintext_derivatives\.acquire_notes\(&ids\)/);
  assert.match(coordinator, /fn multi_note_plaintext_operation_blocks_only_included_transitions\(\)/);
});


import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {assertRustOrder} from './source-contract-helpers.mjs';
import test from 'node:test';

function read(path) {
  return readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');
}

const uiMod = read('src/ui/mod.rs');
const noteListData = read('src/ui/note_list_data.rs');
const notesHeader = read('src/ui/notes_header.rs');
const sidebar = read('src/ui/sidebar.rs');
const editor = read('src/ui/editor.rs');
const applicationEditor = read('src/application/editor.rs');
const synchronization = read('src/ui/synchronization.rs');
const noteLinks = read('src/ui/note_links.rs');

function functionBody(source, signature, nextSignature) {
  const start = source.indexOf(signature);
  assert.notEqual(start, -1, `missing ${signature}`);
  const end = source.indexOf(nextSignature, start + signature.length);
  assert.notEqual(end, -1, `missing boundary ${nextSignature}`);
  return source.slice(start, end);
}

test('ordinary note navigation checkpoints source before target authorization/load/publication', () => {
  const body = functionBody(noteListData, 'pub(super) fn select_note', 'fn restore_note_list_scroll');
  const sourceSync = body.indexOf('synchronize_current_editor_when_privacy_ready');
  const targetCapture = body.indexOf('capture_note_publication_token');
  const targetLoad = body.indexOf('worker.load_note(&worker_note_id)');
  const targetFreshness = body.indexOf('if !target_token.is_publishable()');
  const targetLease = body.indexOf('target_token.begin_publication()');
  const targetPublish = body.indexOf('state.selected_note.set(Some(note))');

  for (const [label, index] of [
    ['source sync', sourceSync],
    ['target capture', targetCapture],
    ['target load', targetLoad],
    ['target freshness check', targetFreshness],
    ['target lease', targetLease],
    ['target publication', targetPublish],
  ]) {
    assert.notEqual(index, -1, `missing ${label}`);
  }

  assert.ok(sourceSync < targetCapture);
  assert.ok(targetCapture < targetLoad);
  assert.ok(targetLoad < targetFreshness);
  assert.ok(targetFreshness < targetLease);
  assert.ok(targetLease < targetPublish);
  assert.match(body, /let selection_generation = state\.advance_note_selection_generation\(\)/);
  assert.match(body, /if !state\.note_selection_is_current\(selection_generation\)[\s\S]*?return;/);
  assert.match(body, /if !target_token\.is_publishable\(\)\s*\{\s*continue;/);
});

test('list reconciliation chooses target identity only and re-enters ordinary target navigation', () => {
  assert.match(noteListData, /enum SelectionReconciliation\s*\{[\s\S]*?Replace\(Option<NoteId>\)/);
  assert.doesNotMatch(noteListData, /Replace\(Option<Box<Note>>\)/);
  assert.match(noteListData, /page\.notes\.first\(\)\.map\(\|first\| first\.id\.clone\(\)\)/);
  assert.match(noteListData, /SelectionReconciliation::Replace\(target_note_id\)[\s\S]*?Some\(target_note_id\)[\s\S]*?select_note\(editor\.clone\(\), target_note_id, state, \|_\| \{\}\)/);
});

test('section and sidebar navigation checkpoint source then acquire independent aggregate publication', () => {
  const section = functionBody(notesHeader, 'pub(super) fn switch_library_section', 'pub(super) fn toggle_selected_pin');
  const sidebarNav = functionBody(sidebar, 'fn schedule_sidebar_navigation', 'fn focus_notebook_create_input');

  for (const body of [section, sidebarNav]) {
    assert.match(body, /synchronize_then_begin_multi_note_publication_when_privacy_ready/);
    assert.doesNotMatch(body, /synchronize_and_begin_publication_when_privacy_ready/);
  }

  const helper = functionBody(uiMod, 'async fn synchronize_then_begin_multi_note_publication_when_privacy_ready', '\n}');
  assert.match(helper, /synchronize_current_editor_when_privacy_ready/);
  assert.match(helper, /wait_for_multi_note_publication_token/);
  assert.ok(helper.indexOf('synchronize_current_editor_when_privacy_ready') < helper.indexOf('wait_for_multi_note_publication_token'));
});

test('navigation entry points funnel target-note publication through select_note', () => {
  assert.match(notesHeader, /onopen:[\s\S]*?select_note\(/);
  assert.match(notesHeader, /ontags:[\s\S]*?select_note\(/);
  assert.match(editor, /onopennote:[\s\S]*?select_note\(/);
  assert.match(noteListData, /navigate_to_note_index[\s\S]*?select_note\(/);
});



test('no-current-note navigation preserves the pending-flush durability edge case', () => {
  const helper = functionBody(uiMod, 'async fn synchronize_current_editor_when_privacy_ready', 'async fn synchronize_then_begin_multi_note_publication_when_privacy_ready');
  assertRustOrder(helper, [
    'match source_note_id.as_ref()',
    'Some(note_id)',
    'capture_note_publication_token',
    'None => self.privacy_epoch.capture_multi_note_publication_with_lifecycle',
  ]);
  const sync = functionBody(synchronization, 'pub(super) async fn synchronize_current_editor', 'const EDITOR_SESSION_STABILIZATION_ATTEMPTS');
  assert.match(sync, /let Some\(selected\) = captured\.selected else \{[\s\S]*?flush_without_selected_note\(service, mode\)\.await/);
});

test('backlinks reuse target navigation while note pickers remain selection-only', () => {
  assert.match(editor, /BacklinksPanelActions[\s\S]*?onopen: onopennote/);
  assert.match(noteLinks, /BacklinksPanelActions[\s\S]*?pub onopen: EventHandler<NoteId>/);
  assert.match(noteLinks, /NotePickerActions \{[\s\S]*?onselect,[\s\S]*?oncancel,[\s\S]*?onstatus/);
  assert.doesNotMatch(noteLinks, /selected_note\.set/);
});

test('old combined application switch-note workflow remains absent', () => {
  assert.doesNotMatch(applicationEditor, /pub\(crate\) fn switch_note\s*\(/);
  assert.doesNotMatch(applicationEditor, /NoteSwitchPersistence/);
  assert.doesNotMatch(applicationEditor, /checkpoint_and_load_note_for_selection/);
});

test('create-note selection does not reuse source authority to publish the new target', () => {
  const create = functionBody(notesHeader, 'fn create_contextual_note(', 'fn create_contextual_note_blocking');
  assert.match(create, /synchronize_current_editor_when_privacy_ready/);
  assert.match(create, /capture_note_publication_token\(\s*&target_note_id/);
  assert.match(create, /target_token\.begin_publication\(\)/);
  assert.match(create, /wait_for_multi_note_publication_token\(Some\(selection_generation\)\)/);
  assert.doesNotMatch(create, /wait_for_note_list_publication_token/);
});

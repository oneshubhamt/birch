import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {assertRustOrder} from './source-contract-helpers.mjs';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');
const noteCard = read('src/ui/note_card.rs');
const uiMod = read('src/ui/mod.rs');
const sync = read('src/ui/synchronization.rs');
const list = read('src/ui/note_list_data.rs');
const header = read('src/ui/notes_header.rs');
const app = read('src/ui/app.rs');

function segment(text, start, end) {
  const a = text.indexOf(start);
  assert.notEqual(a, -1, `missing start marker: ${start}`);
  const b = text.indexOf(end, a + start.length);
  assert.notEqual(b, -1, `missing end marker: ${end}`);
  return text.slice(a, b);
}

const workflow = segment(noteCard, 'pub(super) fn apply_note_card_action(', 'pub(super) fn apply_note_card_tag_action(');
const canonical = segment(noteCard, 'async fn read_canonical_note_for_vault_publication(', 'fn note_card_action_note_id');
const aggregateLifetime = segment(app, 'let _aggregate_privacy_lifetime = use_effect', 'let _note_list_page_executor');
const mutationSync = segment(sync, 'pub(super) async fn synchronize_for_note_mutation(', 'fn acquire_publication_lease');
const selection = segment(list, 'pub(super) fn select_note', 'fn restore_note_list_scroll');
const create = segment(header, 'pub(super) fn create_contextual_note', 'fn create_contextual_note_blocking');

test('Vault busy state names the active note and is not consulted by ordinary navigation', () => {
  assert.match(uiMod, /vault_action_busy: Signal<Option<NoteId>>/);
  assert.match(app, /use_signal\(\|\| None::<NoteId>\)/);
  assert.match(workflow, /VaultActionBusyGuard::begin\(state\.vault_action_busy, &target_note_id\)/);
  assert.doesNotMatch(selection, /vault_action_busy/);
  assert.doesNotMatch(create, /vault_action_busy\.peek\(\)/);
});

test('selected B uses live-editor synchronization while unselected B gets a silent checkpoint that upgrades if B becomes live', () => {
  assert.match(mutationSync, /loop \{[\s\S]*?target_is_selected[\s\S]*?synchronize_current_editor\([\s\S]*?SynchronizationMode::CheckpointSilent/);
  assert.match(mutationSync, /checkpoint_note_silent\(&checkpoint_note_id\)/);
  assert.match(mutationSync, /let target_selected = selected_note[\s\S]*?if target_selected \{[\s\S]*?continue;/);
  assert.match(mutationSync, /NoteMutationSynchronization[\s\S]*?selection_generation[\s\S]*?target_selected/);
  assert.doesNotMatch(workflow, /vault_action_requires_selected_target/);
  assert.doesNotMatch(workflow, /apply_note_card_action_with_selection/);
  assert.doesNotMatch(workflow, /select_note\(/);
});

test('B durability barrier returns the exact revocation-boundary selection snapshot before UI revocation and storage transition', () => {
  const checkpoint = workflow.indexOf('synchronize_for_note_mutation(');
  const boundary = workflow.indexOf('if let Some(synchronized) = mutation_synchronization');
  const revoke = workflow.indexOf('begin_note_ui_revocation(target_note_id.clone())');
  const storage = workflow.indexOf('ApplicationExecutor::read("note action"');
  assert.ok(checkpoint >= 0 && boundary > checkpoint && revoke > boundary && storage > revoke);
  assert.match(workflow, /synchronized\.selection_generation/);
  assert.match(workflow, /synchronized\.target_selected/);
  assert.match(workflow, /capture_note_action_publication_token\(&target_note_id\)/);
});

test('unselected B becoming selected during its silent checkpoint is live-checkpointed before fail-closed B treatment', () => {
  const silentCheckpoint = mutationSync.indexOf('checkpoint_note_silent(&checkpoint_note_id)');
  const resample = mutationSync.indexOf('let target_selected = selected_note', silentCheckpoint);
  const retry = mutationSync.indexOf('if target_selected {', resample);
  const loopBack = mutationSync.indexOf('continue;', retry);
  const liveSync = mutationSync.indexOf('synchronize_current_editor(', mutationSync.indexOf('loop {'));
  assert.ok(silentCheckpoint >= 0 && resample > silentCheckpoint && retry > resample && loopBack > retry);
  assert.ok(liveSync >= 0, 'the retry target must have a live-editor checkpoint path');
  assertRustOrder(workflow, [
    'mutation_synchronization',
    'if selected_target && (protecting_note || locking_note || unlocking_note || removing_encryption)',
  ]);
});

test('selected Protect Lock Unlock and Remove Encryption discard the selected Note during the revocation window', () => {
  assertRustOrder(workflow, ['protecting_note || locking_note || unlocking_note || removing_encryption']);
  assert.match(workflow, /vault_transition\.set\(Some\(super::VaultTransitionUiState/);
  assert.match(workflow, /if selected_target && vault_visibility_transition[\s\S]*?selected_note\.set\(None\)/);
  assertRustOrder(noteCard, [
    'NoteCardAction::LockEncrypted(_) | NoteCardAction::UnlockEncrypted { .. } | NoteCardAction::RemoveEncryption { .. }',
    '"Encrypted note".to_string()',
  ]);
});

test('fresh B authorization is captured before canonical reread and its lease spans publication', () => {
  const capture = canonical.indexOf('wait_for_note_action_publication_token(&note_id)');
  const reread = canonical.indexOf('ApplicationExecutor::read("privacy action canonical reread"');
  const lease = canonical.indexOf('publication_token.begin_publication()');
  assert.ok(capture >= 0 && reread > capture && lease > reread);
  assert.match(workflow, /read_canonical_note_for_vault_publication\([\s\S]*?republish_selected_note_after_external_mutation/);
});

test('Vault completion cannot resurrect B after navigation or an unselected B action', () => {
  const guard = segment(noteCard, 'fn should_republish_selected_vault_target(', 'async fn read_canonical_note_for_vault_publication');
  assert.match(guard, /selected_target_at_revocation/);
  assert.match(guard, /note_selection_is_current\(selection_generation_at_revocation\)/);
  assert.match(guard, /vault_transition[\s\S]*?transition\.note_id == note_id/);
  assert.doesNotMatch(guard, /selected_note\.read/);
});

test('aggregate recovery is independent and centrally waits for fresh multi-note authority', () => {
  assert.match(aggregateLifetime, /wait_for_multi_note_publication_token\(None\)/);
  assert.match(aggregateLifetime, /token\.begin_publication\(\)/);
  assert.match(aggregateLifetime, /refresh_tags/);
  assert.match(aggregateLifetime, /reload_note_list\(library_state\)/);
  assert.doesNotMatch(workflow, /capture_multi_note_publication_token\([\s\S]{0,500}privacy action canonical reread/);
  assert.match(workflow, /centralized multi-note lifetime/);
});

test('selected B can navigate to C after its checkpoint without waiting for B aggregate revocation', () => {
  assert.match(uiMod, /fn selected_source_checkpointed_by_active_vault_transition[\s\S]*?selected_note\.peek\(\)\.is_none\(\)[\s\S]*?busy\.as_ref\(\) == Some\(&transition\.note_id\)/);
  assert.match(selection, /if !state\.selected_source_checkpointed_by_active_vault_transition\(\)[\s\S]*?synchronize_current_editor_when_privacy_ready/);
  assert.match(selection, /capture_note_publication_token\([\s\S]*?&note_id/);
});

test('selected B can create C immediately while aggregate list refresh remains independently gated', () => {
  assert.match(create, /if !state\.selected_source_checkpointed_by_active_vault_transition\(\)/);
  assert.match(create, /capture_note_publication_token\([\s\S]*?&target_note_id/);
  const selected = create.indexOf('selected_note.set(Some(note));');
  const focus = create.indexOf('focus_title_input();');
  const aggregateWait = create.indexOf('wait_for_multi_note_publication_token(Some(selection_generation))');
  assert.ok(selected >= 0 && focus > selected && aggregateWait > focus);
});

test('wrong-password and post-commit failure paths reconcile B under fresh note authority', () => {
  assert.match(workflow, /Err\(error\) => \{[\s\S]*?read_canonical_note_for_vault_publication\([\s\S]*?should_republish_selected_vault_target/);
  assert.match(noteCard, /Incorrect password\. The note remains locked\./);
  assert.match(noteCard, /Incorrect password\. Encryption was not removed\./);
  assert.match(workflow, /Aggregate reconciliation is owned by the centralized multi-note/);
});

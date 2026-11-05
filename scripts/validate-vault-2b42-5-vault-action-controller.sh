#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
import re

def read(path):
    return Path(path).read_text(errors='ignore')

def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f"{label} missing: {marker}")

def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f"{label} contains forbidden marker: {marker}")

def segment(text, start, end):
    i = text.find(start)
    if i < 0:
        raise SystemExit(f"missing segment start: {start}")
    j = text.find(end, i + len(start))
    if j < 0:
        raise SystemExit(f"missing segment end: {end}")
    return text[i:j]

note_card = read('src/ui/note_card.rs')
ui_mod = read('src/ui/mod.rs')
sync = read('src/ui/synchronization.rs')
list_data = read('src/ui/note_list_data.rs')
header = read('src/ui/notes_header.rs')
app = read('src/ui/app.rs')
protection = read('src/storage/protection.rs')
runner = read('scripts/run-browser-behavior-tests.mjs')

workflow = segment(note_card, 'pub(super) fn apply_note_card_action(', 'pub(super) fn apply_note_card_tag_action(')
canonical = segment(note_card, 'async fn read_canonical_note_for_vault_publication(', 'fn note_card_action_note_id')
mutation_sync = segment(sync, 'pub(super) async fn synchronize_for_note_mutation(', 'fn acquire_publication_lease')
selection = segment(list_data, 'pub(super) fn select_note', 'fn restore_note_list_scroll')
create = segment(header, 'pub(super) fn create_contextual_note', 'fn create_contextual_note_blocking')
aggregate = segment(app, 'let _aggregate_privacy_lifetime = use_effect', 'let _note_list_page_executor')

require(ui_mod, ['vault_action_busy: Signal<Option<NoteId>>'], 'Vault busy state')
forbid(ui_mod, ['vault_action_busy: Signal<bool>'], 'Vault busy state')
require(workflow, ['VaultActionBusyGuard::begin(state.vault_action_busy, &target_note_id)'], 'Vault action guard')
forbid(selection + create, ['vault_action_busy.peek()'], 'ordinary navigation/create')

forbid(workflow, [
    'vault_action_requires_selected_target',
    'apply_note_card_action_with_selection',
    'select_note(',
    'target_checkpointed_by_selection',
], 'forced target selection')
require(sync, ['pub(super) struct NoteMutationSynchronization'], 'note mutation synchronization boundary type')
require(mutation_sync, [
    'loop {',
    'let target_is_selected = selected_note',
    'SynchronizationMode::CheckpointSilent',
    'checkpoint_note_silent(&checkpoint_note_id)',
    'if target_selected {',
    'continue;',
    'selection_generation,',
    'target_selected,',
], 'selected/unselected B synchronization')

silent_checkpoint = mutation_sync.find('checkpoint_note_silent(&checkpoint_note_id)')
post_checkpoint_selection = mutation_sync.find('let target_selected = selected_note', silent_checkpoint)
retry_live = mutation_sync.find('if target_selected {', post_checkpoint_selection)
retry_continue = mutation_sync.find('continue;', retry_live)
if min(silent_checkpoint, post_checkpoint_selection, retry_live, retry_continue) < 0 or not (
    silent_checkpoint < post_checkpoint_selection < retry_live < retry_continue
):
    raise SystemExit('unselected B checkpoint does not upgrade to live-editor synchronization when B becomes selected')

checkpoint = workflow.find('synchronize_for_note_mutation(')
selection_snapshot = workflow.find('if let Some(synchronized) = mutation_synchronization')
revoke = workflow.find('begin_note_ui_revocation(target_note_id.clone())')
storage = workflow.find('ApplicationExecutor::read("note action"')
if min(checkpoint, selection_snapshot, revoke, storage) < 0 or not checkpoint < selection_snapshot < revoke < storage:
    raise SystemExit('Vault order is not stable B checkpoint -> returned selection boundary -> B UI revocation -> storage transition')
require(workflow, [
    'capture_note_action_publication_token(&target_note_id)',
    'synchronized.selection_generation',
    'synchronized.target_selected',
], 'pre-transition B authorization/boundary')

require(workflow, [
    'protecting_note || locking_note || unlocking_note || removing_encryption',
    'vault_transition.set(Some(super::VaultTransitionUiState',
    'if selected_target && vault_visibility_transition',
    'selected_note.set(None)',
], 'selected B fail-closed transition shell')
require(note_card, [
    'NoteCardAction::LockEncrypted(_) | NoteCardAction::UnlockEncrypted { .. } | NoteCardAction::RemoveEncryption { .. }',
    '"Encrypted note".to_string()',
], 'non-secret transition label')

capture = canonical.find('wait_for_note_action_publication_token(&note_id)')
reread = canonical.find('ApplicationExecutor::read("privacy action canonical reread"')
lease = canonical.find('publication_token.begin_publication()')
if min(capture, reread, lease) < 0 or not capture < reread < lease:
    raise SystemExit('canonical B flow is not fresh B auth -> reread -> B publication lease')
require(workflow, [
    'read_canonical_note_for_vault_publication(',
    'should_republish_selected_vault_target(',
    'republish_selected_note_after_external_mutation(',
], 'canonical selected-B publication')

guard = segment(note_card, 'fn should_republish_selected_vault_target(', 'async fn read_canonical_note_for_vault_publication')
require(guard, [
    'selected_target_at_revocation',
    'note_selection_is_current(selection_generation_at_revocation)',
    'transition.note_id == note_id',
], 'anti-resurrection guard')
forbid(guard, ['selected_note.read'], 'anti-resurrection guard')

require(ui_mod, [
    'fn selected_source_checkpointed_by_active_vault_transition',
    'self.selected_note.peek().is_none()',
    'busy.as_ref() == Some(&transition.note_id)',
], 'selected B escape marker')
require(selection, [
    'selected_source_checkpointed_by_active_vault_transition()',
    'capture_note_publication_token(',
    '&note_id',
], 'navigation during selected B transition')
require(create, [
    'selected_source_checkpointed_by_active_vault_transition()',
    'capture_note_publication_token(',
    '&target_note_id',
    'selected_note.set(Some(note))',
    'focus_title_input()',
    'wait_for_multi_note_publication_token(Some(selection_generation))',
], 'create during selected B transition')
if not create.find('focus_title_input()') < create.find('wait_for_multi_note_publication_token(Some(selection_generation))'):
    raise SystemExit('new note C is not made usable before aggregate refresh wait')

require(aggregate, [
    'wait_for_multi_note_publication_token(None)',
    'token.begin_publication()',
    'refresh_tags(&task_library, aggregate_tags, aggregate_status)',
    'reload_note_list(library_state)',
], 'independent aggregate recovery')
forbid(workflow, ['schedule_vault_action_aggregate_refresh'], 'duplicated aggregate refresh')
require(workflow, ['centralized multi-note lifetime'], 'Vault/aggregate ownership comment')

err = segment(workflow, 'Err(error) => {', 'clear_vault_transition_for_note(state, &target_note_id);')
require(err, [
    'read_canonical_note_for_vault_publication(',
    'should_republish_selected_vault_target(',
], 'Vault failure reconciliation')
require(note_card, [
    'Incorrect password. The note remains locked.',
    'Incorrect password. Encryption was not removed.',
], 'Vault authentication errors')

# Low-level storage remains the transition admission authority for all four Vault visibility operations.
require(protection, [
    'NotePrivacyTransition::Protect',
    'NotePrivacyTransition::Unlock',
    'NotePrivacyTransition::Lock',
    'NotePrivacyTransition::Unprotect',
], 'storage Vault transition admission')

require(runner, ['vault-action-privacy-contract.test.mjs'], 'browser behavior runner')

print('2b.42.5 Vault action controller validation passed.')
PY

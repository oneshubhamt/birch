import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {assertRustOrder} from './source-contract-helpers.mjs';

function read(path) {
  return readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');
}

const commands = read('editor-src/commands/index.js');
const core = read('editor-src/editor/core.js');
const interactions = read('editor-src/editor/interactions.js');
const listSystem = read('editor-src/editor/list_system.js');
const controller = read('editor-src/runtime/controller.js');
const attachments = read('editor-src/attachments/view.js');
const bridgeHost = read('editor-src/bridge/host.js');
const bridgeCommand = read('src/editor_bridge/command.rs');
const liveEditor = read('src/ui/live_editor.rs');
const rightPanel = read('src/ui/right_panel.rs');
const editor = read('src/ui/editor.rs');
const noteCard = read('src/ui/note_card.rs');
const noteListData = read('src/ui/note_list_data.rs');
const synchronization = read('src/ui/synchronization.rs');
const protection = read('src/storage/protection.rs');
const notesHeader = read('src/ui/notes_header.rs');
const uiMod = read('src/ui/mod.rs');
const css = read('assets/main.css');
const toolbar = read('src/ui/editor_toolbar.rs');
const toolbarSupport = read('src/ui/editor_toolbar_support.rs');
const schema = read('src/storage/schema.rs');
const responsive = read('src/ui/responsive.rs');

test('editor focus returns through ProseMirror selection authority', () => {
  assert.match(commands, /function focusWithoutScroll\(view\)\s*{[\s\S]*?view\.focus\(\);[\s\S]*?}/);
  assert.doesNotMatch(commands, /view\.dom\.focus\(/);
  assert.match(bridgeCommand, /FocusEditor/);
  assert.match(bridgeHost, /case 'focus_editor':\s*\n\s*return controller\?\.focusEditor\(\)/);
  assert.match(liveEditor, /live_editor_focus_script/);
  assert.match(rightPanel, /document::eval\(&live_editor_focus_script\(\)\)/);
});

test('title and body boundary navigation plus empty-title styling are explicit', () => {
  assert.match(editor, /matches!\(event\.key\(\), Key::Enter \| Key::ArrowDown\)/);
  assert.match(editor, /focus_live_editor\(\);/);
  assert.match(interactions, /function firstBodyTextblockPosition/);
  assert.match(interactions, /if \(position !== null\) return false/);
  assert.match(interactions, /function focusNoteTitleFromBody/);
  assert.match(interactions, /selectionOnFirstBodyVisualLine/);
  assert.match(interactions, /ArrowUp: focusNoteTitleFromBody/);
  assert.match(interactions, /Backspace: backspaceToNoteTitle/);
  assert.match(noteCard, /title_placeholder = note\.title\.trim\(\)\.is_empty\(\)/);
  assert.match(noteCard, /preview_placeholder\s*=\s*note\s*\.\s*preview_text\s*\.\s*split_whitespace\(\)\s*\.\s*next\(\)\s*\.\s*is_none\(\)\s*&&\s*!attachment_match/);
  assert.match(editor, /note-title-input placeholder/);
  assert.match(css, /\.note-title-input::placeholder\s*\{[\s\S]*?-webkit-text-fill-color:\s*var\(--muted\)[\s\S]*?font-weight:\s*400[\s\S]*?opacity:\s*\.48/);
  assert.match(css, /\.note-title-input\.placeholder\s*\{[\s\S]*?-webkit-text-fill-color:\s*var\(--muted\)[\s\S]*?font-weight:\s*400/);
  assert.match(css, /\.note-card-heading\.placeholder/);
  assert.match(css, /\.note-preview\.placeholder/);
});


test('Vault transitions keep navigation usable and expose only an explicit locked-note label', () => {
  const appVault = read('src/application/vault.rs');
  const appEditor = read('src/application/editor.rs');
  const protection = read('src/storage/protection.rs');
  const noteDomain = read('src/domain/note.rs');

  assert.match(noteCard, /span \{ "Locked-note label" \}/);
  assert.match(noteCard, /maxlength: "160"/);
  assert.match(noteCard, /Visible without the password\. Use a neutral label if the current title is sensitive\./);
  assert.match(noteCard, /public_label: vault_public_label\.read\(\)\.clone\(\)/);
  assert.match(noteCard, /let unlocking_note = matches!\(&action, NoteCardAction::UnlockEncrypted/);

  const actionWorkflow = noteCard.split('pub(super) fn apply_note_card_action(', 2)[1].split('pub(super) fn apply_note_card_tag_action', 1)[0];
  assert.match(actionWorkflow, /state\.spawn\(async move/);
  assert.doesNotMatch(actionWorkflow, /spawn_forever/);
  assert.match(uiMod, /vault_action_busy: Signal<Option<NoteId>>/);
  assert.match(actionWorkflow, /VaultActionBusyGuard::begin\(state\.vault_action_busy, &target_note_id\)/);
  assert.doesNotMatch(actionWorkflow, /vault_action_requires_selected_target|apply_note_card_action_with_selection|select_note\(/);
  assert.match(synchronization, /fn synchronize_for_note_mutation[\s\S]*?loop \{[\s\S]*?target_is_selected[\s\S]*?synchronize_current_editor[\s\S]*?checkpoint_note_silent\(&checkpoint_note_id\)[\s\S]*?if target_selected \{[\s\S]*?continue;/);
  assert.match(actionWorkflow, /capture_note_action_publication_token\(&target_note_id\)[\s\S]*?synchronize_for_note_mutation[\s\S]*?synchronized\.selection_generation[\s\S]*?synchronized\.target_selected[\s\S]*?begin_note_ui_revocation\(target_note_id\.clone\(\)\)[\s\S]*?ApplicationExecutor::read\("note action"/);
  assert.match(actionWorkflow, /if selected_target && vault_visibility_transition[\s\S]*?selected_note\.set\(None\)/);

  const canonical = noteCard.split('async fn read_canonical_note_for_vault_publication(', 2)[1].split('fn schedule_vault_action_aggregate_refresh', 1)[0];
  assert.match(canonical, /wait_for_note_action_publication_token\(&note_id\)[\s\S]*?privacy action canonical reread[\s\S]*?publication_token\.begin_publication\(\)/);
  assert.match(actionWorkflow, /read_canonical_note_for_vault_publication[\s\S]*?should_republish_selected_vault_target[\s\S]*?republish_selected_note_after_external_mutation/);
  assert.match(noteCard, /fn should_republish_selected_vault_target[\s\S]*?selected_target_at_revocation[\s\S]*?note_selection_is_current\(selection_generation_at_revocation\)/);
  assert.match(read('src/ui/app.rs'), /let _aggregate_privacy_lifetime = use_effect[\s\S]*?wait_for_multi_note_publication_token\(None\)[\s\S]*?reload_note_list\(library_state\)/);
  assert.match(actionWorkflow, /centralized multi-note lifetime/);

  assert.match(uiMod, /fn selected_source_checkpointed_by_active_vault_transition/);
  const selectionFlow = noteListData.split('pub(super) fn select_note', 2)[1].split('fn restore_note_list_scroll', 1)[0];
  assert.match(selectionFlow, /selected_source_checkpointed_by_active_vault_transition/);
  assert.doesNotMatch(selectionFlow, /vault_action_busy/);
  assert.match(notesHeader, /create_contextual_note[\s\S]*?selected_source_checkpointed_by_active_vault_transition[\s\S]*?selected_note\.set\(Some\(note\)\)[\s\S]*?focus_title_input\(\)[\s\S]*?wait_for_multi_note_publication_token/);

  assert.match(uiMod, /task_scope: ScopeId/);
  assert.match(uiMod, /Runtime::current\(\)\.spawn\(self\.task_scope, future\)/);
  assert.doesNotMatch(read('src/ui/sidebar.rs'), /spawn_forever/);
  assert.doesNotMatch(noteListData, /spawn_forever/);
  assert.doesNotMatch(notesHeader, /spawn_forever/);
  assert.doesNotMatch(noteCard, /spawn_forever/);
  assert.doesNotMatch(uiMod, /fn capture_selection_publication\(/);

  assert.match(noteCard, /fn vault_action_progress_message[\s\S]*?Encrypting note…[\s\S]*?Unlocking note…[\s\S]*?Locking note…[\s\S]*?Removing note encryption…/);
  const republish = noteCard.split('fn republish_selected_note_after_external_mutation(', 2)[1].split('type NoteCardActionOutcome', 1)[0];
  assert.match(uiMod, /note_selection_generation: Signal<u64>[\s\S]*?note_view_generation: Signal<u64>/);
  assert.match(uiMod, /fn advance_note_view_generation[\s\S]*?note_view_generation\.set\(generation\)/);
  assert.match(republish, /advance_note_view_generation\(\)/);
  assert.match(republish, /selected_note\.set\(Some\(note\)\)/);
  assert.doesNotMatch(republish, /note_selection_generation|begin_publication|selected_note\.set\(None\)|yield_now/);
  assert.match(editor, /key: "locked-\{note\.id\}-\{view_generation\}"/);
  assert.match(editor, /key: "note-\{note\.id\}-\{view_generation\}"/);
  assert.match(synchronization, /EDITOR_SESSION_STABILIZATION_ATTEMPTS: usize = 320/);
  assert.match(synchronization, /SessionChangedDuringSynchronization[\s\S]*?EDITOR_CAPTURE_RETRIES/);
  assert.match(uiMod, /struct VaultTransitionUiState[\s\S]*?note_id: NoteId[\s\S]*?public_label: String[\s\S]*?message: String/);
  assert.match(noteCard, /vault_transition\.set\(Some\(super::VaultTransitionUiState/);
  assertRustOrder(noteCard, [
    'NoteCardAction::LockEncrypted(_) | NoteCardAction::UnlockEncrypted { .. } | NoteCardAction::RemoveEncryption { .. }',
    '"Encrypted note".to_string()',
  ]);
  assert.match(noteCard, /fn clear_vault_transition_for_note/);
  assert.match(editor, /fn VaultTransitionPane[\s\S]*?aria_busy: "true"[\s\S]*?private note content has been removed from the interface/);
  assert.match(editor, /else if let Some\(transition\) = vault_transition\.read\(\)\.clone\(\)/);

  const hydrateLockedSummary = protection
    .split('pub(super) fn hydrate_protected_summary', 2)[1]
    .split('pub(super) fn protected_revision_summaries_if_unlocked', 1)[0];
  assert.match(hydrateLockedSummary, /summary\.protection = NoteProtectionState::Locked/);
  assert.doesNotMatch(hydrateLockedSummary, /summary\.title\.clear\(\)/);
  assert.match(hydrateLockedSummary, /summary\.preview_text\.clear\(\)[\s\S]*?summary\.tags\.clear\(\)[\s\S]*?summary\.attachment_names\.clear\(\)/);
  assertRustOrder(notesHeader, [
    'let selected_id = selected_note',
    '.map(|note| note.id.clone())',
    '.or_else(|| {',
    'vault_transition',
    '.read()',
    '.as_ref()',
    '.map(|transition| transition.note_id.clone())',
  ]);
  assert.match(editor, /h2 \{ "\{locked_label\}" \}/);
  assert.match(noteDomain, /if self\.protection\.is_locked\(\)[\s\S]*?"Locked note"/);

  assert.match(appVault, /worker barrier is transition-scoped, not a permanent lock-state flag/);
  assert.match(appVault, /unblock_note_after_vault\(note_id\)/);
  assert.match(appEditor, /fn checkpoint_note_silent[\s\S]*?note\.protection\.is_locked\(\)[\s\S]*?already satisfied/);
  assert.match(protection, /fn note_autosave_mutation_is_blocked/);
  assert.match(protection, /note_session_is_active\(note_id\)/);
  assert.match(protection, /fn normalize_locked_note_label/);
  assert.match(protection, /"title" => Ok\("title = \?4"\)/);
  assert.match(protection, /DELETE FROM notes_fts/);
  assert.match(noteCard, /Incorrect password\. The note remains locked\./);
  assert.match(noteCard, /Incorrect password\. Encryption was not removed\./);
  const toast = read('src/ui/toast.rs');
  assert.match(toast, /fn toast_heading[\s\S]*?Incorrect password/);
  assert.match(toast, /normalized\.starts_with\("incorrect password"\)/);
  assert.match(css, /\.toast\s*\{[\s\S]*?background:\s*var\(--panel\);/);
});

test('style control visibly follows block and list context', () => {
  assert.match(core, /h3: 'Heading 3'/);
  assert.match(core, /bullet_list: 'Bullet list'/);
  assert.match(core, /ordered_list: 'Numbered list'/);
  assert.match(core, /task_list: 'Checklist'/);
  assert.match(core, /visibleLabel\.textContent = currentLabel/);
});

test('image focus and edit keep only a layout placeholder in the editor', () => {
  assert.match(attachments, /setInlineImageFocusPlaceholder\(active\)/);
  assert.match(controller, /attachmentView\.setInlineImageFocusPlaceholder\(true\)/);
  assert.match(controller, /active\.setInlineImageFocusPlaceholder\(false\)/);
  assert.match(css, /\.prosemirror-image-node\.prosemirror-image-focus-placeholder\s*{\s*visibility: hidden;/);
});

test('list editing delegates structural behavior to native ProseMirror commands', () => {
  assert.match(listSystem, /import \{autoJoin, chainCommands, joinTextblockBackward\} from "prosemirror-commands"/);
  assert.match(listSystem, /import \{liftListItem, sinkListItem, splitListItem, wrapInList\} from "prosemirror-schema-list"/);
  assert.match(listSystem, /const splitNativeListItem = chainCommands\([\s\S]*?splitListItem\(schema\.nodes\.list_item\)[\s\S]*?splitListItem\(schema\.nodes\.task_item\)[\s\S]*?\);/);
  assert.match(listSystem, /const liftNativeListItem = chainCommands\([\s\S]*?liftListItem\(schema\.nodes\.list_item\)[\s\S]*?liftListItem\(schema\.nodes\.task_item\)[\s\S]*?\);/);
  assert.match(listSystem, /const sinkNativeListItem = chainCommands\([\s\S]*?sinkListItem\(schema\.nodes\.list_item\)[\s\S]*?sinkListItem\(schema\.nodes\.task_item\)[\s\S]*?\);/);
  assert.match(listSystem, /function backspaceListItemAtStart[\s\S]*?selection\.\$from\.parentOffset !== 0[\s\S]*?\$from\.index\(depth\) !== 0[\s\S]*?liftSelectedListItems/);
  assert.match(listSystem, /const backspaceListPolicy = chainCommands\(\s*backspaceListItemAtStart,\s*backspaceParagraphAfterList,\s*\);/);
  assert.match(listSystem, /function backspaceParagraphAfterList[\s\S]*?joinTextblockBackward[\s\S]*?autoJoin/);
  assert.match(listSystem, /function listKeymap\(\) \{\s*return keymap\(\{\s*Enter: splitNativeListItem,\s*Backspace: backspaceListPolicy,\s*Tab: sinkNativeListItem,\s*'Shift-Tab': liftSelectedListItems,\s*\}\);\s*\}/);
  assert.match(listSystem, /function normalizeExitedListParagraphIndents[\s\S]*?leftIndent: 0,[\s\S]*?rightIndent: 0,[\s\S]*?firstLineIndent: 0/);
  assert.match(listSystem, /function liftSelectedListItems[\s\S]*?liftNativeListItem[\s\S]*?normalizeExitedListParagraphIndents/);
  assert.match(listSystem, /function normalizeOrderedTrailingFragment[\s\S]*?orderedListAttrs\(metadata\.trailingOrder, metadata\.style, false\)/);
  assert.match(listSystem, /function wrapInListJoiningAdjacent[\s\S]*?autoJoin\(wrapInList\(targetListType, attrs\), listsCanAutoJoin\)/);
  assert.doesNotMatch(listSystem, /Delete|joinBackward|joinCompatibleListFragments|ReplaceAroundStep|canJoin|normalizeParagraphLayoutAfterListExit/);
  assert.doesNotMatch(interactions, /prosemirror-schema-list|backspaceCurrentListItemAtStart|liftOutermostListItemExactly|splitCurrentListItem/);
  assert.doesNotMatch(css, /\.prosemirror-list-ul \+ \.prosemirror-paragraph|\.prosemirror-paragraph \+ \.prosemirror-list-ul/);
  assert.match(core, /listKeymap\(\),[\s\S]*?keymap\(baseKeymap\)/);
  assert.match(core, /clearFirstLineIndentOnBackspace[\s\S]*?ancestor\.type === schema\.nodes\.list_item \|\| ancestor\.type === schema\.nodes\.task_item\) return false/);
  assert.match(commands, /case 'paragraph':[\s\S]*?commonSelectedListContext\(state\)[\s\S]*?liftSelectedListItems/);
  assert.match(commands, /toggleList\(state, dispatch, schema\.nodes\.bullet_list/);
  assert.match(interactions, /lineStartEmDashInputRule/);
  assert.match(interactions, /insertText\(EM_DASH_SEQUENCE_PREFIX, start, end\)/);
  assert.match(interactions, /continueEmDashSequence/);
  assert.match(interactions, /exitEmptyEmDashSequenceOnBackspace/);
  assert.match(core, /\/\^\\s\*\[\+\*\]\\s\$\//);
});

test('Enter on an atomic boundary creates durable ordinary blank paragraphs before generic Enter handling', () => {
  assert.match(interactions, /function createIntentionalBlankLineFromAtomicBoundary/);
  assert.match(interactions, /paragraph\.attrs\.boundary !== true/);
  assert.match(interactions, /Fragment\.fromArray\(\[blank, continuation\]\)/);
  assert.match(interactions, /paragraphPosition \+ blank\.nodeSize \+ 1/);
  assert.match(interactions, /function atomicBoundaryKeymap/);
  assert.match(core, /emDashSequenceKeymap\(\),\s*atomicBoundaryKeymap\(\),\s*editorBoundaryKeymap\(\)/);
});

test('title typography preserves native title selection while pointer-formatting', () => {
  assert.match(toolbar, /data-preserve-external-selection.*note-title-input/);
  assert.match(responsive, /target\?\.closest\?\.\('#editor-typography-trigger'\)/);
  assert.match(responsive, /keepExternalSelection/);
  assert.match(responsive, /document\.activeElement === preservedExternal/);
  assert.match(responsive, /currentSurface\.contains\(target\)[\s\S]*?target\?\.closest\?\.\('button'\)[\s\S]*?event\.preventDefault\(\)/);
  assert.match(css, /\.note-title-input::selection\s*\{[\s\S]*?background:\s*Highlight;[\s\S]*?-webkit-text-fill-color:\s*HighlightText/);
});

test('title typography is routed through a persisted title style instead of body commands', () => {
  assert.match(toolbar, /typography_targets_title/);
  assert.match(toolbarSupport, /TitleTypographyCommand::Toggle/);
  assert.match(toolbar, /TitleTypographyCommand::SetFontFamily/);
  assert.match(editor, /apply_title_typography/);
  assert.match(editor, /NoteChangeScope::Title/);
  assert.match(schema, /title_style\s+TEXT NOT NULL DEFAULT '\{\}'/);
});

test('paragraph popover stays compact while exposing canonical alignment and defaults', () => {
  const settingsUi = read('src/ui/settings.rs');
  const applicationSettings = read('src/application/settings.rs');
  const storageSettings = read('src/storage/settings.rs');
  const liveEditor = read('src/ui/live_editor.rs');
  const bridgeHost = read('editor-src/bridge/host.js');
  const documentSpec = read('editor-spec/document-spec.json');
  const portabilityHtml = read('src/portability/html.rs');

  assert.match(toolbar, /title: "Paragraph"/);
  assert.match(toolbar, /span \{ aria_hidden: "true", "¶" \}/);
  assert.match(toolbar, /h3 \{ "Alignment" \}/);
  for (const alignment of ['left', 'center', 'right', 'justify']) {
    assert.match(toolbar, new RegExp(`\\(\\"[^\\"]+\\", \\"${alignment}\\"\\)`));
  }
  assert.match(toolbar, /"First-line indent"/);
  assert.match(toolbar, /"Default first-line indent"/);
  assert.match(toolbar, /\[\("Auto", "default"\), \("1\.0", "1"\), \("1\.15", "1\.15"\), \("1\.5", "1\.5"\), \("1\.75", "1\.75"\), \("2\.0", "2"\)\]/);
  assert.match(toolbar, /"Drop cap"/);
  assert.match(toolbar, /"Reset paragraph formatting"/);
  assert.doesNotMatch(toolbar, /h3 \{ "Spelling" \}/);
  assert.doesNotMatch(toolbar, /span \{ "Spellcheck" \}/);

  assert.match(settingsUi, /strong \{ "Default paragraph line spacing" \}/);
  assert.match(settingsUi, /Applied to newly created normal paragraphs/);
  assert.match(settingsUi, /strong \{ "Spellcheck" \}/);
  assert.match(core, /function splitTextblockWithParagraphDefaults/);
  assert.match(core, /defaultLineSpacing: controller\.defaultLineSpacing/);
  assert.doesNotMatch(controller, /rustnote-default-line-spacing|applyDefaultLineSpacingState/);
  assert.doesNotMatch(css, /rustnote-default-line-spacing/);
  assert.match(applicationSettings, /enum EditorLineSpacing/);
  assert.match(storageSettings, /editor\.default_line_spacing/);
  assert.match(liveEditor, /default_line_spacing:\s*editor_preferences\s*\.\s*default_line_spacing\s*\.\s*map\(/);
  assert.match(bridgeHost, /config\.default_line_spacing/);
  assert.match(documentSpec, /"text_alignment"/);
  assert.match(documentSpec, /"textAlignment"/);
  assert.match(commands, /function setTextAlignment\(value\)/);
  assert.match(controller, /case 'text_alignment':/);
  assert.match(core, /data-live-editor-text-alignment/);
  assert.match(core, /setTypographyOptionState\(\s*menu,\s*'\[data-live-editor-line-spacing\]'/);
  assert.match(css, /\[data-text-alignment="justify"\] \{ text-align: justify; \}/);
  assert.match(portabilityHtml, /data-rustnote-text-alignment/);
});

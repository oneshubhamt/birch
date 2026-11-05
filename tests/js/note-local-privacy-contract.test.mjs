import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {assertRustContains} from './source-contract-helpers.mjs';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');

const privacy = read('src/ui/privacy.rs');
const editor = read('src/ui/editor.rs');
const properties = read('src/ui/note_properties.rs');
const sources = read('src/ui/sources.rs');
const metadata = read('src/ui/note_metadata.rs');
const rightPanel = read('src/ui/right_panel.rs');

function segment(text, start, end) {
  const a = text.indexOf(start);
  assert.notEqual(a, -1, `missing start marker: ${start}`);
  const b = text.indexOf(end, a + start.length);
  assert.notEqual(b, -1, `missing end marker: ${end}`);
  return text.slice(a, b);
}

test('note-local reactivity is derived from note identity, not aggregate generation', () => {
  assert.match(privacy, /use_note_privacy[\s\S]*?use_memo\(use_reactive!\(\|note_id\| epoch\.observe_note\(&note_id\)\)\)/);
  assert.match(properties, /use_note_privacy\(privacy_epoch, note\.id\.clone\(\)\)/);
  assert.match(sources, /use_note_privacy\(privacy_epoch, note_id\.clone\(\)\)/);
});

test('properties cache is cleared and reloaded through note-local privacy state', () => {
  const effect = segment(properties, 'let _context_effect = use_effect', 'let notebook_name =');
  assert.match(effect, /context_privacy\.read\(\)/);
  assert.match(effect, /capture_note_publication/);
  assert.match(effect, /context\.set\(NotePropertiesContext::default\(\)\)/);
  assert.doesNotMatch(effect, /list_generation|capture_multi_note/);
});

test('current-note sources clear plaintext state on same-note revocation', () => {
  const invalidator = segment(sources, 'let _local_privacy_invalidator = use_effect', 'let summaries_are_current');
  assert.match(invalidator, /revocation_active/);
  assert.match(invalidator, /summaries\.set\(Vec::new\(\)\)/);
  assert.match(invalidator, /summaries_publication_token\.set\(None\)/);
  assert.match(invalidator, /new_title\.set\(String::new\(\)\)/);
  assert.match(invalidator, /new_reference\.set\(String::new\(\)\)/);
  assert.match(invalidator, /citation_locator\.set\(String::new\(\)\)/);
  const loader = segment(sources, 'let _load_effect = use_effect', 'let search_notes =');
  assert.match(loader, /load_note_privacy\.read\(\)/);
  assert.match(loader, /capture_note_publication/);
});

test('revision plaintext preview cache is note-local and actively erased', () => {
  const revisions = segment(metadata, 'pub(super) fn RevisionHistoryPanel', 'fn schedule_revision_limit_change');
  assert.match(revisions, /use_note_privacy\(privacy_epoch, note_id\.clone\(\)\)/);
  assert.match(revisions, /preview_cache\.write\(\)\.clear\(\)/);
  assert.match(revisions, /preview_cache_order\.write\(\)\.clear\(\)/);
  assert.match(revisions, /preview_publication_token\.set\(None\)/);
  assert.match(revisions, /capture_note_publication/);
});

test('editor revocation removes note-derived in-memory and WebView plaintext', () => {
  const redaction = segment(editor, 'let _note_privacy_redaction_effect = use_effect', 'let initial_find_query =');
  assert.match(redaction, /editor_note_privacy\.read\(\)/);
  assert.match(redaction, /redact_note_for_privacy_transition/);
  assert.match(redaction, /privacy_tags_text\.set\(String::new\(\)\)/);
  assert.match(redaction, /privacy_attachments\.set\(Vec::new\(\)\)/);
  assert.match(redaction, /privacy_note_link_targets\.set\(Vec::new\(\)\)/);
  assert.match(redaction, /privacy_editor_popover\.set\(None\)/);
  assert.match(redaction, /privacy_right_panel_mode\.set\(None\)/);
  assert.match(redaction, /privacy_link_url\.set\(String::new\(\)\)/);
  assert.match(redaction, /live_editor_dispose_script\(false\)/);
  const helper = segment(editor, 'fn redact_note_for_privacy_transition', 'fn title_style_css');
  assert.match(helper, /title = "Encrypted note"/);
  assert.match(helper, /document = Default::default\(\)/);
  assert.match(helper, /tags\.clear\(\)/);
  assert.match(helper, /attachment_names\.clear\(\)/);
});

test('attachment UI authority is note-local and has no aggregate lifecycle field', () => {
  const attachments = segment(rightPanel, 'pub(super) struct AttachmentsPanelView', 'fn attachment_kind');
  assert.doesNotMatch(attachments, /library_generation|list_generation/);
  assert.match(attachments, /capture_note_publication[\s\S]*?&attachment\.note_id/);
});

test('note-local editor and metadata scopes bind the actual note identity', () => {
  assertRustContains(
    editor,
    'capture_note_publication(&attachment_registry_privacy, &attachment_registry_note_id, editor_generation)',
  );
  assertRustContains(
    editor,
    'capture_note_publication(&services.privacy(), &draft.peek().id, editor_generation)',
  );
  assert.match(metadata, /MetadataExportAction::Note\(_\) \| MetadataExportAction::NotePdf => context\.privacy_epoch[\s\S]*?capture_note_publication/);
  assert.match(metadata, /publication_scope: privacy_epoch\.note_publication_scope\([\s\S]*?draft\.peek\(\)\.id\.clone\(\)/);
});

test('note-local captures do not use list generation', () => {
  const combined = [editor, properties, sources, metadata, rightPanel].join('\n');
  assert.doesNotMatch(combined, /capture_note_publication\([^;]{0,240}list_generation/);
  assert.doesNotMatch(combined, /note_publication_scope\([^;]{0,240}list_generation/);
});

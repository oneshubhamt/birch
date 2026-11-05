import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');

const app = read('src/ui/app.rs');
const noteList = read('src/ui/note_list_data.rs');
const notesHeader = read('src/ui/notes_header.rs');
const filterPopover = read('src/ui/filter_popover.rs');
const backlinks = read('src/ui/note_links.rs');
const hierarchy = read('src/ui/note_hierarchy.rs');
const picker = read('src/ui/note_picker.rs');
const editor = read('src/ui/editor.rs');
const sources = read('src/ui/sources.rs');
const metadata = read('src/ui/note_metadata.rs');
const sidebar = read('src/ui/sidebar.rs');

function segment(text, start, end) {
  const a = text.indexOf(start);
  assert.notEqual(a, -1, `missing start marker: ${start}`);
  const b = text.indexOf(end, a + start.length);
  assert.notEqual(b, -1, `missing end marker: ${end}`);
  return text.slice(a, b);
}

test('aggregate list/tag caches are erased for active multi-note revocation and repopulated under fresh authority', () => {
  const lifetime = segment(app, 'let _aggregate_privacy_lifetime = use_effect', 'let _note_list_page_executor');
  assert.match(app, /let aggregate_privacy = use_memo\(move \|\| privacy_epoch\.observe_multi_note\(\)\)/);
  assert.match(lifetime, /aggregate_list_generation\.set\(next_generation\)/);
  assert.match(lifetime, /aggregate_list_page_request\.set\(None\)/);
  assert.match(lifetime, /aggregate_note_cache\.set\(NoteListCache::default\(\)\)/);
  assert.match(lifetime, /aggregate_page_start_cursors\.set\(vec!\[None\]\)/);
  assert.match(lifetime, /aggregate_tags\.set\(Vec::new\(\)\)/);
  assert.match(lifetime, /next_filters\.tags\.clear\(\)/);
  assert.doesNotMatch(lifetime, /selected_note\.set\(None\)/);
  assert.match(lifetime, /wait_for_multi_note_publication_token\(None\)/);
  assert.match(lifetime, /token\.begin_publication\(\)/);
  assert.match(lifetime, /refresh_tags/);
  assert.match(lifetime, /reload_note_list/);
});

test('All Notes/list rendering fails closed while aggregate revocation is active', () => {
  const panel = segment(notesHeader, 'pub(super) fn NotesListPanel', 'pub(super) fn create_contextual_note');
  assert.match(panel, /privacy_epoch\.observe_multi_note\(\)/);
  assert.match(panel, /if aggregate_revocation_active \{\s*Vec::new\(\)/);
  assert.match(panel, /let available_tags = if aggregate_revocation_active/);
  assert.match(panel, /if aggregate_revocation_active \{\s*return;\s*\}/);
  assert.match(noteList, /capture_multi_note_publication\(&state\.services\.peek\(\)\.privacy\(\), state\.list_generation\)/);
});

test('backlinks, hierarchy and note picker explicitly react to the multi-note privacy epoch', () => {
  for (const [name, text] of [['backlinks', backlinks], ['hierarchy', hierarchy], ['picker', picker]]) {
    assert.match(text, /use_memo\(move \|\| privacy_epoch\.observe_multi_note\(\)\)/, `${name} missing multi-note memo`);
    assert.match(text, /multi_note_privacy\.read\(\)/, `${name} effect does not observe aggregate privacy`);
    assert.match(text, /capture_multi_note_publication_with_lifecycle/, `${name} missing aggregate publication token`);
  }
});

test('editor note-link target registry starts empty and is populated/cleared only under multi-note authority', () => {
  const linkBlock = segment(editor, 'let mut note_link_targets = use_signal(Vec::<NoteLinkTarget>::new);', 'let revision_panel_operation');
  assert.doesNotMatch(editor, /initial_editor_service\.note_link_targets|initial_note_link_note_id/);
  assert.match(linkBlock, /use_memo\(move \|\| privacy_epoch\.observe_multi_note\(\)\)/);
  assert.match(linkBlock, /capture_multi_note_publication_with_lifecycle/);
  assert.match(linkBlock, /service\.note_link_targets\(&note_id\)/);
  assert.match(linkBlock, /note_link_targets\.set\(Vec::new\(\)\)/);
  assert.match(linkBlock, /live_editor_set_note_link_registry_script\(&\[\]\)/);
});

test('global source candidate search is aggregate while current-note source summaries remain note-local', () => {
  const candidateSearch = segment(sources, 'let search_notes =', 'let visible_summaries =');
  assert.match(candidateSearch, /multi_note_privacy\.read\(\)/);
  assert.match(candidateSearch, /capture_multi_note_publication_with_lifecycle/);
  assert.match(candidateSearch, /search_source_candidates/);
  const localLoad = segment(sources, 'let _load_effect = use_effect', 'let search_notes =');
  assert.match(localLoad, /capture_note_publication/);
  assert.doesNotMatch(localLoad, /capture_multi_note_publication/);
});

test('tag catalog mutation is protected by a multi-note publication lease', () => {
  const mutation = segment(metadata, 'fn mutate_catalog(&self, mutation: TagCatalogMutation)', '#[component]\nfn MetadataTagChip');
  assert.match(mutation, /capture_multi_note_publication\(&privacy, library_generation\)/);
  assert.match(mutation, /catalog_publication_token\.begin_publication\(\)/);
  assert.match(mutation, /execute_tag_command_blocking/);
  assert.match(mutation, /catalog_publication_token\.publish/);
});

test('search, sort and filter aggregate changes checkpoint source first, then acquire multi-note authority', () => {
  const combined = `${notesHeader}\n${filterPopover}`;
  assert.match(combined, /synchronize_then_begin_multi_note_publication_when_privacy_ready/);
  assert.doesNotMatch(combined, /capture_multi_note_publication_token\(\)[\s\S]{0,700}synchronize_current_editor\(/);
});

test('sidebar multi-note notebook mutation no longer uses one aggregate token to checkpoint the source editor', () => {
  const command = segment(sidebar, 'fn apply_notebook_command', '#[component]\npub(super) fn SidebarButton');
  assert.match(command, /synchronize_current_editor_when_privacy_ready/);
  assert.match(command, /wait_for_multi_note_publication_token/);
  assert.match(command, /publication_token\.begin_publication\(\)/);
  assert.doesNotMatch(command, /synchronize_current_editor\([\s\S]{0,350}publication_token\.clone\(\)/);
});

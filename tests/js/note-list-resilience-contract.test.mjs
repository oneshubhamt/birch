import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../../src/ui/note_list_data.rs', import.meta.url), 'utf8');

function functionBody(name) {
  const start = source.indexOf(`fn ${name}`);
  assert.notEqual(start, -1, `${name} must exist`);
  const next = source.indexOf('\npub(super) fn ', start + 1);
  return source.slice(start, next === -1 ? source.length : next);
}

test('reload keeps the last known-good note cache until replacement succeeds', () => {
  const reload = functionBody('reload_note_list');
  assert.doesNotMatch(reload, /note_cache\.set\(NoteListCache::default\(\)\)/);
  assert.doesNotMatch(reload, /page_start_cursors\.set\(vec!\[None\]\)/);
  assert.doesNotMatch(reload, /list_scroll_top\.set\(0\.0\)/);
  assert.match(reload, /NotePageLoadMode::Replace/);
  assert.match(source, /if request\.mode\.replaces_cache\(\) \{\s*state\.list_scroll_top\.set\(request\.restore_scroll_top\)/);
});

test('impossible empty replacement cannot erase a non-empty library view', () => {
  assert.match(source, /fn inconsistent_empty_replacement/);
  assert.match(source, /request\.page_index == 0/);
  assert.match(source, /page\.total_count\.is_some_and\(\|count\| count > 0\)/);
  assert.match(source, /rejected inconsistent replacement/);
  assert.match(source, /keeping the previous list/);
});

test('note-list failures preserve cache and emit diagnostic context', () => {
  assert.match(source, /\[rustnote\]\[note-list\] load failed generation=/);
  assert.match(source, /state\.note_cache\.peek\(\)\.len\(\)/);
  assert.match(source, /state\.status\.set\(format!\("Could not load notes:/);
});

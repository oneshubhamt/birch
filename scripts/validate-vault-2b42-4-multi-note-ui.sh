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

app = read('src/ui/app.rs')
list_data = read('src/ui/note_list_data.rs')
header = read('src/ui/notes_header.rs')
filter_popover = read('src/ui/filter_popover.rs')
links = read('src/ui/note_links.rs')
hierarchy = read('src/ui/note_hierarchy.rs')
picker = read('src/ui/note_picker.rs')
editor = read('src/ui/editor.rs')
sources = read('src/ui/sources.rs')
metadata = read('src/ui/note_metadata.rs')
sidebar = read('src/ui/sidebar.rs')
runner = read('scripts/run-browser-behavior-tests.mjs')

lifetime = segment(app, 'let _aggregate_privacy_lifetime = use_effect', 'let _note_list_page_executor')
require(app, ['let aggregate_privacy = use_memo(move || privacy_epoch.observe_multi_note());'], 'aggregate privacy observer')
require(lifetime, [
    'aggregate_list_generation.set(next_generation)',
    'aggregate_list_page_request.set(None)',
    'aggregate_note_cache.set(NoteListCache::default())',
    'aggregate_page_start_cursors.set(vec![None])',
    'aggregate_tags.set(Vec::new())',
    'next_filters.tags.clear()',
    'wait_for_multi_note_publication_token(None)',
    'token.begin_publication()',
    'refresh_tags(&task_library, aggregate_tags, aggregate_status)',
    'reload_note_list(library_state)',
], 'aggregate cache lifetime')
forbid(lifetime, ['selected_note.set(None)', 'editor_session.set(None)'], 'aggregate cache lifetime')

panel = segment(header, 'pub(super) fn NotesListPanel', 'pub(super) fn create_contextual_note')
require(panel, [
    'privacy_epoch.observe_multi_note()',
    'if aggregate_revocation_active {',
    'let available_tags = if aggregate_revocation_active',
], 'All Notes fail-closed rendering')
require(list_data, [
    'capture_multi_note_publication(&state.services.peek().privacy(), state.list_generation)',
], 'note-list page publication')

for name, text in [('backlinks', links), ('hierarchy', hierarchy), ('note picker', picker)]:
    require(text, [
        'use_memo(move || privacy_epoch.observe_multi_note())',
        'multi_note_privacy.read()',
        'capture_multi_note_publication_with_lifecycle',
    ], name)

link_registry = segment(editor, 'let mut note_link_targets = use_signal(Vec::<NoteLinkTarget>::new);', 'let revision_panel_operation')
require(link_registry, [
    'use_memo(move || privacy_epoch.observe_multi_note())',
    'capture_multi_note_publication_with_lifecycle',
    'service.note_link_targets(&note_id)',
    'note_link_targets.set(Vec::new())',
    'live_editor_set_note_link_registry_script(&[])',
], 'editor link-target registry')
forbid(editor, ['initial_editor_service.note_link_targets', 'initial_note_link_note_id'], 'unguarded editor link-target bootstrap')

candidate = segment(sources, 'let search_notes =', 'let visible_summaries =')
require(candidate, [
    'multi_note_privacy.read()',
    'capture_multi_note_publication_with_lifecycle',
    'search_source_candidates',
], 'global source candidates')

catalog = segment(metadata, 'fn mutate_catalog(&self, mutation: TagCatalogMutation)', '#[component]\nfn MetadataTagChip')
require(catalog, [
    'capture_multi_note_publication(&privacy, library_generation)',
    'catalog_publication_token.begin_publication()',
    'execute_tag_command_blocking',
    'catalog_publication_token.publish',
], 'tag catalog mutation')

combined_controls = header + '\n' + filter_popover
require(combined_controls, ['synchronize_then_begin_multi_note_publication_when_privacy_ready'], 'aggregate controls')
if re.search(r'capture_multi_note_publication_token\(\)[\s\S]{0,700}synchronize_current_editor\(', combined_controls):
    raise SystemExit('aggregate search/sort/filter still checkpoint source with aggregate token')

notebook_command = segment(sidebar, 'fn apply_notebook_command', '#[component]\npub(super) fn SidebarButton')
require(notebook_command, [
    'synchronize_current_editor_when_privacy_ready',
    'wait_for_multi_note_publication_token',
    'publication_token.begin_publication()',
], 'sidebar notebook mutation')
if re.search(r'synchronize_current_editor\([\s\S]{0,350}publication_token\.clone\(\)', notebook_command):
    raise SystemExit('sidebar notebook command still reuses aggregate token for source checkpoint')

require(runner, ['multi-note-privacy-contract.test.mjs'], 'browser behavior runner')

print('2b.42.4 multi-note UI validation passed.')
PY

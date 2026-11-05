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
    i=text.find(start)
    if i < 0: raise SystemExit(f"missing segment start: {start}")
    j=text.find(end, i+len(start))
    if j < 0: raise SystemExit(f"missing segment end: {end}")
    return text[i:j]

privacy=read('src/ui/privacy.rs')
editor=read('src/ui/editor.rs')
properties=read('src/ui/note_properties.rs')
sources=read('src/ui/sources.rs')
metadata=read('src/ui/note_metadata.rs')
right=read('src/ui/right_panel.rs')

require(privacy, [
    'pub(super) fn use_note_privacy(',
    'epoch.observe_note(&note_id)',
], 'note-local reactive privacy')

prop=segment(properties, 'pub(super) fn NotePropertiesPanel', '#[component]\nfn PropertyRow')
require(prop, ['use_note_privacy(privacy_epoch, note.id.clone())', 'context_privacy.read()', 'capture_note_publication(', 'context.set(NotePropertiesContext::default())'], 'properties note-local lifetime')
forbid(prop, ['capture_multi_note_publication', 'list_generation'], 'properties note-local lifetime')

src_load=segment(sources, 'let _local_privacy_invalidator = use_effect', 'let search_notes =')
require(src_load, ['revocation_active', 'summaries.set(Vec::new())', 'summaries_publication_token.set(None)', 'new_title.set(String::new())', 'new_reference.set(String::new())', 'citation_locator.set(String::new())', 'load_note_privacy.read()', 'capture_note_publication'], 'current-note sources lifetime')

revision=segment(metadata, 'pub(super) fn RevisionHistoryPanel', 'fn schedule_revision_limit_change')
require(revision, ['use_note_privacy(privacy_epoch, note_id.clone())', 'preview_cache.write().clear()', 'preview_cache_order.write().clear()', 'preview_publication_token.set(None)', 'capture_note_publication('], 'revision preview lifetime')

redaction=segment(editor, 'fn redact_note_for_privacy_transition', 'fn title_style_css')
require(redaction, ['"Encrypted note".to_string()', 'document = Default::default()', 'tags.clear()', 'attachment_names.clear()'], 'editor transition redaction')
editor_effect=segment(editor, 'let _note_privacy_redaction_effect = use_effect', 'let initial_find_query =')
require(editor_effect, ['editor_note_privacy.read()', 'privacy_draft.set(redacted)', 'privacy_tags_text.set(String::new())', 'privacy_attachments.set(Vec::new())', 'privacy_note_link_targets.set(Vec::new())', 'privacy_note_link_picker.set(None)', 'privacy_text_statistics.set(TextStatistics::default())', 'privacy_editor_popover.set(None)', 'privacy_right_panel_mode.set(None)', 'privacy_link_url.set(String::new())', 'privacy_link_label.set(String::new())', 'privacy_link_error.set(None)', 'live_editor_dispose_script(false)'], 'editor same-note revocation')

attachments=segment(right, 'pub(super) struct AttachmentsPanelView', 'fn attachment_kind')
forbid(attachments, ['library_generation', 'list_generation'], 'attachment note-local UI')
require(attachments, ['capture_note_publication(', '&attachment.note_id'], 'attachment note-local UI')

# No direct note-local publication may be coupled to list generation anywhere in UI.
combined='\n'.join([editor, properties, sources, metadata, right])
if re.search(r'capture_note_publication\([^;]{0,260}list_generation', combined, flags=re.S):
    raise SystemExit('note-local capture is coupled to list_generation')
if re.search(r'note_publication_scope\([^;]{0,260}list_generation', combined, flags=re.S):
    raise SystemExit('note-local publication scope is coupled to list_generation')

# Multi-note search/tag/registry code is deliberately left for 2b.42.4.
require(sources, ['capture_multi_note_publication_with_lifecycle'], 'deferred aggregate source search')
require(metadata, ['capture_multi_note_publication'], 'deferred aggregate metadata/tag work')
require(editor, ['capture_multi_note_publication_with_lifecycle'], 'deferred aggregate editor registry work')

print('2b.42.3 note-local UI validation passed.')
PY

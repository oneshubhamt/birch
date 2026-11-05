#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 - "$ROOT" <<'PY'
from pathlib import Path
import re, sys
root=Path(sys.argv[1])
read=lambda rel:(root/rel).read_text()
notes=read('src/ui/note_list_data.rs')
ui=read('src/ui/mod.rs')
header=read('src/ui/notes_header.rs')
sidebar=read('src/ui/sidebar.rs')
app_editor=read('src/application/editor.rs')
editor=read('src/ui/editor.rs')
sync=read('src/ui/synchronization.rs')
links=read('src/ui/note_links.rs')

errors=[]
def require(cond,msg):
    if not cond: errors.append(msg)

def between(text,start,end):
    a=text.find(start)
    if a < 0: return ''
    b=text.find(end,a+len(start))
    return text[a:] if b < 0 else text[a:b]

select=between(notes,'pub(super) fn select_note','fn restore_note_list_scroll')
require(select, 'select_note function missing')
if select:
    markers=[
        ('source synchronization','synchronize_current_editor_when_privacy_ready'),
        ('target authorization','capture_note_publication_token'),
        ('target load','worker.load_note(&worker_note_id)'),
        ('target freshness','if !target_token.is_publishable()'),
        ('target lease','target_token.begin_publication()'),
        ('target publication','state.selected_note.set(Some(note))'),
    ]
    positions=[]
    for label,marker in markers:
        pos=select.find(marker)
        require(pos>=0,f'select_note missing {label}')
        positions.append(pos)
    if all(p>=0 for p in positions):
        require(positions==sorted(positions),'select_note source/target operations are out of order')
    require('advance_note_selection_generation()' in select,'selection generation must advance before navigation')
    require('note_selection_is_current(selection_generation)' in select,'navigation must reject superseded selection')
    require('continue;' in select[select.find('if !target_token.is_publishable()'):select.find('target_token.begin_publication()')], 'stale target result must retry, not publish')

require('Replace(Option<NoteId>)' in notes,'list reconciliation must carry only target NoteId')
require('Replace(Option<Box<Note>>)' not in notes,'list reconciliation must not carry full target Note')
require('select_note(editor.clone(), target_note_id, state, |_| {})' in notes,'fallback target must re-enter scoped select_note path')

for label,text,start,end in [
    ('library section navigation',header,'pub(super) fn switch_library_section','pub(super) fn toggle_selected_pin'),
    ('sidebar navigation',sidebar,'fn schedule_sidebar_navigation','fn focus_notebook_create_input'),
]:
    body=between(text,start,end)
    require(body,f'{label} function missing')
    require('synchronize_then_begin_multi_note_publication_when_privacy_ready' in body,f'{label} must checkpoint source then acquire fresh aggregate authority')
    require('synchronize_and_begin_publication_when_privacy_ready' not in body,f'{label} must not reuse source-note publication authority')

helper=between(ui,'async fn synchronize_then_begin_multi_note_publication_when_privacy_ready','\n}')
require('synchronize_current_editor_when_privacy_ready' in helper,'two-stage navigation helper must checkpoint source')
require('wait_for_multi_note_publication_token' in helper,'two-stage navigation helper must acquire fresh aggregate token')
if helper:
    require(helper.find('synchronize_current_editor_when_privacy_ready') < helper.find('wait_for_multi_note_publication_token'),'source checkpoint must precede aggregate authority')
require('synchronize_and_begin_publication_when_privacy_ready' not in ui,'obsolete source-token publication helper must be removed')

require('pub(crate) fn switch_note(' not in app_editor,'combined EditorService::switch_note must remain removed')
require('NoteSwitchPersistence' not in app_editor,'combined note-switch persistence type must remain removed')
require('checkpoint_and_load_note_for_selection' not in app_editor,'combined checkpoint+target-load helper must remain removed')

require(re.search(r'onopen:[\s\S]{0,1200}?select_note\(',header) is not None,'note-picker/open navigation must use select_note')
require(re.search(r'ontags:[\s\S]{0,1200}?select_note\(',header) is not None,'tag/open navigation must use select_note')
require(re.search(r'onopennote:[\s\S]{0,1200}?select_note\(',editor) is not None,'editor note-link navigation must use select_note')
require(re.search(r'BacklinksPanelActions[\s\S]{0,500}?onopen: onopennote',editor) is not None,'backlinks must reuse target navigation callback')
require('selected_note.set' not in links,'note picker/link panel must not publish selected-note state directly')

source_helper=between(ui,'async fn synchronize_current_editor_when_privacy_ready','async fn synchronize_then_begin_multi_note_publication_when_privacy_ready')
require('None => self.privacy_epoch.capture_multi_note_publication_with_lifecycle' in source_helper,'no-source navigation must preserve aggregate pending-flush authority')
sync_body=between(sync,'pub(super) async fn synchronize_current_editor','const EDITOR_SESSION_STABILIZATION_ATTEMPTS')
require('flush_without_selected_note(service, mode).await' in sync_body,'no-source navigation must flush pending autosave work before target load')

create=between(header,'fn create_contextual_note(','fn create_contextual_note_blocking')
require('capture_note_publication_token(' in create and '&target_note_id' in create,'new-note target publication must acquire target-note authority')
require('wait_for_note_list_publication_token' not in header,'obsolete note-list publication wait helper must be absent')

if errors:
    print('2b.42.2 navigation source/target validation FAILED:')
    for e in errors: print(f' - {e}')
    raise SystemExit(1)
print('2b.42.2 navigation source/target validation passed.')
PY

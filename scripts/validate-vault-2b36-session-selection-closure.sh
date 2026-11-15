#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path
ui_mod=Path('src/ui/mod.rs').read_text(); app=Path('src/ui/app.rs').read_text(); editor=Path('src/ui/editor.rs').read_text(); card=Path('src/ui/note_card.rs').read_text(); note_list=Path('src/ui/note_list_data.rs').read_text(); sync=Path('src/ui/synchronization.rs').read_text(); sidebar=Path('src/ui/sidebar.rs').read_text(); protection=Path('src/storage/protection.rs').read_text()
assert 'note_selection_generation: Signal<u64>' in ui_mod and 'note_view_generation: Signal<u64>' in ui_mod
assert 'let note_view_generation = use_signal(|| 0_u64);' in app
assert 'let view_generation = *note_view_generation.read();' in editor
assert 'key: "note-{note.id}-{view_generation}"' in editor
assert 'key: "locked-{note.id}-{view_generation}"' in editor
select=note_list.split('pub(super) fn select_note',1)[1].split('fn restore_note_list_scroll',1)[0]
assert ('advance_note_selection_generation(state)' in select or 'state.advance_note_selection_generation()' in select)
assert 'advance_note_view_generation' not in select
rep=card.split('fn republish_selected_note_after_external_mutation(',1)[1].split('type NoteCardActionOutcome',1)[0]
assert 'advance_note_view_generation()' in rep and 'note_selection_generation' not in rep
assert 'stable_editor_session_for_selected_note' in sync
assert 'EDITOR_SESSION_STABILIZATION_ATTEMPTS' in sync and 'EDITOR_CAPTURE_RETRIES' in sync
assert 'SessionChangedDuringSynchronization' in sync
assert 'state.advance_note_view_generation();' in sidebar
assert 'VaultTransitionPane' in editor
assert 'begin_note_ui_revocation(target_note_id.clone())' in card
print('Rustnote 2b.36 session/selection objective guard passed under current scoped Vault model.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b35-selection-continuity-closure.sh
fi

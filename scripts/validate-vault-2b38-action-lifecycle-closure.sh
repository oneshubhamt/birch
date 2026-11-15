#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path
card = Path('src/ui/note_card.rs').read_text()
editor = Path('src/ui/editor.rs').read_text()
ui_mod = Path('src/ui/mod.rs').read_text()
notes = Path('src/ui/note_list_data.rs').read_text()
workflow = card.split('pub(super) fn apply_note_card_action(', 1)[1].split('pub(super) fn apply_note_card_tag_action(', 1)[0]
assert 'state.spawn(async move {' in workflow
assert 'spawn_forever' not in workflow
assert 'if selected_target && vault_visibility_transition' in workflow
assert 'selected_note.set(None);' in workflow
assert 'read_canonical_note_for_vault_publication(' in workflow
assert 'should_republish_selected_vault_target(' in workflow
assert 'republish_selected_note_after_external_mutation' in workflow
assert 'clear_vault_transition_for_note(state, &target_note_id);' in workflow
assert 'revocation_selection_generation' in workflow
assert 'centralized multi-note lifetime' in workflow
assert 'note_view_generation: Signal<u64>' in ui_mod
assert 'VaultTransitionPane' in editor
assert 'fn update_loaded_note(' in notes
canonical = card.split('async fn read_canonical_note_for_vault_publication(',1)[1].split('fn note_card_action_note_id',1)[0]
assert 'wait_for_note_action_publication_token(&note_id)' in canonical
assert 'privacy action canonical reread' in canonical
assert 'publication_token.begin_publication()' in canonical
print('Rustnote 2b.38 Vault action lifecycle objective guard passed under current scoped model.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b37-clippy-cleanup-closure.sh
fi

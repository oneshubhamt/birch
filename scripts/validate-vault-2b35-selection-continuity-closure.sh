#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path
note_card = Path('src/ui/note_card.rs').read_text()
editor = Path('src/ui/editor.rs').read_text()
notes_header = Path('src/ui/notes_header.rs').read_text()
ui_mod = Path('src/ui/mod.rs').read_text()
cargo = Path('Cargo.toml').read_text()
runner = Path('scripts/run-browser-behavior-tests.mjs').read_text()

assert 'fn republish_selected_note_after_external_mutation(' in note_card
republish = note_card.split('fn republish_selected_note_after_external_mutation(', 1)[1].split('type NoteCardActionOutcome', 1)[0]
assert 'state.advance_note_view_generation();' in republish
assert 'state.selected_note.set(Some(note));' in republish
assert 'clear_vault_transition_for_note(state, &note_id);' in republish
assert 'note_selection_generation' not in republish

workflow = note_card.split('pub(super) fn apply_note_card_action(', 1)[1].split('pub(super) fn apply_note_card_tag_action(', 1)[0]
assert 'begin_note_ui_revocation(target_note_id.clone())' in workflow
assert 'if selected_target && vault_visibility_transition' in workflow
assert 'selected_note.set(None);' in workflow
assert 'read_canonical_note_for_vault_publication(' in workflow
assert 'should_republish_selected_vault_target(' in workflow
assert 'republish_selected_note_after_external_mutation(' in workflow

assert 'struct VaultTransitionUiState' in ui_mod
assert 'vault_transition.set(Some(super::VaultTransitionUiState' in note_card
assert 'fn clear_vault_transition_for_note' in note_card
assert 'fn VaultTransitionPane' in editor
assert 'aria_busy: "true"' in editor
assert 'private note content has been removed from the interface' in editor
assert 'vault_transition.read().as_ref().map(|transition| transition.note_id.clone())' in notes_header
for message in ('Encrypting note…','Unlocking note…','Locking note…','Removing note encryption…'):
    assert message in note_card
assert '[profile.dev]' in cargo and 'debug = 0' in cargo
assert 'vault-action-privacy-contract.test.mjs' in runner
print('Rustnote 2b.35 selection-continuity objective guard passed under current scoped Vault model.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b34-runtime-dead-code-closure.sh
fi

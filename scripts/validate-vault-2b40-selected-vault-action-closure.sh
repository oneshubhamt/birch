#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
for required in NATIVE_COMPILE_2B40_PLAN.md NATIVE_COMPILE_2B40_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B40.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.40 closure file: $required" >&2; exit 1; }
done
python3 - <<'PY'
from pathlib import Path
card = Path('src/ui/note_card.rs').read_text(); privacy=Path('src/ui/privacy.rs').read_text(); sync=Path('src/ui/synchronization.rs').read_text(); ui=Path('src/ui/mod.rs').read_text(); runner=Path('scripts/run-browser-behavior-tests.mjs').read_text()
for marker in ('NoteCardAction::Encrypt { .. }','NoteCardAction::UnlockEncrypted { .. }','NoteCardAction::LockEncrypted(_)','NoteCardAction::RemoveEncryption { .. }'):
    assert marker in card
# 2b.42 supersedes forced target selection: actions operate directly on B.
for forbidden in ('fn vault_action_requires_selected_target','if vault_action_requires_selected_target(&action) && !target_is_selected','apply_note_card_action_with_selection(','target_checkpointed_by_selection'):
    assert forbidden not in card
assert 'VaultActionBusyGuard::begin(state.vault_action_busy, &target_note_id)' in card
assert 'capture_note_action_publication_token(&target_note_id)' in card
assert 'synchronize_for_note_mutation(' in card
assert 'SynchronizationMode::CheckpointSilent' in sync
assert 'vault_action_busy: Signal<Option<NoteId>>' in ui
assert 'capture_selection_publication' not in privacy
assert '#[allow(dead_code)]' not in privacy
assert 'vault-action-privacy-contract.test.mjs' in runner
print('Rustnote 2b.40 Vault-action ownership objective guard passed under current note-scoped model.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b39-privacy-navigation-toast-closure.sh
fi

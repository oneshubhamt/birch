#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path
ui = Path('src/ui/mod.rs').read_text(); notes=Path('src/ui/note_list_data.rs').read_text(); header=Path('src/ui/notes_header.rs').read_text(); sidebar=Path('src/ui/sidebar.rs').read_text(); sync=Path('src/ui/synchronization.rs').read_text(); card=Path('src/ui/note_card.rs').read_text(); toast=Path('src/ui/toast.rs').read_text(); css=Path('assets/main.css').read_text()
assert 'vault_action_busy: Signal<Option<NoteId>>' in ui
assert 'capture_note_action_publication_token' in ui
assert 'async fn wait_for_note_action_publication_token' in ui
assert 'async fn wait_for_multi_note_publication_token' in ui
assert 'capture_library_publication' not in ui
assert 'async fn synchronize_current_editor_when_privacy_ready' in ui
assert 'state.spawn(async move' in notes and 'spawn_forever' not in notes
assert 'capture_multi_note_publication(&state.services.peek().privacy(), state.list_generation)' in notes
assert 'synchronize_current_editor_when_privacy_ready' in header
assert 'wait_for_multi_note_publication_token' in sidebar
assert 'EDITOR_SESSION_STABILIZATION_ATTEMPTS: usize = 320' in sync
assert 'VaultActionBusyGuard::begin(state.vault_action_busy, &target_note_id)' in card
assert 'capture_note_action_publication_token(&target_note_id)' in card
assert 'Incorrect password. The note remains locked.' in card
assert 'Incorrect password. Encryption was not removed.' in card
assert 'normalized.starts_with("incorrect password")' in toast
assert 'background: var(--panel);' in css
assert 'background: color-mix(in srgb, var(--panel) 97%, transparent);' not in css
print('Rustnote 2b.39 privacy-navigation/toast objective guard passed under current scoped model.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b38-action-lifecycle-closure.sh
fi

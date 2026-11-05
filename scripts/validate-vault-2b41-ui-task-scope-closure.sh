#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
for required in NATIVE_COMPILE_2B41_PLAN.md NATIVE_COMPILE_2B41_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B41.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.41 closure file: $required" >&2; exit 1; }
done
python3 - <<'PY'
from pathlib import Path
app = Path('src/ui/app.rs').read_text(); ui=Path('src/ui/mod.rs').read_text(); card=Path('src/ui/note_card.rs').read_text(); notes=Path('src/ui/note_list_data.rs').read_text(); header=Path('src/ui/notes_header.rs').read_text(); sidebar=Path('src/ui/sidebar.rs').read_text(); sync=Path('src/ui/synchronization.rs').read_text(); all_ui='\n'.join(p.read_text() for p in Path('src/ui').glob('*.rs'))
assert 'let task_scope = dioxus::core::current_scope_id();' in app
assert 'task_scope,' in app
assert 'task_scope: ScopeId' in ui
assert 'Runtime::current().spawn(self.task_scope, future);' in ui
assert 'spawn_forever' not in all_ui
assert 'state.spawn(async move {' in card
assert 'state.spawn(async move {' in notes
assert header.count('state.spawn(async move {') >= 2
assert 'state.spawn(async move {' in sidebar
# Preserve the behavioral ownership objective under 2b.42's note-scoped controller.
assert 'vault_action_busy: Signal<Option<NoteId>>' in ui
assert 'VaultActionBusyGuard::begin(state.vault_action_busy, &target_note_id)' in card
assert 'synchronize_for_note_mutation(' in card
assert 'SynchronizationMode::CheckpointSilent' in sync
for forbidden in ('vault_action_requires_selected_target','target_checkpointed_by_selection'):
    assert forbidden not in card
print('Rustnote 2b.41 UI task-scope ownership guard passed under current note-scoped Vault model.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b40-selected-vault-action-closure.sh
fi

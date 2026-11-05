#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
for required in NATIVE_COMPILE_2B37_PLAN.md NATIVE_COMPILE_2B37_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B37.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.37 closure file: $required" >&2; exit 1; }
done
python3 - <<'PY'
from pathlib import Path
card = Path('src/ui/note_card.rs').read_text()
editor = Path('src/ui/editor.rs').read_text()
assert 'use super::privacy::PrivacyPublicationToken;' not in card
pane = editor.split('pub(super) fn EditorPane', 1)[1].split('let view_generation', 1)[0]
assert '\n        note_selection_generation,\n' not in pane
assert 'status_note_generation: library_state.note_selection_generation' in editor
assert 'note_view_generation,' in pane
print('Rustnote 2b.37 Clippy cleanup guard passed.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b36-session-selection-closure.sh
fi

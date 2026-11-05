#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# 2a.18's UI-lifetime objective is now implemented by the scoped 2b.42 model.
# Reuse the canonical current guards instead of preserving a duplicated snapshot
# of the superseded global-generation architecture.
bash scripts/validate-vault-2b42-3-note-local-ui.sh
bash scripts/validate-vault-2b42-4-multi-note-ui.sh

python3 - <<'PY'
from pathlib import Path

privacy = Path('src/ui/privacy.rs').read_text(errors='ignore')
coord = Path('src/security/privacy_coordinator.rs').read_text(errors='ignore')

required = [
    'library_generation: u64',
    'multi_note_generation: u64',
    'note_generations: HashMap<NoteId, u64>',
    'pub(super) fn observe_note(',
    'pub(super) fn observe_multi_note(',
    'pub(super) fn begin_note_ui_revocation(',
    'pub(super) fn begin_multi_note_ui_revocation(',
    'pub(super) fn begin_library_ui_revocation(',
    'pub(super) fn capture_note_publication(',
    'pub(super) fn capture_multi_note_publication(',
]
for marker in required:
    if marker not in privacy:
        raise SystemExit(f'2a.18 scoped UI lifetime missing: {marker}')

for marker in [
    'lock_b_leaves_note_a_publication_valid_and_undrained',
    'lock_b_invalidates_and_drains_note_b_publication',
    'lock_b_invalidates_and_drains_multi_note_publication',
    'library_restore_invalidates_and_drains_every_publication_scope',
]:
    if marker not in coord:
        raise SystemExit(f'2a.18 coordinator lifetime regression missing: {marker}')

print('E0-E3 2a.18 scoped UI lifetime guard passed.')
PY

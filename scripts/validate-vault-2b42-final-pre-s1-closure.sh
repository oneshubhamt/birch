#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "[2b.42.11] shell syntax"
while IFS= read -r -d '' file; do
  bash -n "$file"
done < <(find scripts -type f -name '*.sh' -print0)

echo "[2b.42.11] JS/module syntax"
while IFS= read -r -d '' file; do
  node --check "$file" >/dev/null
done < <(find . -type f \( -name '*.js' -o -name '*.mjs' \) -not -path './target/*' -print0)

echo "[2b.42.11] core inherited Vault contracts"
for script in \
  scripts/validate-vault-2b1-foundation.sh \
  scripts/validate-vault-2b2-privacy-authority.sh \
  scripts/validate-vault-2b3-publication-token.sh \
  scripts/validate-vault-2b4-granular-cutover.sh \
  scripts/validate-vault-2b5-consolidation.sh \
  scripts/validate-vault-2b6-independent-closure.sh \
  scripts/validate-vault-2b8-plaintext-authority-closure.sh \
  scripts/validate-vault-2b9-backup-ownership-closure.sh \
  scripts/validate-vault-2b10-cross-cutting-invariant-closure.sh \
  scripts/validate-vault-2b11-atomic-privacy-authority-closure.sh \
  scripts/validate-vault-2b12-aegis-privacy-disclosure-closure.sh; do
  bash "$script"
done

echo "[2b.42.11] inherited objectives superseded by scoped 2b.42 implementation"
for script in \
  scripts/validate-vault-2b13-native-compile-closure.sh \
  scripts/validate-vault-2b26-autosave-baseline-validator-closure.sh \
  scripts/validate-vault-2b31-native-full-qa-closure.sh \
  scripts/validate-vault-2b35-selection-continuity-closure.sh \
  scripts/validate-vault-2b36-session-selection-closure.sh \
  scripts/validate-vault-2b37-clippy-cleanup-closure.sh \
  scripts/validate-vault-2b38-action-lifecycle-closure.sh \
  scripts/validate-vault-2b39-privacy-navigation-toast-closure.sh \
  scripts/validate-vault-2b40-selected-vault-action-closure.sh \
  scripts/validate-vault-2b41-ui-task-scope-closure.sh; do
  RUSTNOTE_VALIDATOR_NO_CHAIN=1 bash "$script"
done

echo "[2b.42.11] scoped 2b.42 closure chain"
for script in \
  scripts/validate-vault-2b42-1-scoped-publication-core.sh \
  scripts/validate-vault-2b42-2-navigation-source-target.sh \
  scripts/validate-vault-2b42-3-note-local-ui.sh \
  scripts/validate-vault-2b42-4-multi-note-ui.sh \
  scripts/validate-vault-2b42-5-vault-action-controller.sh \
  scripts/validate-vault-2b42-6-library-wide-operations.sh \
  scripts/validate-vault-2b42-7-profiling.sh \
  scripts/validate-vault-2b42-7-native-compile-fix.sh \
  scripts/validate-vault-2b42-8-no-vacuum-forensic.sh \
  scripts/validate-vault-2b42-9-cleanup.sh \
  scripts/validate-vault-2b42-10-independent-closure.sh; do
  bash "$script"
done

echo "[2b.42.11] E0-E3 cumulative closure"
RUSTNOTE_E0_STATIC_ONLY=1 \
RUSTNOTE_E1_STATIC_ONLY=1 \
RUSTNOTE_E2_STATIC_ONLY=1 \
RUSTNOTE_E3_STATIC_ONLY=1 \
  bash scripts/validate-security-e0-e3-closure.sh

echo "[2b.42.11] final source sweeps"
python3 - <<'PY'
from pathlib import Path
import re

root = Path('.')
production = '\n'.join(p.read_text(errors='ignore') for p in (root/'src').rglob('*.rs'))
for marker in (
    '#![allow(dead_code)]',
    'ProtectedNotePayload',
    'capture_library_publication',
    'vault_action_busy: Signal<bool>',
    'vault_action_requires_selected_target',
    'target_checkpointed_by_selection',
):
    if marker in production:
        raise SystemExit(f'final pre-S1 source sweep found obsolete marker: {marker}')
for marker in ('TODO', 'FIXME', 'HACK'):
    if re.search(rf'\b{marker}\b', production):
        raise SystemExit(f'final pre-S1 production source retains {marker} marker')

# No temporary validator fragments or editor-build artifacts belong in source.
for path in root.rglob('*'):
    if path.name.startswith('.qa_current_'):
        raise SystemExit(f'temporary QA helper remains: {path}')

print('2b.42.11 final pre-S1 source sweeps passed.')
PY

echo "Vault 2b.42.11 final pre-S1 static closure passed. Native Rust + dependency-backed browser gates remain external release gates."

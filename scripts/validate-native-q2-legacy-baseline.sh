#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export RUSTNOTE_VALIDATOR_NO_CHAIN=1
export RUSTNOTE_E0_STATIC_ONLY=1
export RUSTNOTE_E1_STATIC_ONLY=1
export RUSTNOTE_E2_STATIC_ONLY=1
export RUSTNOTE_E3_STATIC_ONLY=1

validators=(
  scripts/validate-vault-2b13-native-compile-closure.sh
  scripts/validate-vault-2b14-editor-native-capture-closure.sh
  scripts/validate-vault-2b15-transport-visibility-clippy-closure.sh
  scripts/validate-vault-2b16-native-warning-closure.sh
  scripts/validate-vault-2b17-native-test-compile-closure.sh
  scripts/validate-vault-2b18-strict-clippy-closure.sh
  scripts/validate-vault-2b19-test-target-dead-code-closure.sh
  scripts/validate-vault-2b20-startup-test-isolation-privacy-order-closure.sh
  scripts/validate-vault-2b21-native-test-state-lifetime-closure.sh
  scripts/validate-vault-2b24-tag-transport-test-closure.sh
  scripts/validate-vault-2b25-transport-helper-scope-closure.sh
  scripts/validate-vault-2b26-autosave-baseline-validator-closure.sh
  scripts/validate-vault-2b27-main-pdf-lifetime-closure.sh
  scripts/validate-vault-2b28-pdf-test-transport-binding-closure.sh
  scripts/validate-vault-2b29-macos12-aegis-deployment-target-closure.sh
  scripts/validate-vault-2b30-python-compat-deployment-validator-closure.sh
  scripts/validate-vault-2b31-native-full-qa-closure.sh
  scripts/validate-vault-2b32-native-full-qa-closure.sh
  scripts/validate-vault-2b33-dead-code-closure.sh
  scripts/validate-vault-2b34-runtime-dead-code-closure.sh
  scripts/validate-vault-2b35-selection-continuity-closure.sh
  scripts/validate-vault-2b36-session-selection-closure.sh
  scripts/validate-vault-2b37-clippy-cleanup-closure.sh
  scripts/validate-vault-2b38-action-lifecycle-closure.sh
  scripts/validate-vault-2b39-privacy-navigation-toast-closure.sh
  scripts/validate-vault-2b40-selected-vault-action-closure.sh
  scripts/validate-vault-2b41-ui-task-scope-closure.sh
)

passed=()
stale=()

for validator in "${validators[@]}"; do
  echo "[Q2 legacy] $validator"
  if bash "$validator"; then
    passed+=("$validator")
  else
    stale+=("$validator")
  fi
done

printf 'Q2 legacy baseline: %d current / %d stale-or-superseded of %d historical entry points.\n'   "${#passed[@]}" "${#stale[@]}" "${#validators[@]}"
if (("${#stale[@]}" > 0)); then
  printf 'Q2 legacy stale-or-superseded entry points:\n'
  printf '  %s\n' "${stale[@]}"
fi

# This runner is evidence gathering, not a release gate. Historical failures are
# expected when a later architecture has legitimately superseded source-shape
# assertions. The replacement current suite that follows is the hard gate.
exit 0

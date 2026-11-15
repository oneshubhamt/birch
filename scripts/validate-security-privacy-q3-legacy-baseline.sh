#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export RUSTNOTE_E0_STATIC_ONLY=1
export RUSTNOTE_E1_STATIC_ONLY=1
export RUSTNOTE_E2_STATIC_ONLY=1
export RUSTNOTE_E3_STATIC_ONLY=1
export RUSTNOTE_VALIDATOR_NO_CHAIN=1

validators=(
  scripts/validate-security-e0.sh
  scripts/validate-security-e1.sh
  scripts/validate-security-e2.sh
  scripts/validate-security-e3.sh
  scripts/validate-security-e0-e3-ownership-2a16.sh
  scripts/validate-security-e0-e3-residue-2a17.sh
  scripts/validate-security-e0-e3-closure.sh
  scripts/validate-vault-2b1-foundation.sh
  scripts/validate-vault-2b8-plaintext-authority-closure.sh
  scripts/validate-vault-2b11-atomic-privacy-authority-closure.sh
  scripts/validate-vault-2b12-aegis-privacy-disclosure-closure.sh
  scripts/validate-vault-2b42-8-no-vacuum-forensic.sh
  scripts/validate-vault-2b42-9-cleanup.sh
)

passed=()
stale=()
for validator in "${validators[@]}"; do
  echo "[Q3 legacy] $validator"
  if bash "$validator"; then
    passed+=("$validator")
  else
    stale+=("$validator")
  fi
done

printf 'Q3 legacy baseline: %d current / %d stale-or-superseded of %d historical entry points.\n'   "${#passed[@]}" "${#stale[@]}" "${#validators[@]}"
if (("${#stale[@]}" > 0)); then
  printf 'Q3 legacy stale-or-superseded entry points:\n'
  printf '  %s\n' "${stale[@]}"
fi

# Evidence gathering only. Historical source-shape failures do not gate current
# architecture; the concern-based suite and native all-feature CI are hard gates.
exit 0

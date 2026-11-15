#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "[native-current] baseline/toolchain source contracts"
bash scripts/validate-dioxus-0.7.10-baseline.sh
bash scripts/validate-macos-deployment-target.sh

echo "[native-current] scoped privacy/publication architecture"
for validator in \
  scripts/validate-vault-2b42-1-scoped-publication-core.sh \
  scripts/validate-vault-2b42-2-navigation-source-target.sh \
  scripts/validate-vault-2b42-3-note-local-ui.sh \
  scripts/validate-vault-2b42-4-multi-note-ui.sh \
  scripts/validate-vault-2b42-5-vault-action-controller.sh \
  scripts/validate-vault-2b42-6-library-wide-operations.sh \
  scripts/validate-vault-2b42-7-native-compile-fix.sh \
  scripts/validate-vault-2b42-10-independent-closure.sh; do
  bash "$validator"
done

echo "[native-current] current privacy/UI source behavior contracts"
node --test \
  tests/js/navigation-publication-contract.test.mjs \
  tests/js/note-local-privacy-contract.test.mjs \
  tests/js/multi-note-privacy-contract.test.mjs \
  tests/js/library-wide-privacy-contract.test.mjs \
  tests/js/vault-action-privacy-contract.test.mjs \
  tests/js/note-list-resilience-contract.test.mjs

echo "Current native source/invariant suite passed."

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for suite in \
  scripts/qa-toolchain.sh \
  scripts/qa-editor-browser.sh \
  scripts/qa-security-privacy.sh \
  scripts/qa-backup-format.sh \
  scripts/qa-architecture.sh; do
  echo "[qa-release] $suite"
  bash "$suite"
done

echo "qa-release: all five concern suites passed."

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Current native source coverage is intentionally concern-based rather than
# historical-pass-based. Compilation/lints/tests are the native macOS CI job;
# this fast source-side gate covers pinned toolchain/deployment configuration
# plus the maintained current browser/privacy/storage contract corpus.
echo "[native-current] pinned toolchain and deployment contracts"
bash scripts/validate-dioxus-0.7.10-baseline.sh
bash scripts/validate-macos-deployment-target.sh

echo "[native-current] maintained current behavior/source contracts"
node scripts/run-browser-behavior-tests.mjs

echo "Current native source/invariant suite passed."

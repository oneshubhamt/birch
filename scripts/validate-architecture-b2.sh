#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "Checking B2 canonical editor-document specification authority..."
node scripts/validate-editor-document-spec.mjs
node scripts/validate-editor-document-contract.mjs

echo
echo "Checking first-party JavaScript and shell syntax..."
find editor-src -type f \( -name '*.js' -o -name '*.mjs' \) -not -path '*/vendor/*' -print0 \
  | xargs -0 -n1 node --check
find pdf-src -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
find scripts -type f -name '*.mjs' -print0 | xargs -0 -n1 node --check
find scripts -type f -name '*.sh' -print0 | xargs -0 -n1 zsh -n

echo
echo "Running cumulative architecture guards in static mode..."
RUSTNOTE_A1_STATIC_ONLY=1 ./scripts/validate-architecture-a1.sh
RUSTNOTE_A2_STATIC_ONLY=1 ./scripts/validate-architecture-a2.sh
RUSTNOTE_A3_STATIC_ONLY=1 ./scripts/validate-architecture-a3.sh
RUSTNOTE_A4_STATIC_ONLY=1 ./scripts/validate-architecture-a4.sh
RUSTNOTE_A14_CLOSURE_STATIC_ONLY=1 ./scripts/validate-architecture-a1-a4-closure.sh
RUSTNOTE_A5_STATIC_ONLY=1 ./scripts/validate-architecture-a5.sh
RUSTNOTE_A6_STATIC_ONLY=1 ./scripts/validate-architecture-a6.sh
RUSTNOTE_A7_STATIC_ONLY=1 ./scripts/validate-architecture-a7.sh
RUSTNOTE_B1_STATIC_ONLY=1 ./scripts/validate-architecture-b1.sh

if [[ "${RUSTNOTE_B2_STATIC_ONLY:-0}" == "1" ]]; then
  echo
  echo "Architecture B2 static validation passed (prepared-runtime/Rust QA skipped by RUSTNOTE_B2_STATIC_ONLY=1)."
  exit 0
fi

echo
echo "Running complete B1/A0-A7 acceptance suite..."
./scripts/validate-architecture-b1.sh

echo
echo "Rechecking the canonical document contract after the cumulative build/QA chain..."
node scripts/validate-editor-document-spec.mjs
node scripts/validate-editor-document-contract.mjs

echo
echo "Architecture B2 validation passed."

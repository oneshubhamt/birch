#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node scripts/validate-editor-document-spec.mjs
node scripts/validate-editor-document-contract.mjs

node --test \
  tests/js/editor-ui-contract.test.mjs \
  tests/js/note-list-resilience-contract.test.mjs \
  tests/js/pdf-behavior.test.mjs \
  tests/js/pdf-runtime-integrity.test.mjs \
  tests/js/pdf-layout-integrity.test.mjs

echo "qa-editor-browser: document authority and dependency-free editor/PDF behavior passed."

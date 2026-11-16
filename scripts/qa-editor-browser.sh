#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node scripts/validate-editor-document-spec.mjs
node scripts/validate-editor-document-contract.mjs

python3 - <<'PY'
from pathlib import Path

owners = {
    Path("src/editor_document_spec.rs"): (
        "canonical_document_spec_is_complete_and_self_consistent",
    ),
    Path("src/editor_document.rs"): (
        "bundled_editor_states_validate",
        "editor_state_json_round_trip_preserves_the_canonical_document",
        "invalid_contract_and_unsafe_content_are_rejected",
        "shared_editor_document_contract_corpus_matches_rust_validator",
    ),
}
missing = []
for path, symbols in owners.items():
    source = path.read_text(errors="ignore")
    for symbol in symbols:
        if f"fn {symbol}" not in source:
            missing.append(f"{path}: {symbol}")
if missing:
    raise SystemExit("editor Rust parity owner disappeared:\n- " + "\n- ".join(missing))
print("qa-editor-browser: JS/Rust canonical document owner traceability is present.")
PY

node --test \
  tests/js/editor-ui-contract.test.mjs \
  tests/js/note-list-resilience-contract.test.mjs \
  tests/js/pdf-behavior.test.mjs \
  tests/js/pdf-runtime-integrity.test.mjs \
  tests/js/pdf-layout-integrity.test.mjs

echo "qa-editor-browser: document authority and dependency-free editor/PDF behavior passed."

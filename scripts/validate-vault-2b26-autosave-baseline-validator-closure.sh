#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
import re
root=Path('.')
protection=(root/'src/storage/protection.rs').read_text()
history=(root/'src/storage/history.rs').read_text()
guard=(root/'scripts/validate-vault-2b23-attachment-roundtrip-lifetime-closure.sh').read_text()
e2=(root/'scripts/validate-security-e2.sh').read_text()

# The metadata-locality Autosave assertion must have a real recent revision
# baseline; otherwise the first Autosave is correctly due and appends one.
name='fn protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only()'
start=protection.index(name)
tail=protection[start:]
try:
    end=tail.index('\n    #[test]', len(name))
except ValueError:
    end=len(tail)
body=tail[:end]
for marker in (
    'let protected_note = storage',
    '.save_note_checkpoint_owned(protected_note)',
    'metadata-locality autosave test requires a recent revision baseline',
    'RevisionMode::Autosave',
    'autosave link mutation must not append a revision inside the autosave interval',
):
    assert marker in body, marker
seed=body.index('.save_note_checkpoint_owned(protected_note)')
before=body.index('let before_source = current_vault_graph_ids')
autosave=body.index('RevisionMode::Autosave')
assert seed < before < autosave

# Do not "fix" production by suppressing the first protected Autosave revision.
policy=protection[protection.index('let append_revision = if let Some(mode) = revision_mode'):]
policy=policy[:policy.index('let revision_recorded = append_revision.is_some();')]
assert '(RevisionMode::Autosave, Some(revision_head))' in policy
assert '_ => true' in policy
assert 'AUTOSAVE_REVISION_INTERVAL_SECONDS' in policy
assert 'latest.as_ref().is_some_and' in history

# The inherited resource-lifetime guard must be format tolerant and retain its
# semantic ordering checks.
for marker in ('import re', 're.search(', 're.S', 'read_match.start() < drop_match.start() < unprotect_match.start()'):
    assert marker in guard, marker
assert "read_marker='assert_eq!(fs::read" not in guard

# E2 must classify the current build on the retained schema-v21 architecture.
# Match the current build semantically instead of freezing an exact historical
# tuple, otherwise every later build extension falsely breaks this validator.
schema_branch=e2.split('schema_markers = [',1)[0]
assert 'def is_2b_at_least(minor: int)' in e2
assert 'if is_2b_at_least(4):' in e2
assert 'CURRENT_SCHEMA_VERSION: i64 = 21;' in e2

print('Rustnote 2b.26 autosave-baseline/validator closure guard passed.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then

  bash scripts/validate-vault-2b25-transport-helper-scope-closure.sh
fi

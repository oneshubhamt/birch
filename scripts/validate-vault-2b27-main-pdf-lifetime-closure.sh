#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
root=Path('.')
mod=(root/'src/storage/mod.rs').read_text()
protocol=(root/'src/attachment_protocol.rs').read_text()

helper='fn protected_note_accepts_vault_reads_for_test('
pos=mod.index(helper)
prefix=mod[max(0,pos-180):pos]
assert '#[cfg(any(test, feature = "internal-test-support"))]' in prefix
assert '#[doc(hidden)]' in prefix
assert 'pub fn protected_note_accepts_vault_reads_for_test(' in mod

for name, release_marker in (
    ('fn protected_pdf_annotation_guard_keeps_lock_waiting_until_response_scope_drops()', 'drop(record);'),
    ('fn protected_pdf_file_handle_keeps_lock_waiting_until_descriptor_drops()', 'drop(file);'),
):
    start=protocol.index(name)
    tail=protocol[start:]
    try:
        end=tail.index('\n    #[', len(name))
    except ValueError:
        end=len(tail)
    body=tail[:end]
    assert 'protected_note_accepts_vault_reads_for_test(&storage, &note_id)' in body, name
    assert 'transport.attachment_resource(' not in body, name
    assert 'done_rx.try_recv().is_err()' in body, name
    assert release_marker in body, (name, release_marker)
    assert 'recv_timeout(Duration::from_secs(15))' in body, name
    assert body.index('done_rx.try_recv().is_err()') < body.index(release_marker) < body.index('recv_timeout(Duration::from_secs(15))')

print('Rustnote 2b.27 main PDF lifetime closure guard passed.')
PY

bash scripts/validate-vault-2b26-autosave-baseline-validator-closure.sh

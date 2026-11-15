#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
protocol = Path('src/attachment_protocol.rs').read_text()

for name in (
    'fn protected_pdf_annotation_guard_keeps_lock_waiting_until_response_scope_drops()',
    'fn protected_pdf_file_handle_keeps_lock_waiting_until_descriptor_drops()',
):
    start = protocol.index(name)
    tail = protocol[start:]
    try:
        end = tail.index('\n    #[', len(name))
    except ValueError:
        end = len(tail)
    body = tail[:end]
    assert 'let (storage, transport, resource, note_id, _attachment_id, root) =' in body, name
    assert 'let (storage, _transport, resource, note_id, _attachment_id, root) =' not in body, name
    assert 'drop(transport);' in body, name
    assert 'drop(storage);' in body, name
    assert 'remove_dir_all(root)' in body, name
    assert body.index('drop(transport);') < body.index('drop(storage);') < body.index('remove_dir_all(root)'), name

print('Rustnote 2b.28 PDF test transport binding closure guard passed.')
PY

bash scripts/validate-vault-2b27-main-pdf-lifetime-closure.sh

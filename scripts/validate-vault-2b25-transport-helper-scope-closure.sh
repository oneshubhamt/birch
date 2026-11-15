#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
root=Path('.')
transport=(root/'src/storage/transport.rs').read_text()
storage=(root/'src/storage/mod.rs').read_text()

name='fn protected_pdf_transport_resource_keeps_lock_waiting_until_request_releases_it()'
start=transport.index(name)
tail=transport[start:]
try:
    end=tail.index('\n    #[test]', len(name))
except ValueError:
    end=len(tail)
body=tail[:end]
correct='crate::storage::protected_note_accepts_vault_reads_for_test(&storage, &note.id)'
wrong='super::protected_note_accepts_vault_reads_for_test(&storage, &note.id)'
assert correct in body, correct
assert wrong not in body, wrong
assert 'drop(resource);' in body
assert 'recv_timeout(Duration::from_secs(15))' in body

build_id=(root/'build-id.txt').read_text().strip()
if build_id in ('2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    helper='pub fn protected_note_accepts_vault_reads_for_test('
    pos=storage.index(helper)
    prefix=storage[max(0,pos-180):pos]
    assert '#[cfg(any(test, feature = "internal-test-support"))]' in prefix
    assert '#[doc(hidden)]' in prefix
else:
    helper='pub(crate) fn protected_note_accepts_vault_reads_for_test('
    pos=storage.index(helper)
    prefix=storage[max(0,pos-120):pos]
    assert '#[cfg(test)]' in prefix
    assert '#[cfg(any(test, feature = "internal-test-support"))]' not in prefix[-80:]

media=(root/'src/media_server.rs').read_text()
assert 'crate::storage::protected_note_accepts_vault_reads_for_test(&storage, &note_id)' in media
print('Rustnote 2b.25 transport test helper scope closure guard passed.')
PY

bash scripts/validate-vault-2b24-tag-transport-test-closure.sh

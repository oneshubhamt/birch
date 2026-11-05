#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

python3 - <<'PY'
from pathlib import Path
import re
src=(Path('src/storage/protection.rs')).read_text()
name='fn protected_note_encrypts_attachment_blob_and_restores_it_losslessly()'
start=src.index(name)
tail=src[start:]
try:
    end=tail.index('\n    #[test]', len(name))
except ValueError:
    end=len(tail)
body=tail[:end]
read_match=re.search(
    r'assert_eq!\(\s*fs::read\(&resource\.path\)\s*\.expect\("read materialized attachment"\),\s*plaintext\s*\);',
    body,
    re.S,
)
drop_match=re.search(r'drop\(resource\);', body)
unprotect_match=re.search(
    r'\.remove_note_protection_with_password\(\s*&note\.id,\s*b"correct horse battery staple"\s*\)',
    body,
    re.S,
)
assert read_match, 'materialized attachment verification is missing'
assert drop_match, 'protected attachment resource is not explicitly released'
assert unprotect_match, 'Unprotect call is missing'
assert read_match.start() < drop_match.start() < unprotect_match.start()
segment=body[drop_match.end():unprotect_match.start()]
assert '.protected_attachment_resource(' not in segment
assert 'VaultReadPermit' in body
print('Rustnote 2b.23 protected attachment roundtrip lifetime closure guard passed.')

PY

bash scripts/validate-vault-2b22-media-lock-validator-closure.sh

#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

for required in NATIVE_COMPILE_2B16_PLAN.md NATIVE_COMPILE_2B16_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B16.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.16 closure file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path
src=Path('src/attachment_protocol.rs').read_text()
start=src.index('fn serve_pdf_document(')
end=src.index('\nfn pdf_document_response(', start)
body=src[start:end]
assert 'let transport = library.transport_store();' not in body, 'stale unused transport local remains in serve_pdf_document'
assert 'pdf_attachment(library, attachment_id)' in body, 'PDF reads must continue through authorized transport lookup'
managed_start=src.index('fn managed_attachment_from_resource(')
managed_end=src.index('\n}', managed_start)+2
managed=src[managed_start:managed_end]
assert '_resource_guard: resource' in managed, 'managed attachment must retain transport authorization for request lifetime'
print('Rustnote 2b.16 native warning closure static guard passed.')
PY

./scripts/validate-vault-2b15-transport-visibility-clippy-closure.sh

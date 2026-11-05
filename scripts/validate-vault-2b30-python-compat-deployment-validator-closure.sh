#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
for required in NATIVE_COMPILE_2B30_PLAN.md NATIVE_COMPILE_2B30_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B30.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.30 closure file: $required" >&2; exit 1; }
done
./scripts/validate-macos-deployment-target.sh
python3 - <<'PY'
from pathlib import Path
validator = Path('scripts/validate-macos-deployment-target.sh').read_text()
assert 'import tomllib' not in validator
assert 'import re' in validator
assert 'MACOSX_DEPLOYMENT_TARGET' in validator
assert '12\\.0' in validator
print('Rustnote 2b.30 Python-compatible deployment validator closure guard passed.')
PY
bash scripts/validate-vault-2b29-macos12-aegis-deployment-target-closure.sh

#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


./scripts/validate-macos-deployment-target.sh

python3 - <<'PY'
from pathlib import Path

binary_validator = Path('scripts/validate-macos-binary-deployment-target.sh').read_text()
for needle in ('LC_BUILD_VERSION', 'LC_VERSION_MIN_MACOSX', '12.0|12.0.0', 'otool -l "$BINARY"'):
    assert needle in binary_validator, needle

bundle_validator = Path('scripts/validate-rustnote-macos-app.sh').read_text()
assert "Print :LSMinimumSystemVersion" in bundle_validator
assert './scripts/validate-macos-binary-deployment-target.sh "$EXECUTABLE"' in bundle_validator

prepare = Path('scripts/prepare-rustnote-mac.sh').read_text()
bundle = Path('scripts/build-rustnote-macos-app.sh').read_text()
assert './scripts/validate-macos-binary-deployment-target.sh "$ROOT_DIR/target/release/rustnote"' in prepare
for name, text in [('prepare', prepare), ('bundle', bundle)]:
    assert './scripts/validate-macos-deployment-target.sh' in text, name

print('Rustnote 2b.29 macOS 12 / AEGIS deployment-target closure guard passed.')
PY

bash scripts/validate-vault-2b28-pdf-test-transport-binding-closure.sh

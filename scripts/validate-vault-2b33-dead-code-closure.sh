#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
for required in NATIVE_COMPILE_2B33_PLAN.md NATIVE_COMPILE_2B33_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B33.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.33 closure file: $required" >&2; exit 1; }
done
python3 - <<'PY'
from pathlib import Path
protection = Path('src/storage/protection.rs').read_text()
storage_mod = Path('src/storage/mod.rs').read_text()

wrapper_marker = '''    #[cfg(test)]\n    pub(crate) fn protect_note_with_password(\n'''
assert wrapper_marker in protection
wrapper = protection.split(wrapper_marker, 1)[1].split('    pub(crate) fn protect_note_with_password_and_public_label(', 1)[0]
assert 'protect_note_with_password_and_public_label(note_id, password, "Locked note")' in wrapper
assert '#[allow(dead_code)]' not in wrapper

fixture = storage_mod.split('pub fn protected_transport_test_fixture(', 1)[1].split('pub fn unix_timestamp()', 1)[0]
assert '.protect_note_with_password_and_public_label(' in fixture
assert '.protect_note_with_password(' not in fixture
assert '"Locked note"' in fixture

label_aware = protection.split('    pub(crate) fn protect_note_with_password_and_public_label(', 1)[0]
assert label_aware.rstrip().endswith('}')

print('Rustnote 2b.33 dead-code closure guard passed.')
PY
bash scripts/validate-vault-2b32-native-full-qa-closure.sh

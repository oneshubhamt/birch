#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
root = Path('.')
plain = (root/'src/storage/plaintext_staging.rs').read_text()
notes = (root/'src/storage/notes.rs').read_text()
protection = (root/'src/storage/protection.rs').read_text()
storage_mod = (root/'src/storage/mod.rs').read_text()
protocol = (root/'src/attachment_protocol.rs').read_text()
manifest = (root/'Cargo.toml').read_text()

assert 'Storage::at_with_backup(&data, &backups)' not in plain
assert plain.count('Storage::at_with_backup(data.clone(), backups.clone())') >= 6

assert '#[cfg(test)]\n    pub(super) fn save_note_owned_with_revision_status_for_test(' in notes
assert '\n    fn save_note_owned_with_revision_status(\n' in notes
assert '.save_note_owned_with_revision_status(' not in protection
assert '.save_note_owned_with_revision_status_for_test(' in protection

assert '.allocate_attachment_path()' not in protection
assert '.allocate_vault_plaintext_path()' in protection

assert 'internal-test-support = []' in manifest
assert '#[cfg(any(test, feature = "internal-test-support"))]\n#[doc(hidden)]\npub fn protected_transport_test_fixture(' in storage_mod
assert 'crate::storage::protected_transport_test_fixture(' not in protocol
assert protocol.count('rustnote::storage::protected_transport_test_fixture(') == 2
assert protocol.count('#[cfg(feature = "internal-test-support")]') >= 2

# Do not widen the real protected-note mutation/storage helpers just for tests.
assert 'pub(crate) fn protect_note_with_password(' in (root/'src/storage/protection.rs').read_text()
assert 'pub fn protect_note_with_password(' not in (root/'src/storage/protection.rs').read_text()

print('Rustnote 2b.17 native test-compile closure static guard passed.')
PY

./scripts/validate-vault-2b16-native-warning-closure.sh

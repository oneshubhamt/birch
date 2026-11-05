#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

python3 - <<'PY'
from pathlib import Path
root=Path('.')
media=(root/'src/media_server.rs').read_text()
storage=(root/'src/storage/mod.rs').read_text()
validator=(root/'scripts/validate-vault-2b21-native-test-state-lifetime-closure.sh').read_text()

helper='protected_note_accepts_vault_reads_for_test'
build_id=(root/'build-id.txt').read_text().strip()
prefix=storage[max(0,storage.index(helper)-180):storage.index(helper)]
if build_id in ('2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    assert '#[cfg(any(test, feature = "internal-test-support"))]' in prefix
    assert '#[doc(hidden)]' in prefix
else:
    assert '#[cfg(test)]' in prefix
assert 'privacy_coordinator' in storage[storage.index(helper):storage.index(helper)+500]
assert '.contains_note_key(note_id)' in storage[storage.index(helper):storage.index(helper)+500]

start=media.index('fn protected_media_file_keeps_lock_waiting_until_file_handle_drops()')
tail=media[start:]
end=tail.index('\n    #[test]', 10)
body=tail[:end]
assert helper in body
assert 'transport.attachment_resource(&attachment_id)' not in body
assert 'drop(leased_file);' in body
assert 'recv_timeout(Duration::from_secs(15))' in body
assert body.index(helper) < body.index('drop(leased_file);')
assert body.index('done_rx.try_recv().is_err()') < body.index('drop(leased_file);')

assert "compact_restore = ''.join(restore.split())" in validator
assert "lock_marker = 'storage.lock_protected_note(&note.id)'" in validator
print('Rustnote 2b.22 media Lock test + validator closure guard passed.')
PY

bash scripts/validate-vault-2b21-native-test-state-lifetime-closure.sh

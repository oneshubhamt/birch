#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path
runtime = Path('src/security/runtime.rs').read_text()
coordinator = Path('src/security/privacy_coordinator.rs').read_text()
protection = Path('src/storage/protection.rs').read_text()
ui_contract = Path('tests/js/editor-ui-contract.test.mjs').read_text()

runtime_semantic = runtime.split('pub(crate) fn note_session_is_active(', 1)[1].split('pub(crate) fn contains_note_key(', 1)[0]
assert '.is_some_and(|session| !session.revoking)' in runtime_semantic
expected_runtime_cfg = '#[cfg(test)]' if Path('build-id.txt').read_text().strip() in ('2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') else '#[cfg(any(test, feature = "internal-test-support"))]'
assert f'{expected_runtime_cfg}\n    pub(crate) fn contains_note_key(' in runtime
runtime_test_helper = runtime.split('pub(crate) fn contains_note_key(', 1)[1].split('#[cfg(test)]', 1)[0]
assert 'self.note_session_is_active(note_id)' in runtime_test_helper

assert 'pub(crate) fn note_session_is_active(&self, note_id: &NoteId)' in coordinator
coordinator_semantic = coordinator.split('pub(crate) fn note_session_is_active(', 1)[1].split('#[cfg(any(test, feature = "internal-test-support"))]', 1)[0]
assert 'self.inner.vault_runtime.note_session_is_active(note_id)' in coordinator_semantic
assert '#[cfg(any(test, feature = \"internal-test-support\"))]\n    pub(crate) fn contains_note_key(' in coordinator

autosave_admission = protection.split('pub(crate) fn note_autosave_mutation_is_blocked(', 1)[1].split('pub(crate) fn note_requires_vault_autosave_barrier(', 1)[0]
assert '.note_session_is_active(note_id)' in autosave_admission
assert '.contains_note_key(note_id)' not in autosave_admission
assert 'note_has_pending_vault_operation(note_id)?' in autosave_admission
assert 'protected_note_exists(&connection, note_id)?' in autosave_admission
assert r'note_session_is_active\(note_id\)' in ui_contract
assert r'contains_note_key\(note_id\)' not in ui_contract

print('Rustnote 2b.32 Vault session-query compile closure guard passed.')
PY
bash scripts/validate-vault-2b31-native-full-qa-closure.sh

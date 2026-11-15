#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
python3 - <<'PY'
from pathlib import Path
runtime = Path('src/security/runtime.rs').read_text()
coordinator = Path('src/security/privacy_coordinator.rs').read_text()
storage_mod = Path('src/storage/mod.rs').read_text()
protection = Path('src/storage/protection.rs').read_text()

runtime_helper = '''    #[cfg(test)]\n    pub(crate) fn contains_note_key(&self, note_id: &NoteId) -> Result<bool, &'static str> {\n        self.note_session_is_active(note_id)\n    }\n'''
assert runtime_helper in runtime
assert '#[cfg(any(test, feature = "internal-test-support"))]\n    pub(crate) fn contains_note_key(' not in runtime
assert '#[allow(dead_code)]' not in runtime[runtime.index('pub(crate) fn note_session_is_active'):runtime.index('pub(crate) fn active_permit_count')]
assert '#[expect(dead_code)]' not in runtime[runtime.index('pub(crate) fn note_session_is_active'):runtime.index('pub(crate) fn active_permit_count')]

# Production autosave stays on the semantic session query.
autosave = protection.split('pub(crate) fn note_autosave_mutation_is_blocked(', 1)[1].split('pub(crate) fn note_requires_vault_autosave_barrier(', 1)[0]
assert '.note_session_is_active(note_id)' in autosave
assert '.contains_note_key(note_id)' not in autosave

# The coordinator façade remains available to internal-test-support and delegates
# to the semantic query rather than the raw runtime compatibility helper.
assert '#[cfg(any(test, feature = "internal-test-support"))]\n    pub(crate) fn contains_note_key(' in coordinator
coord_helper = coordinator.split('pub(crate) fn contains_note_key(', 1)[1].split('#[cfg(test)]', 1)[0]
assert 'self.note_session_is_active(note_id)' in coord_helper
assert 'vault_runtime.contains_note_key' not in coord_helper

fixture = storage_mod.split('pub fn protected_note_accepts_vault_reads_for_test(', 1)[1].split('#[cfg(any(test, feature = "internal-test-support"))]', 1)[0]
assert '.privacy_coordinator' in fixture
assert '.contains_note_key(note_id)' in fixture

# Every direct call to the raw runtime helper remains inside the unit-test module.
runtime_production = runtime.split('#[cfg(test)]\nmod tests {', 1)[0]
assert '.contains_note_key(' not in runtime_production

print('Rustnote 2b.34 Vault runtime dead-code closure guard passed.')
PY
bash scripts/validate-vault-2b33-dead-code-closure.sh

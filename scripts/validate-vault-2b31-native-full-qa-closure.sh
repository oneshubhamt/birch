#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
for required in NATIVE_COMPILE_2B31_PLAN.md NATIVE_COMPILE_2B31_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B31.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.31 closure file: $required" >&2; exit 1; }
done
python3 - <<'PY'
from pathlib import Path
privacy = Path('src/security/privacy_coordinator.rs').read_text()
architecture = Path('scripts/validate-architecture-a0.sh').read_text()
behavior = Path('tests/js/editor-prosemirror-behavior.test.mjs').read_text()
locations = Path('src/storage/locations.rs').read_text()
autosave = Path('src/autosave.rs').read_text()
protection = Path('src/storage/protection.rs').read_text()
note_card = Path('src/ui/note_card.rs').read_text()
transport = Path('src/editor_bridge/transport.rs').read_text()
app_vault = Path('src/application/vault.rs').read_text()
app_editor = Path('src/application/editor.rs').read_text()
ui_mod = Path('src/ui/mod.rs').read_text()

# Current PrivacyCoordinator is scoped rather than using the old global
# generation-advance polling helper retained by the original 2b.31 validator.
for marker in ('begin_note_transition', 'begin_library_transition', 'capture_note_publication', 'capture_multi_note_publication'):
    assert marker in privacy, marker
assert 'capture_library_publication' not in ui_mod

normalizer = locations.split('fn normalize_directory_for_comparison(path: &Path)', 1)[1].split('fn lexical_normalize(path: &Path)', 1)[0]
assert 'fs::canonicalize(&existing)' in normalizer
assert 'error.kind() == std::io::ErrorKind::NotFound' in normalizer
assert 'missing.push(name);' in normalizer and 'existing.pop()' in normalizer

assert './scripts/validate-dioxus-0.7.10-baseline.sh' in architecture
assert 'cargo test --workspace --locked' not in architecture
assert 'Full Rust behavior is validated separately by the serial workspace test gate.' in architecture
for marker in (
    'stale atomic-boundary markers at list edges are normalized to ordinary paragraphs',
    'a demoted stale marker must follow ordinary blank-paragraph Backspace semantics',
    'a demoted stale marker must follow ordinary blank-paragraph Delete semantics',
    'intentional blank lines must survive boundary normalization',
):
    assert marker in behavior, marker

assert 'pub(crate) fn note_has_pending_vault_operation(' in protection
assert 'unlocked protected notes autosave through the' in protection
assert 'vault_autosave_is_blocked(&storage, &vault_blocked_notes' in autosave
assert 'note_autosave_mutation_is_blocked(note_id)' in autosave
assert 'unlocked_protected_note_autosaves_and_close_checkpoints_without_plaintext_journal' in autosave

workflow = note_card.split('pub(super) fn apply_note_card_action(',1)[1].split('pub(super) fn apply_note_card_tag_action(',1)[0]
assert 'synchronize_for_note_mutation(' in workflow
assert 'NoteCardAction::DeletePermanently' in note_card
assert 'NoteCardAction::UnlockEncrypted' in note_card
assert 'NoteCardAction::LockEncrypted' in note_card
assert 'NoteCardAction::RemoveEncryption' in note_card
assert transport.count('const deadline = Date.now() + 8000;') == 2

protect_segment = app_vault.split('pub(crate) fn protect_note(', 1)[1].split('pub(crate) fn unlock_note(', 1)[0]
assert 'self.unblock_autosave_profiled(note_id)?;' in protect_segment
lock_segment = app_vault.split('pub(crate) fn lock_note(', 1)[1].split('pub(crate) fn remove_note_protection(', 1)[0]
assert 'self.unblock_autosave_profiled(note_id)?;' in lock_segment
assert 'note_autosave_mutation_is_blocked(' in protection
assert '.note_session_is_active(note_id)' in protection
assert 'Locked-note label' in note_card and 'Visible without the password' in note_card
assert 'checkpoint_note_silent' in app_editor
assert 'locked target checkpoint must be an intentional no-op' in app_editor
assert 'normalize_locked_note_label' in protection
assert '"title" => Ok("title = ?4")' in protection
print('Rustnote 2b.31 native/full-QA source objectives passed under current scoped architecture.')
PY
if [[ "${RUSTNOTE_VALIDATOR_NO_CHAIN:-0}" != "1" ]]; then
  bash scripts/validate-vault-2b30-python-compat-deployment-validator-closure.sh
fi

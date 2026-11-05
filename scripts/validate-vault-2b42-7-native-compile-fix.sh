#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path

app = Path('src/ui/app.rs').read_text()
note_card = Path('src/ui/note_card.rs').read_text()
editor = Path('src/ui/editor.rs').read_text()
portable_bundle = Path('src/storage/portable_bundle.rs').read_text()
note_list_data = Path('src/ui/note_list_data.rs').read_text()
synchronization = Path('src/ui/synchronization.rs').read_text()

checks = [
    ('app imports note-list preference save helper', 'request_note_list_preferences_save,' in app.split('use crate::domain', 1)[0]),
    ('app no longer imports stale synchronize_current_editor', 'SynchronizationMode, synchronize_current_editor,' not in app),
    ('note card no longer imports stale synchronize_current_editor', 'SynchronizationMode, synchronize_current_editor,' not in note_card),
    ('note-link target signal has explicit type', 'let mut note_link_targets = use_signal(Vec::<NoteLinkTarget>::new);' in editor),
    ('backlinks view receives library generation', 'privacy_epoch,\n                                library_generation,\n                                note_generation: editor_generation,' in editor),
]

action_start = note_card.index('pub(super) fn apply_note_card_action')
action_end = note_card.index('pub(super) fn apply_note_card_tag_action', action_start)
action_body = note_card[action_start:action_end]
checks.append(('Vault action retains required editor_session binding', 'editor_session,' in action_body and 'synchronize_for_note_mutation(' in action_body))

tag_start = note_card.index('pub(super) fn apply_note_card_tag_action')
tag_end = note_card.find('\n}\n', tag_start)
tag_body = note_card[tag_start:tag_end]
checks.append(('tag action no longer binds unused editor_session', 'editor_session,' not in tag_body))

checks.extend([
    ('aggregate status binding is not unnecessarily mutable', 'let mut aggregate_status = status;' not in app and 'let aggregate_status = status;' in app),
    ('obsolete unlocked portable bundle wrapper is removed', 'pub(super) fn export_portable_bundle(' not in portable_bundle),
    ('obsolete note-list redaction helper is removed', 'pub(super) fn redact_protected_note' not in note_list_data),
    ('obsolete NoteSwitchLoadResult enum is removed', 'enum NoteSwitchLoadResult' not in synchronization),
    ('shared editor synchronization context exists', 'pub(super) struct EditorSynchronizationContext' in synchronization),
    ('scoped synchronization uses compact context signature', 'pub(super) async fn synchronize_current_editor_scoped(\n    context: &EditorSynchronizationContext,\n    mode: SynchronizationMode,' in synchronization),
    ('whole-library synchronization uses compact context signature', 'pub(super) async fn synchronize_then_begin_whole_library_operation(\n    _operation_turn: &ApplicationExecutionTurn,\n    context: &EditorSynchronizationContext,\n    operation_privacy: &PrivacyOperationService,\n    mode: SynchronizationMode,' in synchronization),
])

failed = [label for label, ok in checks if not ok]
if failed:
    raise SystemExit('2b.42.7 native compile/Clippy fix validation failed: ' + '; '.join(failed))
print(f'2b.42.7 native compile/Clippy fix validation passed ({len(checks)}/{len(checks)} contracts).')
PY

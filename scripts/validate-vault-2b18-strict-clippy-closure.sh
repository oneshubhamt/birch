#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
root = Path('.')
protection = (root/'src/storage/protection.rs').read_text()
storage_mod = (root/'src/storage/mod.rs').read_text()
protocol = (root/'src/attachment_protocol.rs').read_text()
attachments = (root/'src/storage/attachments.rs').read_text()
capabilities = (root/'src/storage/capabilities.rs').read_text()
locations = (root/'src/storage/locations.rs').read_text()
rendition = (root/'src/native/rendition.rs').read_text()
backups = (root/'src/storage/backups.rs').read_text()
tags = (root/'src/storage/tags.rs').read_text()
editor = (root/'src/ui/editor.rs').read_text()
flows = (root/'src/ui/editor_workflows.rs').read_text()
note_card = (root/'src/ui/note_card.rs').read_text()
metadata = (root/'src/ui/note_metadata.rs').read_text()
picker = (root/'src/ui/note_picker.rs').read_text()
right_panel = (root/'src/ui/right_panel.rs').read_text()
shutdown = (root/'src/ui/shutdown.rs').read_text()
app = (root/'src/ui/app.rs').read_text()

# Moved-value test closure: retain only identity before consuming Note.
assert 'let note_id = note.id.clone();\n        storage\n            .save_note_checkpoint_owned(note)' in protection
assert 'current_vault_graph_ids(&storage, &note_id)' in protection
assert '[&note_id]' in protection

# Binary tests do not reach through the library crate boundary to a private method.
assert 'pub(crate) fn lock_protected_note(&self' in protection
assert 'pub fn lock_protected_note(&self' not in protection
assert '#[cfg(any(test, feature = "internal-test-support"))]\n#[doc(hidden)]\npub fn lock_protected_note_for_test(' in storage_mod
assert protocol.count('rustnote::storage::lock_protected_note_for_test(&storage_for_lock, &note_for_lock)') == 2
assert 'storage_for_lock.lock_protected_note(&note_for_lock)' not in protocol

# Oversized reported signatures are consolidated into cohesive request/context values.
assert 'pub(crate) struct AuthorizedDerivedImageAttachment' in attachments
assert 'request: AuthorizedDerivedImageAttachment' in attachments
assert 'request: AuthorizedDerivedImageAttachment' in capabilities
assert 'request: AuthorizedAttachmentWrite' in attachments
assert 'struct ProtectedBlobInput' in protection
assert 'input: ProtectedBlobInput' in protection
assert 'pub(super) struct ImageImportFlow' in flows
assert 'pub(super) async fn run_image_import(flow: ImageImportFlow)' in flows
assert 'pub(super) struct RevisionRestoreFlow' in flows
assert 'flow: RevisionRestoreFlow' in flows
assert 'struct AttachmentActionContext' in right_panel
assert 'context: AttachmentActionContext' in right_panel
assert 'pub(super) struct CloseDurabilityRequest' in shutdown
assert 'pub(super) fn begin_close_durability(request: CloseDurabilityRequest)' in shutdown
assert app.count('begin_close_durability(CloseDurabilityRequest {') == 2

# Exact warning-class closure from the supplied native log.
assert '&mut *' not in editor
assert '&mut *' not in note_card
assert '.then(|| selected_id.as_ref())' not in metadata
assert '.then_some(selected_id.as_ref())' in metadata
assert 'if !published {\n                return;\n            }' not in picker
assert '.create(true)\n        .truncate(false)' in locations
assert 'if let Err(error) = result {\n                return Err(error);\n            }' not in protection
assert 'result?;' in protection
assert 'if let Some(owner) = owner {\n        if !destination.starts_with(owner.root())' not in rendition
assert 'if !tracked.contains(&relative) {\n                if crate::native::privacy::remove_file' not in attachments
assert 'if let Some(parent) = temporary_owned.path().parent() {\n                    if let Err(error) = fs::create_dir_all(parent)' not in backups
assert 'if current_name != new_name {\n            if let Some(source_id)' not in tags
assert protection.count('\n            &access,\n            &head.root,') == 0

# No new warning suppression was used as a substitute for these repairs.
# One pre-existing targeted allowance remains on the older graph-commit helper.
assert protection.count('#[allow(clippy::too_many_arguments)]') == 1
assert '#[allow(clippy::too_many_arguments)]\n    fn commit_vault_graph_update(' in protection
for text in (attachments, flows, right_panel, shutdown, editor, note_card):
    assert '#[allow(clippy::too_many_arguments)]' not in text

print('Rustnote 2b.18 strict Clippy closure static guard passed.')
PY

bash scripts/validate-vault-2b17-native-test-compile-closure.sh

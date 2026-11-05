#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path


def read(path):
    return Path(path).read_text(errors='ignore')


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f'{label}: missing {marker!r}')


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f'{label}: obsolete marker {marker!r}')

plaintext = read('src/security/plaintext.rs')
backup = read('src/storage/backup_repository.rs')
journal = read('src/autosave/emergency_journal.rs')
media = read('src/media_compat.rs')
rendition = read('src/native/rendition.rs')
ui_mod = read('src/ui/mod.rs')
privacy = read('src/ui/privacy.rs')
toast = read('src/ui/toast.rs')
app = read('src/ui/app.rs')
editor = read('src/ui/editor.rs')
notes = read('src/storage/notes.rs')

# I1/I2: ownership must pre-exist plaintext; no post-hoc adoption escape hatch.
require(plaintext, [
    'pub(crate) fn establish(',
    'let root_nonempty = match fs::symlink_metadata(&root)',
    'if root_nonempty {',
    'durable plaintext namespace marker is missing for a nonempty managed root',
    'durable_namespace_establish_refuses_posthoc_adoption_of_nonempty_root',
], 'durable namespace fail-closed establishment')
forbid(plaintext, ['adopt_existing_with_proof', 'allow_nonempty_adoption'], 'namespace adoption API')
forbid(backup, ['InlineRepositoryOwner', 'inline_owner', 'adopt_existing_with_proof'], 'backup adoption compatibility')
for name, text in [('journal', journal), ('media', media), ('rendition', rendition)]:
    require(text, ['DurablePlaintextNamespace::establish('], f'{name} namespace establishment')

# I5: protected status crosses into global UI only under a note-scoped token.
require(ui_mod, [
    'privacy_token: Option<privacy::PrivacyPublicationToken>',
    'fn is_publishable(&self) -> bool',
], 'token-retaining toast state')
require(privacy, [
    'pub(super) fn observe_publishable(&self) -> bool',
    'pub(super) fn begin_publication(&self) -> Option<PrivacyPublicationLease>',
], 'reactive scoped publication-token lifetime')
require(toast, [
    'pub(super) struct ProtectedStatusPublisher',
    'pub(super) fn publish(&self, token: PrivacyPublicationToken, message: String) -> bool',
    'let retained_token = token.clone();',
    'token.publish(|| {',
], 'protected status publication boundary')
require(app, [
    'ProtectedStatusPublisher::new(save_status, toast, toast_sequence)',
    'let _privacy_generation = privacy_epoch.observe_multi_note();',
    'is_some_and(|message| !message.is_publishable())',
    '&& current_toast.is_publishable()',
], 'global toast revocation lifetime')
require(editor, [
    'pub status_note_generation: Signal<u64>',
    'let protected_status_scope = note.protection.is_protected().then(||',
    'privacy_epoch.note_publication_scope(',
    'let Some(token) = scope.capture() else',
    'protected_status_publisher.publish(token, message)',
], 'protected editor status classification')

# I6: protected sync payloads are rejected centrally before serialization.
require(notes, [
    'fn sync_payload_allowed(',
    'fn clear_sync_payloads_for_entity(',
    'ordinary_note_and_attachment_sync_payloads_remain_enabled',
    'protected_note_and_attachment_payloads_never_enter_sync_queue',
], 'central protected sync prohibition')
upsert = notes[notes.index('pub(super) fn enqueue_sync_upsert'):]
if upsert.find('if !self.sync_payload_allowed') > upsert.find('serde_json::to_string(payload)?'):
    raise SystemExit('protected sync upsert is checked after serialization')

print('Vault 2b.10 cross-cutting invariant regression guard passed (current architecture).')
PY

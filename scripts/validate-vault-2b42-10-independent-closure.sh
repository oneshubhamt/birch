#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

python3 - <<'PY'
from pathlib import Path
import re

ROOT = Path('.')

def read(path: str) -> str:
    return Path(path).read_text(errors='ignore')

def require(source: str, markers, label: str) -> None:
    for marker in markers:
        if marker not in source:
            raise SystemExit(f'{label} missing: {marker}')

def segment(source: str, start: str, end: str | None = None) -> str:
    begin = source.find(start)
    if begin < 0:
        raise SystemExit(f'missing segment: {start}')
    if end is None:
        return source[begin:]
    finish = source.find(end, begin + len(start))
    return source[begin:finish if finish >= 0 else len(source)]

plaintext = read('src/security/plaintext.rs')
runtime = read('src/security/runtime.rs')
coordinator = read('src/security/privacy_coordinator.rs')
sensitive = read('src/security/sensitive_data.rs')
protection = read('src/storage/protection.rs')
ingest = read('src/storage/attachment_ingest.rs')
attachments = read('src/storage/attachments.rs')
backups = read('src/storage/backups.rs')
backup_repo = read('src/storage/backup_repository.rs')
history = read('src/storage/history.rs')
portable_bundle = read('src/storage/portable_bundle.rs')
staging = read('src/native/staging.rs')
temp_owner = read('src/native/temp_owner.rs')
handoff = read('src/native/handoff.rs')
emergency = read('src/autosave/emergency_journal.rs')
ui_privacy = read('src/ui/privacy.rs')
objects = read('src/storage/vault_objects.rs')

# 1. Canonical conclusive-presence primitive: only NotFound proves absence.
require(plaintext, [
    'pub(crate) fn managed_path_is_present(path: &Path) -> io::Result<bool>',
    'match fs::symlink_metadata(path)',
    'Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false)',
    'Err(error) => Err(error)',
    'true_not_found_is_conclusive_plaintext_absence',
    'metadata_failure_is_not_treated_as_plaintext_absence',
], 'conclusive managed-path presence')

# 2. The high-risk ownership/retirement call sites all use the strict primitive.
require(protection, [
    'if !blob.plaintext.path_entry_exists()? {',
    'retired_plaintext_restore_rejects_ambiguous_absence_and_retries_after_repair',
], 'Protect rollback ownership')
require(runtime, [
    'managed_path_is_present(&existing)',
    'managed_path_is_present(&path)',
    'ambiguous_materialized_attachment_presence_fails_closed_without_replacement',
], 'Vault materialization registry')
require(ingest, [
    'managed_path_is_present(&path)?',
    'ambiguous_pending_ingest_presence_keeps_durable_owner_until_retry',
], 'attachment ingest ownership')
require(attachments, [
    'managed_path_is_present(&staging_path)?',
    'managed_path_is_present(&final_path)?',
    'ambiguous_attachment_operation_stage_presence_fails_closed_until_retry',
], 'attachment-operation ownership')
require(staging, [
    'if !removed && crate::security::managed_path_is_present(path)?',
    'strict_plaintext_cleanup_does_not_accept_ambiguous_absence',
], 'strict native staging cleanup')

# 3. Additional ownership arbitration found by the de-novo sweep also fails closed.
require(temp_owner, [
    'managed_path_is_present(\n                        &root.join(CLEANUP_CLAIM_FILE)',
    'fn valid_owner_marker(path: &Path) -> io::Result<bool>',
    'Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false)',
    'Err(_) => return Some(false)',
    'ambiguous_owner_marker_metadata_never_authorizes_cleanup',
], 'process-private plaintext ownership arbitration')

# 4. Other repaired sensitive path boundaries propagate ambiguous metadata.
require(backups, ['managed_path_is_present(destination)?', 'managed_path_is_present(&blob.path)'], 'backup/Restore path boundary')
require(history, ['managed_path_is_present(&path)?'], 'revision attachment path boundary')
require(portable_bundle, ['managed_path_is_present(&destination)?'], 'portable-bundle path boundary')
require(handoff, ['managed_path_is_present(&destination)'], 'native handoff path boundary')
require(emergency, ['managed_path_is_present(destination)', 'managed_path_is_present(previous_path)'], 'emergency-journal path boundary')

# 5. Direct SQLite authority remains confined to storage.
for path in ROOT.joinpath('src').rglob('*.rs'):
    if path.as_posix().startswith('src/storage/'):
        continue
    source = path.read_text(errors='ignore')
    if re.search(r'\brusqlite\b|Connection::open(?:_with_flags)?\s*\(', source):
        raise SystemExit(f'direct SQLite authority escaped storage: {path}')

# 6. Privacy transitions retain one low-level authority.
for path in ROOT.joinpath('src').rglob('*.rs'):
    if path.as_posix().startswith('src/security/'):
        continue
    source = path.read_text(errors='ignore')
    for primitive in ('begin_note_revocation(', 'begin_note_revocations(', 'begin_library_revocation('):
        if primitive in source:
            raise SystemExit(f'low-level Vault revocation escaped the security authority: {path}: {primitive}')
require(coordinator, [
    'pub(crate) fn begin_note_transition',
    'pub(crate) fn begin_note_transitions',
    'pub(crate) fn begin_library_transition',
    'publication_drained',
], 'single transition authority')

# 7. Publication remains scoped, generation-checked, and leased.
require(ui_privacy, [
    'pub(super) struct PrivacyPublicationToken',
    'authorization: PrivacyPublicationAuthorization',
    'pub(super) fn begin_publication',
    'let coordinator = self.authorization.begin_publication()?;',
], 'scoped publication authority')

# 8. Sensitive relational persistence remains deny-by-default/exhaustively classified.
require(sensitive, [
    'pub(crate) struct SensitiveNoteData',
    'pub(crate) struct ProtectedPublicPersistenceSpec',
    'pub(crate) const SCRUB_STEPS',
    'scrub_plan_covers_every_declared_sensitive_relational_surface',
], 'deny-by-default sensitive persistence')

# 9. Canonical E3 SQLite residue sequence remains secure-delete + FTS rebuild + verified WAL truncate.
require(protection, [
    'PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;',
    "INSERT INTO notes_fts(notes_fts) VALUES('rebuild')",
    'ensure_wal_truncated(&connection)',
], 'E3 SQLite residue closure')
if 'connection.execute_batch("VACUUM;")' in protection:
    raise SystemExit('obsolete production VACUUM returned after 2b.42.8 forensic closure')

# 10. Backup repository deletion authority is external, durable, and fail-closed.
require(backup_repo, [
    'const BACKUP_NAMESPACE_MARKER_FILE_NAME: &str = ".rustnote-backup-repository-owner";',
    'DurablePlaintextNamespace::open_existing(',
    'DurablePlaintextNamespace::establish(',
    'namespace.remove_tree_strict()',
    'interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear',
    'steady_state_backup_writes_fail_closed_after_namespace_marker_loss',
], 'backup durable namespace')
for obsolete in ('INLINE_OWNER', 'InlineRepositoryOwner', 'adopt_existing_with_proof'):
    if obsolete in backup_repo:
        raise SystemExit(f'obsolete backup ownership compatibility remains: {obsolete}')

# 11. Protected export/portable persistence remains explicitly gated.
portability = read('src/storage/portability.rs')
require(portability, ['ensure_portability_scope_unprotected'], 'portable export protection gate')
require(portable_bundle, ['ensure_portability_scope_unprotected'], 'bundle export protection gate')

# 12. Crypto remains isolated behind the security facade.
allowed_vault_users = {
    'src/security/mod.rs',
    'src/security/backup_key_authority.rs',
    'src/platform/secure_key_store.rs',
}
for path in ROOT.joinpath('src').rglob('*.rs'):
    source = path.read_text(errors='ignore')
    if path.as_posix() not in allowed_vault_users and 'rustnote_vault' in source:
        raise SystemExit(f'rustnote-vault bypasses audited security boundary: {path}')
    for primitive in ('use aegis::', 'use argon2::', 'use chacha20poly1305::', 'use aes_gcm::'):
        if primitive in source:
            raise SystemExit(f'low-level crypto primitive leaked into app code: {path}: {primitive}')

# 13. I7 remains granular: autosave mutates graph components rather than hydrating full history.
save = segment(protection, 'pub(super) fn persist_protected_note_content', 'pub(super) fn mutate_protected_metadata_if_unlocked')
require(save, ['read_vault_graph_head', 'commit_vault_graph_update'], 'I7 protected save locality')
for forbidden in ('read_vault_current_graph', 'read_vault_revision_chain'):
    if forbidden in save:
        raise SystemExit(f'I7 protected autosave hydrates unrelated state: {forbidden}')
require(objects, ['VaultObjectKind::NotePrivateState', 'VaultObjectKind::MetadataState', 'VaultObjectKind::AttachmentIndex', 'VaultObjectKind::Revision'], 'granular object model')

# 14. No obsolete monolithic protected-persistence representation is live in production Rust.
for path in ROOT.joinpath('src').rglob('*.rs'):
    source = path.read_text(errors='ignore')
    for obsolete in ('ProtectedNotePayload', 'protected_note_blobs'):
        if obsolete in source:
            raise SystemExit(f'obsolete protected representation remains: {path}: {obsolete}')

# 15. De-novo cleanup sweep must not leave the superseded unauthorised derived-image façade.
# The live application routes image edits through the explicitly authorized variant.
if re.search(r'\bpub\s+fn\s+add_derived_image_attachment_file\s*\(', attachments):
    raise SystemExit('dead unauthorised derived-image Storage façade remains after 2b.42 cleanup')
require(attachments, ['add_derived_image_attachment_file_authorized'], 'authorized derived-image attachment path')

# 16. The repaired defect has adversarial coverage for ambiguous error, retry, NotFound, and durable authority survival.
for marker, source in (
    ('metadata_failure_is_not_treated_as_plaintext_absence', plaintext),
    ('true_not_found_is_conclusive_plaintext_absence', plaintext),
    ('ambiguous_materialized_attachment_presence_fails_closed_without_replacement', runtime),
    ('ambiguous_pending_ingest_presence_keeps_durable_owner_until_retry', ingest),
    ('ambiguous_attachment_operation_stage_presence_fails_closed_until_retry', attachments),
    ('retired_plaintext_restore_rejects_ambiguous_absence_and_retries_after_repair', protection),
    ('strict_plaintext_cleanup_does_not_accept_ambiguous_absence', staging),
    ('ambiguous_owner_marker_metadata_never_authorizes_cleanup', temp_owner),
):
    if marker not in source:
        raise SystemExit(f'adversarial regression missing: {marker}')

print('Vault 2b.42.10 independent closure source validation passed (16/16 contracts).')
PY

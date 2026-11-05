#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path


def read(path):
    return Path(path).read_text(errors="ignore")


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f"{label} missing: {marker}")


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f"{label} contains forbidden marker: {marker}")


def segment(text, start, end):
    i = text.find(start)
    if i < 0:
        raise SystemExit(f"missing segment start: {start}")
    j = text.find(end, i + len(start))
    if j < 0:
        raise SystemExit(f"missing segment end: {end}")
    return text[i:j]

profile = read("src/vault_profile.rs")
lib = read("src/lib.rs")
ui = read("src/ui/note_card.rs")
application_vault = read("src/application/vault.rs")
protection = read("src/storage/protection.rs")
runner = read("scripts/run-browser-behavior-tests.mjs")

# 1. Profiling is disabled unless explicitly opted in. OnceLock means getenv is
# paid once; disabled spans never capture Instant and Drop returns before eprintln.
require(profile, [
    'const VAULT_PROFILE_ENV: &str = "RUSTNOTE_VAULT_PROFILE";',
    'static VAULT_PROFILE_ENABLED: OnceLock<bool>',
    'value == "1"',
    'started_at: vault_profile_enabled().then(Instant::now)',
    'let Some(started_at) = self.started_at else',
    'return;',
], "opt-in profiler")
require(lib, ["mod vault_profile;"], "profiler module wiring")

# 2. Labels are a closed typed vocabulary. The only free-form constructor is
# private to this module, preventing callers from logging titles/IDs/paths.
require(profile, [
    "pub(crate) enum VaultProfileOperation",
    "Protect,",
    "Lock,",
    "Unlock,",
    "RemoveEncryption,",
    "pub(crate) enum VaultProfileStage",
    "UiCheckpoint,",
    "AutosaveBarrier,",
    "AutosaveRelease,",
    "PrivacyDrains,",
    "ProtectPreparation,",
    "Crypto,",
    "AttachmentsEncrypt,",
    "AttachmentsStage,",
    "AttachmentsCleanup,",
    "ManagedResidueScrub,",
    "CacheCleanup,",
    "StagingCleanup,",
    "HandoffCleanup,",
    "BackupRepositoryReset,",
    "SqliteResidueScrub,",
    "TransitionFinalize,",
    "AttachmentsDecrypt,",
    "AttachmentsFinalize,",
    "ObjectWrites,",
    "Fts,",
    "Wal,",
    "CanonicalPublication,",
    "fn new(kind: VaultProfileKind, name: &'static str)",
], "closed profiling vocabulary")
forbid(profile, [
    "pub(crate) fn new(kind: VaultProfileKind",
], "public free-form profiler constructor")
output = segment(profile, "impl Drop for VaultProfileSpan", "#[inline]\nfn vault_profile_enabled")
forbid(output, [
    "note_id",
    "password",
    "public_label",
    "file_name",
    "path",
    "sha256",
    "byte_size",
    "content",
], "sensitive/dynamic profiler output")
require(output, [
    '"[rustnote-vault-profile] kind={} name={} elapsed_us={}"',
], "static profiler output")

# 3. End-to-end operation totals are emitted only for the four requested Vault
# actions. Checkpoint and canonical-publication spans are gated by the same map.
mapping = segment(ui, "fn vault_profile_operation(", "fn vault_action_progress_message(")
require(mapping, [
    "NoteCardAction::Encrypt { .. } => Some(VaultProfileOperation::Protect)",
    "NoteCardAction::UnlockEncrypted { .. } => Some(VaultProfileOperation::Unlock)",
    "NoteCardAction::LockEncrypted(_) => Some(VaultProfileOperation::Lock)",
    "NoteCardAction::RemoveEncryption { .. } => Some(VaultProfileOperation::RemoveEncryption)",
    "_ => None",
], "UI operation map")
action = segment(ui, "pub(super) fn apply_note_card_action(", "pub(super) fn apply_note_card_tag_action(")
require(action, [
    "profile_operation.map(VaultProfileSpan::operation)",
    "vault_profiled_action",
    "VaultProfileStage::UiCheckpoint",
    "VaultProfileStage::CanonicalPublication",
], "UI profiling stages")
if action.count("VaultProfileStage::CanonicalPublication") < 2:
    raise SystemExit("success and reconciliation publication paths are not both profiled")

# 4. Storage stages cover the requested dimensions without changing transition
# authority: every operation still calls begin_note_privacy_transition directly.
protect = segment(protection, "pub(crate) fn protect_note_with_password_and_public_label(", "pub(crate) fn protected_note_ids_for_privacy_recovery(")
require(protect, [
    "VaultProfileStage::PrivacyDrains",
    "begin_note_privacy_transition(",
    "VaultProfileStage::ProtectPreparation",
    "VaultProfileStage::Crypto",
    "VaultProfileStage::AttachmentsEncrypt",
    "VaultProfileStage::AttachmentsStage",
    "VaultProfileStage::ObjectWrites",
    "VaultProfileStage::AttachmentsCleanup",
    "VaultProfileStage::ManagedResidueScrub",
    "VaultProfileStage::TransitionFinalize",
], "Protect profiling")
unlock = segment(protection, "pub(crate) fn unlock_protected_note(", "pub(crate) fn lock_protected_note(")
require(unlock, ["VaultProfileStage::PrivacyDrains", "VaultProfileStage::Crypto"], "Unlock profiling")
lock = segment(protection, "pub(crate) fn lock_protected_note(", "pub(crate) fn remove_note_protection_with_password(")
require(lock, ["VaultProfileStage::PrivacyDrains", "VaultProfileStage::AttachmentsCleanup"], "Lock profiling")
unprotect = segment(protection, "pub(crate) fn remove_note_protection_with_password(", "pub(super) fn hydrate_protected_note(")
require(unprotect, [
    "VaultProfileStage::PrivacyDrains",
    "VaultProfileStage::Crypto",
    "VaultProfileStage::AttachmentsDecrypt",
    "VaultProfileStage::ObjectWrites",
    "VaultProfileStage::AttachmentsFinalize",
], "Remove Encryption profiling")

# 5. Autosave admission/release is measured at the application boundary without
# changing the worker barrier semantics.
require(application_vault, [
    "VaultProfileStage::AutosaveBarrier",
    "block_note_for_vault(note_id)",
    "VaultProfileStage::AutosaveRelease",
    "unblock_note_after_vault(note_id)",
], "autosave barrier profiling")

# 6. The previously residual Protect security scrub is now a parent stage with
# static sub-stages for each existing cleanup boundary. No cleanup call is moved.
managed_scrub = segment(protection, "fn scrub_managed_plaintext_residue_after_hashes(", "/// Protected note identifiers are exposed")
require(managed_scrub, [
    "VaultProfileStage::CacheCleanup",
    "discard_image_rendition_cache_strict",
    "discard_orphaned_quarantine_for_hashes_strict",
    "VaultProfileStage::StagingCleanup",
    "discard_staged_derivatives_for_note",
    "discard_staged_derivatives_for_attachments",
    "VaultProfileStage::HandoffCleanup",
    "discard_handoffs_for_source_keys",
    "VaultProfileStage::BackupRepositoryReset",
    "reset_backup_repository_for_vault_unlocked",
    "VaultProfileStage::SqliteResidueScrub",
    "scrub_sqlite_plaintext_residue_unlocked",
], "managed residue profiling")

# 7. 2b.42.8 intentionally removed VACUUM after native forensic proof. Profiling
# must still wrap the canonical FTS rebuild -> verified WAL truncate sequence and
# must not retain an obsolete VACUUM stage.
scrub = segment(protection, "fn scrub_sqlite_plaintext_residue_unlocked(", "/// Durable Vault transitions block autosave mutation admission")
require(scrub, [
    "VaultProfileStage::Fts",
    "INSERT INTO notes_fts(notes_fts) VALUES('rebuild')",
    "VaultProfileStage::Wal",
    "ensure_wal_truncated(&connection)",
], "SQLite cleanup profiling")
if scrub.count("VaultProfileStage::Wal") != 1 or scrub.count("ensure_wal_truncated(&connection)") != 1:
    raise SystemExit("profiling changed the canonical WAL truncation boundary")
for forbidden in ("VaultProfileStage::Vacuum", 'connection.execute_batch("VACUUM;")'):
    if forbidden in scrub:
        raise SystemExit(f"profiling retained obsolete 2b.42.8 VACUUM marker: {forbidden}")

# 8. Dependency-free regression runner must execute the profiling contract.
require(runner, ["vault-profile-contract.test.mjs"], "browser behavior runner")

print("2b.42.7 profiling validation passed (8/8 contracts).")
PY

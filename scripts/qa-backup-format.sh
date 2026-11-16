#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
from pathlib import Path

authority = Path("src/security/backup_key_authority.rs").read_text()
secure = Path("src/platform/secure_key_store.rs").read_text()
vault = Path("crates/rustnote-vault/src/lib.rs").read_text()
repository = Path("src/storage/backup_repository.rs").read_text()
all_source = "\n".join(
    path.read_text(errors="ignore")
    for root in (Path("src"), Path("crates"))
    for path in root.rglob("*.rs")
)

authority_markers = (
    "BACKUP_REPOSITORY_FORMAT_VERSION: u16 = 3",
    "BACKUP_REPOSITORY_HEADER_VERSION: u16 = 1",
    "BACKUP_KEYSET_VERSION: u16 = 1",
    "BACKUP_REPOSITORY_HEADER_PREFIX_BYTES: usize = 92",
    'BACKUP_REPOSITORY_HEADER_MAGIC: &[u8; 8] = b"RNBHDR01"',
    'BACKUP_REPOSITORY_KEYSET_MAGIC: &[u8; 8] = b"RNBKEY01"',
    "pub(crate) struct BackupRepositoryKeysetV1",
    "snapshot_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>",
    "blob_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>",
    "metadata_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>",
    "object_id_hmac_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>",
)
for marker in authority_markers:
    if marker not in authority:
        raise SystemExit(f"backup key authority contract missing: {marker}")

for marker in (
    "fresh_repository_header_matches_exact_2b43_2_apple_layout",
    "fresh_initialization_creates_root_and_round_trips_keyset",
    "existing_repository_open_never_recreates_missing_root",
    "wrong_device_root_cannot_open_repository_keyset",
    "unavailable_or_corrupt_secure_store_fails_closed",
    "repository_and_library_context_are_authenticated",
    "present_device_root_is_reused_without_replacement",
    "repository_header_rejects_reserved_unknown_and_noncanonical_envelope_fields",
    "repository_keyset_encoding_is_exact_204_byte_canonical_layout",
    "repository_keysets_are_independent_and_redacted",
):
    if f"fn {marker}" not in authority:
        raise SystemExit(f"backup key-authority regression owner missing: {marker}")

for marker in (
    "BACKUP_DEVICE_ROOT_KEY_BYTES: usize = 32",
    "BACKUP_REPOSITORY_KEYSET_V1_BYTES: usize = 204",
    "BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES: usize = 268",
    "BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES: usize = 232",
    "backup_apple_keyset_envelope_round_trips_with_exact_v1_size",
    "backup_apple_keyset_test_fixture_uses_deterministic_nonce_without_production_api",
    "backup_apple_keyset_envelope_authenticates_aad_key_and_ciphertext",
):
    if marker not in vault:
        raise SystemExit(f"backup cryptographic format owner missing: {marker}")

for marker in (
    "pub(crate) trait DeviceBackupKeyProtector: Send + Sync",
    'KEYCHAIN_SERVICE: &str = "org.rustnote.backup-device-root"',
    'KEYCHAIN_ACCOUNT: &str = "device-backup-root-v1"',
    "BackupProtectorKind::AppleKeychainRootV1",
    "BackupProtectorKind::AndroidKeystoreRootV1",
    "UnsupportedDeviceBackupKeyProtector",
):
    if marker not in secure:
        raise SystemExit(f"secure backup key-store boundary missing: {marker}")

for forbidden in (
    "RUSTNOTE_BACKUP_KEY",
    "BACKUP_ROOT_KEY_FILE",
    "backup-root.key",
    ".backup-key",
):
    if forbidden in all_source:
        raise SystemExit(f"forbidden ordinary backup-root-key source returned: {forbidden}")

for marker in (
    "repository_snapshot_restore_round_trips_note_and_attachment_through_library_restore",
    "interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear",
    "restart_recovers_after_repository_removed_before_privacy_obligation_retirement",
    "steady_state_backup_writes_fail_closed_after_namespace_marker_loss",
):
    if f"fn {marker}" not in repository:
        raise SystemExit(f"backup repository recovery owner missing: {marker}")

print("qa-backup-format: key authority, byte format, secure-store and recovery owners passed.")
PY

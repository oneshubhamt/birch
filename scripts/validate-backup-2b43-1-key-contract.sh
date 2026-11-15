#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
from pathlib import Path

root = Path('.')
contract = (root / 'BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md').read_text()
authority = (root / 'src/security/backup_key_authority.rs').read_text()
secure_store = (root / 'src/platform/secure_key_store.rs').read_text()
manifest = (root / 'Cargo.toml').read_text()
build = (root / 'build-id.txt').read_text().strip()

checks = []
def require(text, markers, label):
    missing = [m for m in markers if m not in text]
    assert not missing, f"{label}: missing {missing}"
    checks.append(label)

require(contract, [
    'Device Backup Root Key',
    'RepositoryKeysetV1',
    'snapshot_aead_key',
    'blob_aead_key',
    'metadata_aead_key',
    'object_id_hmac_key',
], 'narrow root authority + independent repository keyset')

require(contract, [
    'Temporarily unavailable secure store',
    'Root key absent/deleted while encrypted repository exists',
    'Root key or key envelope corrupt',
    'FAIL CLOSED',
    'plaintext fallback = forbidden',
], 'key failure state machine')

require(contract, [
    'kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly',
    'AndroidKeyStore',
    'AES-256-GCM wrapping key',
    'non-exportable key material',
], 'macOS + Android secure-store profiles')

require(contract, [
    'REPOSITORY_FORMAT_VERSION = 3',
    'KEY_ENVELOPE_VERSION      = 1',
    'BACKUP_OBJECT_VERSION     = 1',
    'repository.header',
    'index.rnbi',
    '.rnbd',
    '.rnbm',
    '.rnbb',
], 'canonical v3 encrypted repository format')

require(contract, [
    'HMAC-SHA-256(object_id_hmac_key, plaintext_digest)',
    'raw plaintext SHA-256 values must no longer be exposed',
], 'opaque deduplication contract')

require(contract, [
    'reserve authoritative process-private OwnedPlaintext snapshot',
    'Repository `.staging` must contain ciphertext only',
    'PrivacyCoordinator',
    'OwnedPlaintext',
], 'plaintext lifetime reuses existing authorities')

require(contract, [
    'Restore after key loss',
    'fails before creating any plaintext restore stage',
    'Cross-device/key-escrow recovery is outside 2b.43',
], 'restore/key-loss behavior')

require(contract, [
    'does **not** implement',
    'v2 plaintext repository → v3 encrypted repository conversion',
    'destructive privacy cleanup',
    'cutover cleanup rule, not a compatibility layer',
], 'no migration/backward-compatibility architecture')

require(contract, [
    'The existing Protect-time plaintext-backup purge must not be removed',
    'Only then may Protect stop resetting encrypted historical repositories.',
], 'safe transitional Protect rule')

require(authority, [
    'pub(crate) const BACKUP_REPOSITORY_FORMAT_VERSION: u16 = 3;',
    'pub(crate) struct BackupRepositoryKeysetV1',
    'snapshot_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>',
    'blob_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>',
    'metadata_aead_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>',
    'object_id_hmac_key: Zeroizing<[u8; REPOSITORY_KEY_BYTES]>',
    'existing_repository_open_never_recreates_missing_root',
    'wrong_device_root_cannot_open_repository_keyset',
    'unavailable_or_corrupt_secure_store_fails_closed',
    'repository_and_library_context_are_authenticated',
    'repository_keysets_are_independent_and_redacted',
], 'implemented key-authority + fail-closed regression owners')

require(secure_store, [
    'pub(crate) trait DeviceBackupKeyProtector: Send + Sync',
    'const KEYCHAIN_SERVICE: &str = "org.rustnote.backup-device-root";',
    'const KEYCHAIN_ACCOUNT: &str = "device-backup-root-v1";',
    'BackupProtectorKind::AndroidKeystoreRootV1',
    'UnsupportedDeviceBackupKeyProtector',
], 'secure-store authority implementations')

assert 'version = "0.12.6"' in manifest, 'unexpected package version'
assert build == '2b.43', f'unexpected build id: {build}'
checks.append('baseline identity')

# Historical pass evidence is provenance only. Current closure is owned by the
# implemented key authority, secure-store boundary and all-feature Rust tests.
print(f'Backup 2b.43.1 key-authority contract validation passed ({len(checks)}/{len(checks)} contracts).')
PY

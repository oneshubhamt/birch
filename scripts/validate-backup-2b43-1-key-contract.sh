#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
from pathlib import Path

root = Path('.')
contract = (root / 'BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md').read_text()
qa = (root / 'QA_BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md').read_text()
repo = (root / 'src/storage/backup_repository.rs').read_text()
platform = (root / 'src/platform/mod.rs').read_text()
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

require(qa, [
    '**PASS — contract/audit pass only. No production backup implementation changed.**',
    'All fifteen 2b.43.1 Definition-of-Done items are satisfied',
], 'QA closure record')

assert 'version = "0.12.6"' in manifest, 'unexpected package version'
assert build == '2b.43', f'unexpected build id: {build}'
checks.append('baseline identity')

# 2b.43.1 itself was contract-only; later 2b.43 implementation passes are
# expected to consume that contract. Preserve the historical proof in the QA
# artifact rather than incorrectly forbidding subsequent implementation.
assert 'contract-only' in qa.lower(), '2b.43.1 QA must preserve its contract-only closure evidence'
checks.append('historical contract-only closure evidence')

print(f'Backup 2b.43.1 key-authority contract validation passed ({len(checks)}/{len(checks)} contracts).')
PY

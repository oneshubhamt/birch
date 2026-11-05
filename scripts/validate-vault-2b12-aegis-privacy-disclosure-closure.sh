#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

for required in \
  VAULT_2B12_AEGIS_PRIVACY_DISCLOSURE_CLOSURE.md \
  QA_VAULT_2B12_CLOSURE.md \
  SECURITY_ENCRYPTION_E0_CONTRACT.md \
  SECURITY_ENCRYPTION_E1_CONTRACT.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.12 closure file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path
import re

vault_manifest = Path('crates/rustnote-vault/Cargo.toml').read_text()
vault = Path('crates/rustnote-vault/src/lib.rs').read_text()
e0 = Path('SECURITY_ENCRYPTION_E0_CONTRACT.md').read_text()
e1 = Path('SECURITY_ENCRYPTION_E1_CONTRACT.md').read_text()
app_vault = Path('src/application/vault.rs').read_text()
backup = Path('src/storage/backup_repository.rs').read_text()
capabilities = Path('src/storage/capabilities.rs').read_text()
note_card = Path('src/ui/note_card.rs').read_text()
settings = Path('src/ui/settings.rs').read_text()

for pin in (
    'aegis = "=0.9.15"',
    'argon2 = { version = "=0.5.3", default-features = false, features = ["alloc", "zeroize"] }',
    'getrandom = "=0.3.4"',
    'zeroize = { version = "=1.9.0", features = ["alloc"] }',
):
    if pin not in vault_manifest:
        raise SystemExit(f'2b.12 Vault dependency/profile pin missing: {pin}')
if 'pure-rust' in vault_manifest:
    raise SystemExit('2b.12 production AEGIS must keep the default backend; pure-rust is not permitted')

for marker in (
    'const KEY_BYTES: usize = 32;',
    'const NONCE_BYTES: usize = 32;',
    'const TAG_BYTES: usize = 32;',
    'const SALT_BYTES: usize = 16;',
    'const _: [(); 32] = [(); KEY_BYTES];',
    'const _: [(); 32] = [(); NONCE_BYTES];',
    'const _: [(); 32] = [(); TAG_BYTES];',
    'const _: [(); 16] = [(); SALT_BYTES];',
    'const V1_ARGON2_MEMORY_KIB: u32 = 64 * 1024;',
    'const V1_ARGON2_TIME_COST: u32 = 3;',
    'const V1_ARGON2_LANES: u32 = 4;',
    'const MAX_ARGON2_MEMORY_KIB: u32 = V1_ARGON2_MEMORY_KIB;',
    'const MAX_ARGON2_TIME_COST: u32 = V1_ARGON2_TIME_COST;',
    'const MAX_ARGON2_LANES: u32 = V1_ARGON2_LANES;',
    'const BLOB_NONCE_PREFIX_BYTES: usize = 24;',
    'const BLOB_NONCE_COUNTER_BYTES: usize = 8;',
    'const _: [(); NONCE_BYTES] = [(); BLOB_NONCE_PREFIX_BYTES + BLOB_NONCE_COUNTER_BYTES];',
    'Version::V0x13',
    'Algorithm::Argon2id',
    'getrandom::fill(output)',
    'Aegis256::<TAG_BYTES>',
    'authenticated_context(b"object"',
    'authenticated_context(b"key-envelope"',
    'authenticated_context(b"blob-header"',
    'authenticated_context(b"blob-chunk"',
    'let header_nonce = blob_nonce(&nonce_prefix, u64::MAX);',
    'aad.extend_from_slice(&index.to_be_bytes());',
    'aad.extend_from_slice(&plaintext_len.to_be_bytes());',
    'pub const DEFAULT_PROTECTED_BLOB_CHUNK_BYTES: usize = 1024 * 1024;',
    'pub const MIN_PROTECTED_BLOB_CHUNK_BYTES: usize = 4 * 1024;',
    'pub const MAX_PROTECTED_BLOB_CHUNK_BYTES: usize = 8 * 1024 * 1024;',
    'fn v1_argon2_profile_is_the_untrusted_envelope_resource_ceiling()',
):
    if marker not in vault:
        raise SystemExit(f'2b.12 AEGIS/KDF invariant missing: {marker}')

if 'V1_INTERACTIVE_CANDIDATE' in vault:
    raise SystemExit('2b.12 must not retain the obsolete candidate KDF alias after policy is fixed')
for marker in ('pub memory_kib:', 'pub time_cost:', 'pub lanes:'):
    if marker in vault:
        raise SystemExit(f'2b.12 fixed Argon2 policy must not expose constructible parameter fields: {marker}')

for marker in (
    '2b.12 product decision override — AEGIS-256 fixed profile',
    'AEGIS-256 with a 256-bit key, 256-bit nonce and full 256-bit authentication tag',
    'Comparative AEGIS/XChaCha/AES benchmarking remains an optional engineering diagnostic',
    '64 MiB memory, 3 passes, 4 lanes, 128-bit random salt and a 256-bit derived wrapping key',
):
    if marker not in e0:
        raise SystemExit(f'2b.12 E0 product-decision record missing: {marker}')
for marker in (
    'Argon2idParams::V1_INTERACTIVE',
    '64 MiB, time cost 3, lanes 4',
    'same v1 resource profile',
):
    if marker not in e1:
        raise SystemExit(f'2b.12 E1 fixed-profile record missing: {marker}')

for marker in (
    'pub(crate) struct ProtectNoteOutcome',
    'pending_backup_privacy_resets: Option<usize>',
):
    if marker not in app_vault:
        raise SystemExit(f'2b.12 Protect outcome is missing: {marker}')
if not re.search(r'\.pending_backup_privacy_reset_count\(\)\s*\.ok\(\)', app_vault):
    raise SystemExit('2b.12 Protect outcome is missing the read-only privacy-reset status query')

if 'pub(crate) fn pending_backup_privacy_reset_count(&self) -> StorageResult<usize>' not in capabilities:
    raise SystemExit('2b.12 VaultStore read-only privacy-reset query is missing')
if 'pub(crate) fn pending_backup_privacy_reset_count(&self) -> StorageResult<usize>' not in backup:
    raise SystemExit('2b.12 Storage read-only privacy-reset query is missing')
status_body = backup.split('pub(crate) fn pending_backup_privacy_reset_count(&self)', 1)[1].split('pub fn backup_repository_status', 1)[0]
for forbidden in ('verified_backup_repository_layout', '.ensure()', 'lock_operations(', 'write_'):
    if forbidden in status_body:
        raise SystemExit(f'2b.12 post-Protect privacy query must remain read-only: {forbidden}')
if 'pending_backup_privacy_reset_count_for(' not in status_body:
    raise SystemExit('2b.12 post-Protect privacy query must read the durable location-history obligation')

for marker in (
    'protect_note_status_message(outcome.pending_backup_privacy_resets)',
    'Note encrypted locally and unlocked for this session.',
    'earlier plaintext copy',
    'privacy cleanup',
    'Backup privacy status could not be refreshed',
    'Settings > Backups',
    'protect_success_discloses_pending_backup_plaintext_risk',
    'protect_success_is_conservative_when_backup_status_refresh_fails',
):
    if marker not in note_card:
        raise SystemExit(f'2b.12 backup privacy disclosure missing: {marker}')

if 'Encryption is complete locally.' not in settings or 'still require privacy invalidation' not in settings:
    raise SystemExit('2b.12 must retain the persistent Backup-settings privacy warning')

# The product decision is AEGIS-only. No low-level alternative primitive may
# enter production app/storage code; the isolated E0 comparison tool is exempt.
for path in Path('src').rglob('*.rs'):
    text = path.read_text(errors='ignore')
    for marker in ('use chacha20poly1305::', 'use aes_gcm::'):
        if marker in text:
            raise SystemExit(f'alternative production cipher leaked into {path}: {marker}')

print('Vault 2b.12 AEGIS-256/privacy-disclosure closure guard passed.')
PY

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

bash scripts/validate-backup-2b43-1-key-contract.sh >/dev/null
bash scripts/validate-backup-2b43-2-byte-format.sh >/dev/null

python3 - "$ROOT" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
checks = []

def text(path):
    return (root / path).read_text()

def require(name, condition, detail):
    if not condition:
        raise SystemExit(f"FAIL: {name}: {detail}")
    checks.append(name)

cargo = text("Cargo.toml")
lock = text("Cargo.lock")
vault = text("crates/rustnote-vault/src/lib.rs")
platform_mod = text("src/platform/mod.rs")
mobile = text("src/platform/mobile.rs")
secure = text("src/platform/secure_key_store.rs")
security_mod = text("src/security/mod.rs")
authority = text("src/security/backup_key_authority.rs")
backup_repo = text("src/storage/backup_repository.rs")
all_storage = "\n".join(p.read_text(errors="ignore") for p in (root / "src/storage").glob("*.rs"))

require(
    "pinned platform dependencies",
    'getrandom = "=0.3.4"' in cargo
    and 'zeroize = { version = "=1.9.0", features = ["alloc"] }' in cargo
    and '[target.\'cfg(target_os = "macos")\'.dependencies]' in cargo
    and 'core-foundation = "=0.10.1"' in cargo
    and 'security-framework = { version = "=3.7.0", features = ["OSX_10_15"] }' in cargo
    and 'security-framework-sys = { version = "=2.17.0", features = ["OSX_10_15"] }' in cargo,
    "the secure-store boundary must use the pinned audited dependencies",
)

require(
    "lockfile root dependency closure",
    'name = "rustnote"\nversion = "0.12.6"' in lock
    and '"getrandom 0.3.4"' in lock
    and '"security-framework"' in lock
    and '"security-framework-sys"' in lock
    and '"zeroize"' in lock,
    "Cargo.lock must already contain the promoted root dependency closure for --locked builds",
)

require(
    "backup envelope crypto isolated in rustnote-vault",
    all(marker in vault for marker in [
        "BACKUP_DEVICE_ROOT_KEY_BYTES: usize = 32",
        "BACKUP_REPOSITORY_KEYSET_V1_BYTES: usize = 204",
        "BACKUP_APPLE_KEYSET_ENVELOPE_V1_BYTES: usize = 268",
        "BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES: usize = 232",
        "pub struct BackupDeviceRootKey(Zeroizing<[u8; BACKUP_DEVICE_ROOT_KEY_BYTES]>)",
        "pub fn from_zeroizing(bytes: Zeroizing<[u8; BACKUP_DEVICE_ROOT_KEY_BYTES]>)",
        "pub struct BackupKeyEnvelopeService",
        "Aegis256::<TAG_BYTES>::new(root_key.bytes(), &nonce)",
    ]),
    "macOS key-envelope cryptography must remain in the canonical Vault crypto crate",
)

require(
    "random production envelope plus deterministic test fixture",
    "self.seal_apple_keyset_v1_with_nonce(root_key, aad, plaintext, random_nonce()?)" in vault
    and "fn seal_apple_keyset_v1_with_nonce(" in vault
    and "backup_apple_keyset_test_fixture_uses_deterministic_nonce_without_production_api" in vault,
    "production must use random AEGIS nonces while tests retain a private deterministic fixture",
)

require(
    "envelope authentication regression coverage",
    all(name in vault for name in [
        "backup_apple_keyset_envelope_round_trips_with_exact_v1_size",
        "backup_apple_keyset_envelope_authenticates_aad_key_and_ciphertext",
        "BackupDeviceRootKey([REDACTED])",
    ]),
    "exact size, AAD/key/ciphertext authentication and secret Debug redaction must be tested",
)

require(
    "wire-stable protector enums",
    "AppleKeychainRootV1 = 1" in secure
    and "AndroidKeystoreRootV1 = 2" in secure
    and "Aegis256FullTag = 1" in secure
    and "Aes256Gcm = 2" in secure,
    "protector/wrap wire values must match the 2b.43.2 repository header contract",
)

require(
    "secure-store API exposes only what the mobile shell needs",
    "pub(crate) enum BackupProtectorKind" in secure
    and "pub(crate) enum BackupWrapAlgorithm" in secure
    and "pub(crate) fn as_bytes(&self) -> &[u8]" in secure
    and "BackupProtectorKind" in platform_mod.split("pub(crate) use secure_key_store::{", 1)[1],
    "wire-only protector metadata and secret byte access must remain crate-internal",
)

require(
    "secure returned key material zeroizes",
    "pub struct SecureKeyMaterial(Zeroizing<Vec<u8>>)" in secure
    and 'SecureKeyMaterial([REDACTED])' in secure,
    "OS-returned keyset/root bytes must zeroize and redact Debug output",
)

require(
    "macOS data-protection keychain profile",
    "use_protected_keychain()" in secure
    and "set_access_synchronized(Some(false))" in secure
    and "AccessibleAfterFirstUnlockThisDeviceOnly" in secure,
    "macOS root must be device-only, non-sync, and stored in the data-protection Keychain",
)

require(
    "stable macOS root identity",
    'KEYCHAIN_SERVICE: &str = "org.rustnote.backup-device-root"' in secure
    and 'KEYCHAIN_ACCOUNT: &str = "device-backup-root-v1"' in secure,
    "Keychain service/account must be stable across repository locations",
)

require(
    "atomic non-overwriting macOS root creation",
    "SecItemAdd(" in secure
    and "errSecDuplicateItem" in secure
    and "DeviceBackupRootCreation::AlreadyPresent" in secure
    and "SecItemUpdate" not in secure
    and "set_generic_password" not in secure,
    "root creation must be add-only and a duplicate race must never overwrite the established root",
)


require(
    "macOS root stays in zeroizing ownership",
    "let mut bytes = Zeroizing::new([0u8; BACKUP_DEVICE_ROOT_KEY_BYTES]);" in secure
    and "BackupDeviceRootKey::from_zeroizing(bytes)" in secure
    and "BackupDeviceRootKey::from_bytes" not in secure + "\n" + authority + "\n" + vault,
    "materialized macOS root bytes must never transit an ordinary secret-bearing array",
)

require(
    "macOS root generation and corruption handling",
    "SecRandom::default()" in secure
    and "BACKUP_DEVICE_ROOT_KEY_BYTES" in secure
    and "SecureKeyStoreError::CorruptRootKey" in secure
    and "errSecItemNotFound" in secure
    and "SecureKeyStoreError::MissingRootKey" in secure,
    "root generation must be CSPRNG-backed and missing/corrupt states distinct",
)

require(
    "Android root never enters Rust core",
    '#[cfg(target_os = "android")]\n    fn seal_backup_repository_keyset(' in mobile
    and "non-exportable `AndroidKeyStore` AES-256-GCM" in mobile
    and "Root-key\n    /// material itself must never be exported to Rust" in mobile
    and "BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES" in secure,
    "Android must delegate AES-GCM seal/open to the native Keystore shell without exporting its root key",
)

require(
    "unsupported platforms fail closed",
    secure.count("Err(SecureKeyStoreError::UnsupportedPlatform)") >= 4,
    "unsupported targets must never fall back to plaintext or a filesystem key",
)

require(
    "exact v3 header/keyset authority",
    all(marker in authority for marker in [
        "BACKUP_REPOSITORY_FORMAT_VERSION: u16 = 3",
        "BACKUP_REPOSITORY_HEADER_VERSION: u16 = 1",
        "BACKUP_KEYSET_VERSION: u16 = 1",
        "BACKUP_REPOSITORY_HEADER_PREFIX_BYTES: usize = 92",
        'BACKUP_REPOSITORY_HEADER_MAGIC: &[u8; 8] = b"RNBHDR01"',
        'BACKUP_REPOSITORY_KEYSET_MAGIC: &[u8; 8] = b"RNBKEY01"',
    ]),
    "BackupKeyAuthority must implement the exact byte-contract vocabulary rather than an ad-hoc envelope",
)

require(
    "purpose-separated independent repository keys",
    authority.count("random_repository_key()?") == 4
    and all(name in authority for name in [
        "snapshot_aead_key",
        "blob_aead_key",
        "metadata_aead_key",
        "object_id_hmac_key",
    ]),
    "each repository must receive four independently generated purpose-separated 256-bit keys",
)

require(
    "canonical library-binding vector covered",
    "library_binding_matches_2b43_2_canonical_vector" in authority
    and "library_example_001" in authority
    and "0xf0, 0x36, 0x3a, 0x84" in authority,
    "implementation tests must reproduce the 2b.43.2 library-binding vector",
)

open_body = authority.split("pub(crate) fn open_repository_keyset(", 1)[1].split("\n    }\n}", 1)[0]
require(
    "existing repository open never creates root",
    "create_root_if_absent_for_fresh_state" not in open_body
    and ".open_repository_keyset(&aad, parsed.envelope)" in open_body
    and "existing_repository_open_never_recreates_missing_root" in authority,
    "opening existing state must fail on missing root rather than creating/replacing it",
)

require(
    "fresh initialization is the only root-creation path",
    "initialize_fresh_repository" in authority
    and authority.count(".create_root_if_absent_for_fresh_state()") == 1,
    "root creation must be reachable only through explicit fresh-repository initialization",
)

require(
    "header and keyset parser regression coverage",
    all(name in authority for name in [
        "present_device_root_is_reused_without_replacement",
        "repository_header_rejects_reserved_unknown_and_noncanonical_envelope_fields",
        "repository_keyset_encoding_is_exact_204_byte_canonical_layout",
    ]),
    "the implemented bootstrap parser must directly test root reuse, canonical header rejection, and exact keyset layout",
)

require(
    "fail-closed key-loss and context tests",
    all(name in authority for name in [
        "wrong_device_root_cannot_open_repository_keyset",
        "unavailable_or_corrupt_secure_store_fails_closed",
        "repository_and_library_context_are_authenticated",
        "repository_keysets_are_independent_and_redacted",
    ]),
    "wrong device, unavailable/corrupt store, tampered context and key independence must have direct regression tests",
)

require(
    "no backup repository cutover in 2b.43.3",
    "BackupKeyAuthority" not in all_storage
    and "repository.header" not in backup_repo
    and "repository.json" in backup_repo,
    "2b.43.3 integrates key authority only; encrypted snapshot/blob persistence begins in 2b.43.4",
)


source_contract_helper = text("tests/js/source-contract-helpers.mjs")
source_contract_files = "\n".join(text(path) for path in [
    "tests/js/editor-ui-contract.test.mjs",
    "tests/js/library-wide-privacy-contract.test.mjs",
    "tests/js/navigation-publication-contract.test.mjs",
    "tests/js/note-local-privacy-contract.test.mjs",
    "tests/js/vault-action-privacy-contract.test.mjs",
])

require(
    "Vault source contracts are rustfmt-stable",
    "export function compactRustSource" in source_contract_helper
    and "export function assertRustContains" in source_contract_helper
    and "export function assertRustOrder" in source_contract_helper
    and source_contract_files.count("source-contract-helpers.mjs") == 5
    and "assertRustOrder" in source_contract_files
    and "assertRustContains" in source_contract_files,
    "source-contract QA must compare semantic Rust token order rather than exact rustfmt whitespace",
)

forbidden = ["RUSTNOTE_BACKUP_KEY", "BACKUP_ROOT_KEY_FILE", "backup-root.key", ".backup-key"]
require(
    "no environment/filesystem root-key fallback",
    not any(marker in secure + "\n" + authority for marker in forbidden)
    and "std::env" not in secure
    and "std::fs" not in secure,
    "root keys must never be sourced from environment variables or ordinary files",
)

allows = []
expects = []
for base in [root / "src", root / "crates"]:
    for path in base.rglob("*.rs"):
        body = path.read_text(errors="ignore")
        for i, line in enumerate(body.splitlines(), 1):
            if "allow(dead_code)" in line:
                allows.append((path.relative_to(root).as_posix(), i, line.strip()))
            if "expect(dead_code" in line:
                expects.append((path.relative_to(root).as_posix(), i, line.strip()))
require(
    "staged dead-code expectation is narrow, explicit, and self-expiring",
    not allows
    and len(expects) == 2
    and {item[0] for item in expects} == {"src/platform/mod.rs", "src/security/mod.rs"}
    and all("encrypted backup repository integration" in item[2] for item in expects)
    and "next backup-storage pass" in platform_mod
    and "next backup-storage pass" in security_mod,
    f"only the two staged 2b.43.3 modules may expect dead-code before 2b.43.4 consumption; allows={allows}, expects={expects}",
)

require(
    "current platform boundary wired",
    "mod secure_key_store;" in platform_mod
    and "DeviceBackupKeyProtector" in platform_mod
    and "system_device_backup_key_protector" in platform_mod
    and "mod backup_key_authority;" in security_mod,
    "new secure-store and authority modules must be part of the compiled module graph",
)

require(
    "native strict-warning remediation is source-closed",
    '#[cfg(target_os = "android")]\nuse rustnote_vault::BACKUP_ANDROID_KEYSET_ENVELOPE_V1_BYTES;' in secure
    and '#[cfg(test)]\nuse rustnote_vault::{BackupDeviceRootKey, BackupKeyEnvelopeService};' in authority
    and "if status == errSecSuccess" in secure
    and "else if status == errSecDuplicateItem" in secure
    and "errSecSuccess =>" not in secure
    and "errSecDuplicateItem =>" not in secure
    and "return &macos::SYSTEM" not in secure
    and '.initialize_fresh_repository("library-A")\n                .expect_err("secure store failure must stop initialization")' not in authority
    and 'assert_eq!(\n            managed_path_is_present(&managed_path).expect("observe missing path"),\n            false' not in text("src/security/plaintext.rs"),
    "native Mac feedback must stay closed without broad warning allowances, secret-bearing Debug derives, or Clippy bool comparisons",
)

coordinator = text("src/security/privacy_coordinator.rs")
linearization_test = coordinator.split(
    "fn multi_protected_mutation_acquires_entire_reader_set_before_transition_can_admit()", 1
)[1].split("#[test]", 1)[0]
require(
    "multi-note linearization test never inverts storage then admission",
    "coordinator.inner.admission.try_lock().is_err()" in linearization_test
    and "Restore thread reaches admission attempt" in linearization_test
    and "restore_rx.recv_timeout(Duration::from_millis(50)).is_err()" in linearization_test
    and 'coordinator.multi_note_publication_generation(),\n            before' not in linearization_test,
    "the regression test must inspect admission non-blockingly while it deliberately holds storage",
)

print(f"Backup 2b.43.3 secure-key-store source validation passed ({len(checks)}/{len(checks)} contracts).")
PY

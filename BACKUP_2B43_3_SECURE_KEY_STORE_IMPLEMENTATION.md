# Rustnote 2b.43.3 — OS secure-key-store + BackupKeyAuthority

## Status

Implementation complete at source level. This pass deliberately does **not** cut managed backup persistence over to repository v3; `src/storage/backup_repository.rs` remains the existing repository writer/reader until 2b.43.4.

The remaining release gate is native macOS Rust compilation/Clippy/tests because the sandbox used for this pass has no Rust toolchain and cannot compile the macOS-only Security.framework adapter.

## Governing contracts

This implementation is derived from:

- `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`;
- `BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md`.

No key semantics or wire values are invented outside those contracts.

## Objective

Introduce one narrow OS secure-key-store boundary and one Rust security authority so future encrypted backup storage can obtain a repository keyset without storage code ever handling the OS root-key lifecycle directly.

Conceptually:

```text
storage (2b.43.4+)
        ↓
BackupKeyAuthority
        ↓
DeviceBackupKeyProtector
        ├── macOS data-protection Keychain
        ├── Android Keystore mobile-shell bridge
        └── unsupported platforms → fail closed
        ↓
RepositoryKeysetV1
```

## Implemented architecture

### `src/platform/secure_key_store.rs`

Defines the platform-neutral `DeviceBackupKeyProtector` boundary:

```text
root_status()
create_root_if_absent_for_fresh_state()
seal_repository_keyset(aad, keyset)
open_repository_keyset(aad, envelope)
```

The storage layer never receives this trait or a raw root key.

### macOS

The concrete macOS adapter uses the SecItem/data-protection Keychain profile fixed by 2b.43.1:

```text
data-protection Keychain
non-synchronizing
AccessibleAfterFirstUnlockThisDeviceOnly
stable service/account identity
```

Root creation is add-only:

```text
read status
↓
missing
↓
generate 32 random bytes with Security.framework
↓
SecItemAdd
├── success          → Created
└── duplicate race   → verify established root is readable
```

There is no `SecItemUpdate` and no set/update helper capable of replacing an established root.

Existing repository open never invokes root creation.

The OS-returned root is immediately held in `SecureKeyMaterial(Zeroizing<Vec<u8>>)` and copied only into a `Zeroizing<[u8; 32]>` before entering the canonical `rustnote-vault` envelope service. No ordinary secret-bearing root array remains.

### Android

The Rust core does not import/export an Android root key.

The public mobile shell contract supplies only:

```text
backup_root_status
create_backup_root_if_absent_for_fresh_state
seal_backup_repository_keyset
open_backup_repository_keyset
```

The Android shell is responsible for the contract's non-exportable AndroidKeyStore AES-256-GCM key. The only decrypted material returned to Rust is the repository keyset, which Rust must possess to encrypt/decrypt repository objects in later passes.

### Unsupported platforms

Every secure-store operation returns `UnsupportedPlatform`.

There is no environment-variable, ordinary-file, plaintext, or weak fallback.

## Canonical crypto boundary

The application does not implement AEGIS directly.

`crates/rustnote-vault` now owns the macOS repository-key envelope primitive:

```text
BackupDeviceRootKey
BackupKeyEnvelopeService
```

Exact v1 sizes:

```text
root key                 32 bytes
RepositoryKeysetV1      204 bytes
Apple AEGIS envelope    268 bytes
Android GCM envelope    232 bytes
```

The Apple envelope remains:

```text
32-byte random nonce
+ 204-byte ciphertext
+ 32-byte AEGIS tag
```

Production always uses a fresh random nonce. A deterministic nonce path exists only as a private test helper in `rustnote-vault`.

## BackupKeyAuthority

`src/security/backup_key_authority.rs` implements:

- random 32-byte repository IDs;
- canonical library binding from the 2b.43.2 SHA-256 domain;
- four independent zeroizing 256-bit repository keys:
  - snapshot AEAD;
  - blob AEAD;
  - metadata AEAD;
  - opaque-object-ID HMAC;
- exact 204-byte `RNBKEY01` keyset serialization;
- exact 92-byte `RNBHDR01` bootstrap prefix;
- legal protector/algorithm tuple enforcement;
- wrapped-keyset AAD construction;
- strict bootstrap parsing;
- repository/library context duplication and cross-checking;
- explicit fresh initialization separated from existing-repository open.

### Fresh repository

```text
create root if absent
↓
random repository_id
↓
current library binding
↓
4 independent random repository keys
↓
canonical 204-byte keyset
↓
canonical 92-byte header prefix
↓
OS-root seal
↓
repository.header bytes + live zeroizing keyset
```

### Existing repository

```text
parse strict repository.header
↓
validate fixed fields / tuple / exact length
↓
verify expected repository + library context
↓
open envelope using existing OS root only
↓
decode exact keyset
↓
re-verify duplicated repository + library context
```

There is no call to root creation on this path.

## Failure model

The boundary distinguishes:

```text
UnsupportedPlatform
SecureStoreUnavailable
RootKeyMissing
RootKeyCorrupt
AuthenticationFailed
InvalidEnvelope
InvalidRepositoryHeader
InvalidKeyset
ContextMismatch
RandomSource
LibraryIdTooLong
```

No failure causes a plaintext fallback or root replacement.

## Direct regression coverage added

`rustnote-vault` tests cover:

- exact Apple envelope size;
- round trip;
- random production nonce;
- deterministic private test fixture;
- AAD authentication;
- wrong-root authentication;
- ciphertext tampering;
- truncation;
- root-key debug redaction.

`BackupKeyAuthority` tests cover:

- canonical 2b.43.2 library-binding vector;
- exact Apple repository-header layout;
- exact 204-byte keyset layout;
- fresh root creation and keyset reopen;
- existing root reuse without replacement;
- missing-root open never recreating a root;
- wrong device/root rejection;
- unavailable/corrupt secure-store failure;
- repository/library context tampering;
- reserved bootstrap bytes rejection;
- unknown protector/algorithm rejection;
- illegal tuple rejection;
- wrong envelope length/trailing bytes rejection;
- independent per-repository keys;
- secret debug redaction.

## Deliberate staged boundary

2b.43.3 introduces the authority before storage consumes it.

To avoid fake callers solely to appease `dead_code`, exactly two temporary module-level `#[expect(dead_code)]` boundaries exist:

```text
src/platform/mod.rs      → secure_key_store module
src/security/mod.rs      → backup_key_authority module
```

These are expectations rather than broad `allow` suppressions: once encrypted backup persistence consumes the staged APIs and the dead-code condition disappears, Rust's unfulfilled-lint-expectation check forces the annotations to be removed. No crate-wide dead-code suppression exists.

**The encrypted repository integration pass must remove both expectations when these modules gain their production caller.**

## Explicitly not implemented in this pass

- no `repository.header` filesystem writer;
- no encrypted SQLite snapshot writer;
- no encrypted attachment/blob writer;
- no encrypted manifest/index persistence;
- no Restore decryption path;
- no Protect-time backup-purge removal;
- no repository v3 storage cutover;
- no migration from obsolete development backup formats.

The existing backup repository remains authoritative until the later cutover pass.

## macOS execution constraint

The production adapter intentionally targets the data-protection Keychain rather than the legacy file-based keychain. Access is determined by the host executable's app/code-signing identity. Raw command-line execution may therefore fail closed even when a properly packaged/signed app identity can use the store.

This is not worked around with a legacy-keychain fallback. Native macOS compilation is a 2b.43.3 gate; live production Keychain use begins when 2b.43.4 wires repository creation through this authority.

## Native compile follow-up

The first macOS gate confirmed that the release build and prepared runtime can be produced with the synchronized lockfile, then identified only strict-warning/test-compilation issues in the new secure-key-store modules. The remediation keeps the architecture unchanged and deliberately avoids warning allowances or adding `Debug` to secret-owning aggregates. The corrected source is the candidate to use for the next native gate.


## Native compile follow-up — second macOS round

The second native run confirmed the first warning/import fixes, then exposed two additional QA issues:

- strict `dead_code` failures in the intentionally staged secure-store/key-authority modules when compiling `lib test`; the previous `cfg_attr(not(test), allow(dead_code))` did not cover test targets. These are now two explicit module-level `#[expect(dead_code)]` annotations with self-expiring integration reasons and no `allow(dead_code)` remains;
- an older `assert_eq!(..., false)` in the plaintext ownership regression was rejected by Rust 1.97 Clippy and was rewritten as `assert!(!...)`;
- `multi_protected_mutation_acquires_entire_reader_set_before_transition_can_admit` deadlocked its own test harness by holding storage and then calling an accessor that takes the admission mutex while the worker deliberately held admission waiting for storage. Production coordinator code was not changed. The test now uses `try_lock()` plus a transition-attempt/completion channel, preserving the intended linearization proof without violating the canonical lock order.

This follow-up remains a QA/compile repair; no backup persistence cutover and no production privacy-coordinator behavior changed.


## Native QA source-contract hardening

Final macOS QA exposed that six inherited JS source-contract assertions were coupled to the pre-rustfmt textual layout of otherwise-correct Rust code. The remediation is test-only: one shared helper now normalizes Rust whitespace before ordered-token/containment checks. This keeps the contracts semantic enough to survive `cargo fmt` without weakening the architectural properties they assert. No production Rust behavior changed.

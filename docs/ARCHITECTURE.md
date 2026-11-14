# Rustnote architecture — source baseline 2b.43.3

This is a **system overview**; the detailed privacy, format and backup contracts remain authoritative for exact security requirements. Implementation and verification gaps are summarized in [project status](PROJECT_STATUS.md).

## System boundaries

```text
Dioxus 0.7.10 desktop UI
  |-- ProseMirror editor (pinned local JS bundle)
  |-- PDF.js preview (checksum-pinned, WebKit-targeted local bundle)
  |-- scoped UI privacy publication and session lifetime
  |
Rustnote application services
  |-- SQLite: libraries, note content, FTS, search, revisions and attachments
  |-- Vault/PrivacyCoordinator: managed plaintext, lock and publication lifetimes
  |-- BackupKeyAuthority: staged per-repository keyset and encrypted-v3 format authority
  |
Platform interfaces
  |-- macOS Keychain root-key protector
  `-- other platform interfaces/mobile bridge contract (not shipping-app proof)
```

## Source ownership

| Path | Responsibility |
| --- | --- |
| `src/` | Desktop UI, SQLite/storage, search, privacy coordination and platform adapters |
| `src/security/privacy_coordinator.rs` | Scoped privacy operations, transition and publication authority |
| `src/security/backup_key_authority.rs` | **Staged** backup key/header authority for a later repository cutover |
| `src/platform/secure_key_store.rs` | Platform secure-key-store boundary and macOS Keychain adapter |
| `src/storage/backup_repository.rs` | Existing **format-v2** managed-backup repository writer/reader |
| `crates/rustnote-vault/` | Vault cryptographic primitives and core |
| `crates/rustnote-media-compat/` | Media compatibility crate |
| `editor-spec/`, `editor-src/`, `editor-build/` | Canonical editor-document specification, maintainable source and pinned JS dependencies |
| `pdf-src/`, `assets/` | PDF viewer source and bundled/runtime assets |
| `scripts/`, `tests/` | Preparation, validators, behavioral and integration checks |
| `bundle/macos/` | macOS bundle metadata |

For internal component organization, use the [editor specification](../editor-spec/README.md), [editor implementation guide](../editor-src/README.md), and [PDF source map](../pdf-src/README.md).

## Data and security lifetimes

Ordinary notes use the existing SQLite/FTS5 and attachment architecture. Vault-protected notes/objects require stronger handling of plaintext, encryption, lock transitions, UI publication and cleanup. Relevant sensitive surfaces include **titles, document bodies, attachments and metadata, revisions, FTS/SQLite/WAL, previews, staging, backup, temporary files, OS dialogs and stale asynchronous completion**.

The governing [E0](../SECURITY_ENCRYPTION_E0_CONTRACT.md), [E1](../SECURITY_ENCRYPTION_E1_CONTRACT.md), [E2](../SECURITY_ENCRYPTION_E2_CONTRACT.md) and [E3](../SECURITY_ENCRYPTION_E3_CONTRACT.md) contracts define what is required. This overview does not assert that each adversarial path has been exercised on a native device.

### Backup integration is not complete

- **2b.43.1:** planned key-root authority and failure semantics ([contract](../BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md)).
- **2b.43.2:** planned encrypted backup repository **v3** byte-level format ([contract](../BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md)).
- **2b.43.3:** staged implementation of backup authority and OS protector source ([report](../BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md)).
- **Active repository writer:** `src/storage/backup_repository.rs` still sets `REPOSITORY_FORMAT_VERSION: u32 = 2`. The staged backup key authority is explicitly marked as awaiting storage integration in `src/security/mod.rs`.
- **2b.43.4:** managed encrypted-backup persistence/restore cutover remains future work in this baseline. Final integrated failure, recovery and Keychain behavior are **not** established by static source checks.

## Reproducible browser runtimes

The editor uses exact dependencies pinned in `editor-build/package-lock.json`. The PDF.js preparation uses a fixed `pdfjs-dist@5.7.284` package with a checked SHA-512 and bundles the legacy display/viewer/worker for WebKit. Generated outputs are created under ignored paths in `assets/`, `src/` and `target/`; the source generators remain tracked. The dependency's security caveat and enforced-development mitigations are described in [third-party notices](../THIRD_PARTY_NOTICES.md).

## Design discipline

Maintain a single authoritative owner for sensitive state and lifecycle operations; fail closed on security errors; avoid duplicated logic or unnecessary UI-path work. Audit and plan changes, then independently re-audit behavior against objectives and run negative/concurrency tests. See [Contributing](../CONTRIBUTING.md).

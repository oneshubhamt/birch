# Architecture overview

## System boundaries

```text
Dioxus desktop UI
    |-- ProseMirror browser editor (generated, pinned bundle)
    |-- PDF.js local viewer (generated from checksum-pinned package)
    |-- UI privacy publication/session lifetimes
    |
Rust application services
    |-- SQLite libraries, note metadata, FTS/search, revisions, attachments
    |-- Vault/PrivacyCoordinator: controlled plaintext lifetime and operations
    |-- BackupKeyAuthority (2b.43.3): repository keysets / binding / envelopes
    |
Platform integrations
    |-- macOS Keychain adapter (OS-protected device root)
    `-- other platform interfaces (not equivalent to shipping desktop support)
```

## Data, privacy and backups

- Ordinary notes use SQLite and existing attachment/search structures. Vault-protected notes and objects have additional privacy boundaries and protection rules.
- Vault operations and publication must not leak protected plaintext through filesystem staging, caches, search/FTS, revisions, backups, async UI completion, or OS dialogs. The detailed contracts define the scope and known limitations; this overview is not their replacement.
- `rustnote-vault` owns cryptographic primitives. `src/security/backup_key_authority.rs` coordinates the per-repository keyset, while `src/platform/secure_key_store.rs` exposes platform key protection. Storage must not bypass those authorities.
- **Current stage:** 2b.43.1 fixed the key model; 2b.43.2 fixed the byte format; 2b.43.3 implemented the authority and OS secure-store boundary. 2b.43.4 managed backup persistence cutover is still future work.

## Reproducible browser build

The editor uses exact versions in `editor-build/package-lock.json`. The PDF.js build fetches an exact version with a checked SHA-512. These are prepared into generated runtime files under `assets/`, `src/` and `target/`, deliberately not tracked in Git. The original editor/PDF source and build scripts remain tracked.

## Development philosophy

Preserve authoritative ownership, fail closed on secure-store errors, avoid duplicated state, and keep expensive work out of the interactive UI path. New changes must consider performance and security in tandem, with negative and concurrency tests, not only happy-path tests.

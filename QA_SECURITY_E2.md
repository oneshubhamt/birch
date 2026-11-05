# QA — Security E2 selective note encryption

Build: **0.12.5 / 2a.3**

## Objective check

- [x] SQLite and FTS5 remain the database/search technologies.
- [x] No SQLCipher/WebCrypto/database replacement.
- [x] AEGIS-256/Argon2id continue through the isolated E1 vault crate.
- [x] AEGIS-256 uses the full 256-bit authentication tag for E2 objects, key envelopes and protected-blob records.
- [x] Random per-note key is password-wrapped; password is not a data-encryption key.
- [x] Selective note payload is authenticated/encrypted.
- [x] Current and revision-only attachment blobs have protected streaming paths.
- [x] Protected ciphertext has explicit opaque retention references.
- [x] Persistent FTS and plaintext revisions are removed while protected.
- [x] Sources/citations, PDF annotations and outgoing links are captured then removed from ordinary rows.
- [x] Protected attachment placeholder metadata is sanitized.
- [x] Protected note is read-only in E2.
- [x] Unlock is process-local; lock/delete clear key and materialized attachment state.
- [x] Unsupported plaintext exports are blocked; complete encrypted archive remains supported.
- [x] E3 limitations are explicitly documented rather than hidden.

## Targeted source QA performed in this environment

- Static E0/E1/E2 contract validators.
- TOML parsing for root/vault/tool manifests and rust-toolchain configuration.
- Shell syntax validation for changed validation/build scripts.
- Cargo.lock package/pin inspection.
- Direct-low-level-crypto import scan.
- Executed SQLite schema 15→16 migration and verified it is exactly equivalent to a clean schema-16 database (46 catalog objects).
- Protected-blob retention coverage scan across attachment audit, backup, orphan cleanup and deletion.
- Failure-path cleanup check: ciphertext created before a failed DB commit is reference-checked and removed; staged plaintext is restored on transaction failure.
- Decrypted protected-note JSON scratch buffers are overwritten immediately after deserialization; encryption scratch buffers are likewise cleared after sealing.
- UI exhaustive-match regression check for the four note-only vault actions.
- Export containment scan.
- Protected read-through scan for revisions, sources/citations and PDF annotations.
- Changed-file comparison against build 2a.2 E1 baseline.
- E1 vault cryptographic behavior remains unchanged; E2 adds only a compile-time/full-tag regression assertion and test requiring the full 32-byte AEGIS-256 tag.

## Rust tests added/retained

- protected note hides current plaintext and persistent FTS, rejects wrong password, unlocks correctly and reverses to ordinary state;
- protected attachment ciphertext replaces an exclusive plaintext blob, opaque placeholder metadata is used, authenticated materialization restores exact bytes, and removing protection restores the canonical plaintext blob;
- E1 vault tests continue to cover tampering, wrong keys/passwords, context binding, chunk ordering/truncation and bounded Argon2 parameters.

## Broader architecture QA

- Cumulative B6/B5/B4/B3/B2/A1-A6 static and JavaScript/PDF suites pass through A6.
- A7 then stops on its existing `rustfmt clippy` provisioning-marker assertion. The unchanged 2a.2 E1 baseline fails the same A7 assertion, so this is recorded as a pre-existing validator/baseline issue rather than patched into security E2.

## Environment limitation

The packaging sandbox does not provide `cargo`, `rustc` or `rustfmt`, so executable Rust compilation, Clippy and Rust tests cannot honestly be marked passed here. Build 2a.3 must therefore be closed on the development Mac with the commands provided in the release response.

## E3 carry-forward

E3 must audit historical/derived plaintext: SQLite WAL/free pages, old recovery journals/backups/exports, compatibility/preview caches, temporary handoff/materialization residue and abnormal-crash cleanup. E2 must not be described as forensic-erasure complete before that pass.

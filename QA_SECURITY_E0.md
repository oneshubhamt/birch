# Security E0 — build 2a.1

## Audit findings

- SQLite/FTS5 should remain unchanged during E0; there is no need to replace the DBMS to implement selective note/notebook encryption.
- Plain note content currently has multiple durable representations: note columns, FTS5, revision snapshots and emergency-journal records.
- Automatic backups snapshot the SQLite database and copy attachment blobs, so protected-content semantics must eventually extend through backup paths.
- Attachments are content-addressed by SHA-256 and can be referenced by history/duplication. Protected attachment bytes therefore need a separate protected-blob format instead of mutating an ordinary shared blob.
- PDF search currently lives in the viewer rather than the global FTS index; encrypted-PDF search and global PDF search should share the later search/privacy boundary.
- The dormant `sync_queue` is unrelated to E0 and remains untouched.

## E0 objectives

1. Freeze an explicit, honest privacy/threat model before encryption code exists.
2. Keep E0 out of the production Rustnote dependency/runtime graph.
3. Provide one reproducible benchmark for AEGIS-256, XChaCha20-Poly1305, AES-256-GCM and Argon2id.
4. Keep the cipher/KDF decision open until minimum-device measurements exist.
5. Carry forward the blockchain-inspired idea only as later hash-linked history/device signatures, never as a blockchain dependency.
6. Record plaintext-leak surfaces that E2/E3 must close.

## Changes

- Build ID advanced from `1a.35` to `2a.1`.
- Added `SECURITY_ENCRYPTION_E0_CONTRACT.md`.
- Added isolated `tools/e0-crypto-bench` workspace.
- Added `scripts/run-security-e0-bench.sh`.
- Added `scripts/validate-security-e0.sh`.
- No application schema, storage implementation, UI, editor, attachment protocol, search behavior, or production dependency was changed.

## Required QA

1. `./scripts/validate-security-e0.sh`
2. `cargo fmt --all -- --check`
3. `cargo check --locked`
4. `cargo clippy --locked --all-targets -- -D warnings`
5. `cargo test --locked`
6. `cargo fmt --manifest-path tools/e0-crypto-bench/Cargo.toml -- --check`
7. `./scripts/run-security-e0-bench.sh --quick` (the runner resolves its isolated exact direct pins under `target/`, then runs locked)

Before E1 cipher selection, additionally run:

8. `./scripts/run-security-e0-bench.sh --full` on the oldest intended Intel Mac and Android/ARM64 target (or equivalent native benchmark packaging for Android).

## Validation completed in the implementation environment

- E0 static contract validation: passed.
- Shell syntax validation for the changed/new scripts: passed under Bash-compatible parsing.
- `Cargo.toml` and production `Cargo.lock`: byte-for-byte unchanged from 1a.35.
- `src/` and `editor-src/`: byte-for-byte unchanged from 1a.35.
- Rust compile/clippy/test and live benchmark execution could not run in the implementation sandbox because no Rust toolchain is installed there and outbound DNS is unavailable to install one. These remain target-machine QA, not silently assumed passes.

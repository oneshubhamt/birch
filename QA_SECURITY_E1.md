# Security E1 QA — build 2a.2

## Objective audit

- [x] Dedicated `crates/rustnote-vault` boundary created.
- [x] AEGIS-256 selected as encrypted-format v1.
- [x] Random 256-bit scope-key generation uses OS CSPRNG (`getrandom`).
- [x] Argon2id password-to-wrapping-key path implemented.
- [x] Password-derived material wraps random keys rather than encrypting user payloads directly.
- [x] Versioned/tagged bounded-object format implemented.
- [x] Versioned/tagged password key-envelope format implemented.
- [x] Bounded-memory independently authenticated protected-blob streaming implemented.
- [x] Domain/object identity is authenticated as AAD.
- [x] Secret/key owners use zeroization where practical.
- [x] SQLite/FTS/editor/storage/UI remain unintegrated in E1.
- [x] Build advanced from 2a.1 to 2a.2.

## Unit-test surfaces in `rustnote-vault`

- random keys differ and `Debug` is redacted;
- bounded-object round trip;
- fresh nonce/ciphertext on repeated encryption;
- wrong key rejection;
- wrong object context rejection;
- ciphertext tamper rejection;
- unsupported version/algorithm rejection;
- Argon2id key wrap/unwrap;
- wrong password rejection;
- wrong scope rejection;
- invalid/tampered KDF parameter rejection;
- excessive KDF-resource parameter rejection before derivation;
- password-envelope ciphertext tamper rejection;
- protected blob empty/partial/multi-chunk round trip;
- protected blob wrong-context rejection;
- protected blob authenticated-header tamper rejection;
- protected blob equal-sized record reordering rejection;
- protected blob tamper rejection;
- protected blob truncation rejection;
- protected blob trailing-data rejection;
- declared plaintext length mismatch rejection.

## Required executable QA on a Rust toolchain

```bash
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
./scripts/validate-security-e0.sh
./scripts/validate-security-e1.sh
./scripts/validate-dioxus-0.7.10-baseline.sh
```

Then run the existing Rustnote targeted browser/editor/runtime QA because E1 changes the workspace/dependency graph even though it does not integrate crypto into UI/storage.

## Sandbox limitation

The artifact-generation sandbox used for this pass does not contain `cargo`, `rustc`, or `rustfmt`. Therefore build/test/clippy claims must not be marked as executed here. Static structure/contract validation is run here; executable Rust QA must be run on the developer Mac before E2.

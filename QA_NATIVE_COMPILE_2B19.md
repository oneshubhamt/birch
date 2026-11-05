# Rustnote 2b.19 — QA

## Targeted closure
The 2b.19 validator proves that the two warning-producing symbols and the corresponding `std::io` import are production-only, while the production output recovery ledger still uses the durable application configuration directory and the test ledger still uses an isolated temporary root.

## Available sandbox QA
This sandbox does not provide `cargo`, `rustc`, or `rustfmt`; therefore native Rust compilation is not claimed here. Static/security validators, shell syntax checks, dependency-free browser behavior tests, and archive integrity checks are run locally.

## Required authoritative macOS gate
```bash
cargo fmt --all
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
```

## Sandbox results
- `scripts/validate-vault-2b19-test-target-dead-code-closure.sh`: **PASS**, including inherited 2b.18–2b.11 and E0–E3 2a.16–2a.19 guards.
- Vault 2b.1 through 2b.10 validators run individually: **PASS**.
- cumulative E0–E3 closure in documented static-only mode: **PASS**.
- `node scripts/run-browser-behavior-tests.mjs`: **36/36 PASS**.
- shell syntax over `scripts/*.sh`: **PASS**.
- Rust functional diff versus 2b.18 is limited to production/test conditional compilation in `src/app_paths.rs` and `src/native/output.rs`.
- Native Cargo/rustfmt/Clippy execution remains the required macOS evidence because the sandbox has no Rust toolchain.

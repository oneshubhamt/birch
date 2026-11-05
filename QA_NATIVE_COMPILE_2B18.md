# Rustnote 2b.18 — QA

## Closure scope
The 2b.18 guard checks every source pattern corresponding to the supplied native compiler/Clippy failures and then chains the complete inherited 2b.17 → 2b.1 / E0–E3 static closure chain.

## Available sandbox QA
This sandbox has no `cargo`, `rustc`, or `rustfmt`, so it cannot truthfully claim native Rust compilation. Dependency-free validators, shell syntax checks, source-pattern QA, browser behavior tests, and archive integrity checks are run here.

## Required authoritative native gate
On macOS run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
```

## Sandbox results for the packaged tree
- `scripts/validate-vault-2b18-strict-clippy-closure.sh`: passed, including inherited 2b.17–2b.11 and E0–E3 2a.16–2a.19 guards.
- Vault 2b.1 through 2b.10 static/adversarial validators: passed.
- cumulative `validate-security-e0-e3-closure.sh`: passed in its documented static-only mode because Cargo is unavailable.
- `node scripts/run-browser-behavior-tests.mjs`: 36/36 passed.
- shell syntax (`bash -n scripts/*.sh`): passed.
- lexical delimiter QA over all 18 changed Rust source files: passed.
- architecture validators requiring Cargo: not executable in this sandbox; they remain part of the macOS native gate rather than being represented as passed.

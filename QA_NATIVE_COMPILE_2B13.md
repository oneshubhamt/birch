# Rustnote 2b.13 — Native Compile Closure QA

## Available sandbox QA

The sandbox has no `cargo`, `rustc`, `rustfmt`, or Clippy. Therefore native compilation cannot be truthfully claimed here.

The following source-level/inherited gates are required and are run by the 2b.13 validator where applicable:

- 2b.12 AEGIS/privacy-disclosure closure
- 2b.11 atomic privacy-authority/revision-index closure
- 2a.16 E0–E3 plaintext ownership closure
- 2a.17 E0–E3 residue/backup closure
- 2a.18 E0–E3 UI lifetime/revision restore closure
- 2a.19 E0–E3 independent adversarial closure
- shell/JavaScript syntax checks
- targeted static checks for every compiler-reported integration class

## Native release gate on macOS

Run, in order:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo metadata --format-version 1 >/dev/null   # one-time lock sync/fetch on a fresh machine if required
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
./scripts/validate-security-e0.sh
./scripts/validate-security-e1.sh
./scripts/validate-security-e2.sh
./scripts/validate-security-e3.sh
./scripts/validate-vault-2b13-native-compile-closure.sh
./scripts/validate-architecture-a0.sh
```

Any new compiler error revealed after this first closure set must be treated as another native compile defect and repaired at the source, not bypassed.

# Rustnote 2b.15 — Transport Visibility + Warning Closure QA

## Targeted source checks
- build identity is 2b.15;
- binary protocol has no direct `native::private_temp`, `OwnedPlaintext`, or `PlaintextOperationPermit` access;
- `PlaintextOperationPermit` remains crate-private;
- public transport signatures use opaque public guards rather than private security types;
- preview ownership reservation precedes private-directory materialization;
- ingress authorization is retained by an opaque lifetime guard;
- the obsolete public transport permit/re-authorize methods are gone;
- all exact dead-code/private-interface warnings reported in the preceding native compile log are removed structurally rather than silenced.

## Regression evidence available in this environment
The targeted 2b.15 guard chains the 2b.14/2b.13/2b.12/2b.11 closure guards and cumulative E0–E3 ownership/residue/UI-lifetime/adversarial guards.

The base `validate-security-e3.sh` static section also reaches its native Cargo step successfully; Cargo itself remains unavailable in this sandbox.

## Required macOS gate
Run, in order:
1. `cargo fmt --all && cargo fmt --all -- --check`
2. synchronize `Cargo.lock` once if this extracted package is still rejected by the user's Cargo, then return to `--locked`;
3. `cargo check --workspace --all-targets --all-features --locked`
4. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
5. Vault + workspace tests and project validators.

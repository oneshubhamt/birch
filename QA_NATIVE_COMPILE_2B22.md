# QA — Rustnote 2b.22 Media Lock Test + Validator Closure

## Targeted checks

- Media Lock test no longer calls a blocking transport read after Lock admission.
- Test-only Vault state inspection is `cfg(test)` and crate-private.
- Media file lease remains the authorization owner that delays Lock completion.
- 2b.21 restore-order validation normalizes whitespace before checking the Lock call.
- No production Lock/revocation ordering or backup-ownership semantics changed.

## Environment limitation

The current sandbox does not provide the Rust toolchain, so `cargo fmt`, native compile, Clippy and Rust unit tests must be confirmed on macOS. Static/inherited validators and dependency-free behavioral tests are run here before packaging.

## Completed sandbox QA

- `validate-vault-2b22-media-lock-validator-closure.sh`: PASS.
- Inherited 2b.21 -> 2b.11 and E0-E3 2a.16 -> 2a.19 closure chain: PASS.
- Vault 2b.1 -> 2b.10 validators: PASS.
- Dependency-free editor/PDF behavioral suite: 36/36 PASS.
- All shell validator scripts: `bash -n` PASS.
- All JavaScript module scripts: `node --check` PASS.
- Changed Rust files: UTF-8/conflict-marker integrity PASS.
- Cargo/rustfmt are unavailable in the sandbox, so native compile, Clippy and Rust unit tests remain authoritative Mac-side gates.

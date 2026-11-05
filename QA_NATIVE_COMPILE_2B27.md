# Rustnote 2b.27 QA

## Targeted static checks

- Both `src/main.rs` PDF lifetime regressions use the nonblocking Vault-state test probe.
- Neither regression uses `attachment_resource(...)` as a post-admission revocation probe.
- Both retain the assertion that Lock is incomplete while their sensitive lifetime remains alive.
- The helper is limited to `test` / `internal-test-support` compilation.
- Post-release completion bounds are 15 seconds.

## Inherited validation

The 2b.27 closure validator chains the 2b.26 validator, which recursively covers the current 2b / E0–E3 closure chain.

## Native limitation

Cargo/rustc are unavailable in the packaging environment. The user's macOS toolchain remains authoritative for native compile, Clippy, and Rust tests.

## Packaging-environment results

- `scripts/validate-vault-2b27-main-pdf-lifetime-closure.sh`: PASS, including inherited 2b.26 → 2b.11 and E0–E3 closure chain.
- Vault 2b.1 through 2b.10 validators: PASS individually.
- `bash -n scripts/*.sh`: PASS.
- Dependency-free browser/editor/PDF behavior tests: 36/36 PASS.
- Native Cargo/rustc/rustfmt: unavailable in this environment; macOS run required.

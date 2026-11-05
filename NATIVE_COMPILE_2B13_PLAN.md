# Rustnote 2b.13 — Native Compile Closure Plan

## Baseline

Rustnote v0.12.5 build 2b.12 — AEGIS/privacy-disclosure closure.

## Trigger

The first real macOS native `cargo check --workspace --all-targets --all-features --locked` exposed integration defects that static source validators could not detect: Dioxus RSX parsing failures, missing capability exports/imports, a capability method placed on the wrong facade, Rust ownership/move/borrow errors, one return-type mismatch, and warnings that would later fail the `clippy -D warnings` release gate.

## Objectives

1. Fix every compiler error and warning reported by the first native compile without changing Vault security semantics.
2. Keep privacy-sensitive debug output redacted rather than satisfying trait bounds by exposing managed plaintext paths.
3. Restore one coherent application/storage capability boundary for attachment commands and authorized attachment reads.
4. Preserve atomic privacy publication/authorization semantics while fixing Dioxus/Rust ownership issues.
5. Remove dead finalization state instead of silencing unused-variable warnings.
6. Add a compile-closure static guard for the exact classes of integration defect found by native compilation.
7. Re-run all available inherited Vault and E0–E3 security/adversarial guards.
8. Package a new source build; native Cargo/Clippy/tests remain a required Mac release gate because this sandbox has no Rust toolchain.

## Non-objectives

- No Vault format change.
- No crypto/KDF change.
- No persistence schema change.
- No compatibility layer.
- No feature expansion.

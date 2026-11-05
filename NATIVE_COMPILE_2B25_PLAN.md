# Rustnote 2b.25 — Transport Test Helper Scope Closure Plan

## Audit finding

Native `cargo check --workspace --all-targets --all-features --locked` exposed one test-only compile failure in `src/storage/transport.rs`: the protected PDF transport lifetime regression referenced `protected_note_accepts_vault_reads_for_test` through `super::`, but the helper is defined at `crate::storage` in `src/storage/mod.rs`.

The already-compiling media-server regression uses the correct `crate::storage::...` path. The helper remains `#[cfg(test)] pub(crate)` and therefore is not a production API.

## Objectives

1. Correct the transport test helper path without changing production Lock, Vault, transport, encryption, or authorization behavior.
2. Keep the helper test-only and crate-private.
3. Preserve the 2b.24 tag mutation and transport lifetime fixes unchanged.
4. Admit build 2b.25 through inherited E0–E3/Vault validators without changing schema assumptions.
5. Add a targeted regression guard for the exact module-scope mistake.
6. Re-run inherited static/security and dependency-free behavioral QA, then package with byte-exact archive verification.

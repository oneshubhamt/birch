# Rustnote 2b.32 — QA

## Objective closure

- The reported E0599 root cause is removed structurally: production no longer depends on a `#[cfg(any(test, feature = "internal-test-support"))]` method.
- The raw `contains_note_key` helper remains gated; production receives only the semantic `note_session_is_active` query.
- The semantic query retains the same revocation rule used by the former helper: absent or revoking sessions are inactive.
- Autosave admission remains fail-closed for protected notes without an active session and for pending Vault operations.
- No schema, crypto, object-format, editor, attachment, search, or public-label behavior changes were made.
- The inherited E2 validator was updated to recognize the 2b.31+ explicit public locked-note label UI rather than requiring the obsolete literal `Locked note` string.

## Available environment QA

- 2b.32 build-specific Vault session-query guard: **PASS**.
- 2b.31 inherited native/full-QA guard: **PASS** before its long inherited chain continues.
- Security E0–E3 static closure including 2a.16–2a.19: **PASS**.
- Vault 2b.4 granular cutover: **PASS**.
- Vault 2b.5 consolidation/performance: **PASS**.
- Vault 2b.6 independent I1–I7 closure: **PASS**.
- Vault 2b.11 atomic privacy authority: **PASS**.
- Shell syntax across `scripts/*.sh`: **PASS**.
- Changed JavaScript syntax: **PASS**.
- Dependency-free browser/editor/PDF behavior suite: **37/37 PASS**.
- Full inherited validator chain progresses cleanly through the later native-closure guards but exceeds this sandbox's single-command execution window; targeted inherited security/Vault guards above were therefore rerun individually.
- Native Rust compile, Clippy, and full workspace tests cannot run in this sandbox because it has no Rust toolchain.

## Required Mac gate

1. `cargo fmt --all -- --check`
2. `cargo check --workspace --all-targets --all-features --locked`
3. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
4. targeted encrypted autosave/navigation tests
5. serial full workspace Rust tests
6. full dependency-backed browser suite
7. `./scripts/validate-vault-2b32-native-full-qa-closure.sh`
8. architecture A0 guard

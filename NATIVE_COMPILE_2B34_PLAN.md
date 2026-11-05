# Rustnote 2b.34 — Vault runtime dead-code closure plan

## Audit finding

`VaultRuntime::contains_note_key` became obsolete for production/internal-test-support after autosave admission moved to the semantic `note_session_is_active` query. Its only remaining direct callers are unit tests inside `security::runtime`, but it was still compiled whenever the `internal-test-support` feature was enabled. Because `cargo clippy --workspace --all-targets --all-features -- -D warnings` compiles that feature for the normal library, the unused helper failed strict Clippy with `dead_code`.

## Objectives

1. Compile the raw `VaultRuntime::contains_note_key` compatibility helper only for unit tests.
2. Keep production and internal-test-support callers on the semantic `note_session_is_active` authority.
3. Preserve the `PrivacyCoordinator::contains_note_key` test-support façade because the exported internal-test-support storage fixture still uses it.
4. Add no `allow(dead_code)`/`expect(dead_code)` suppression.
5. Preserve all Vault/privacy/session semantics and all 2b.33 behavior.
6. Increment the source build to 2b.34 and retain inherited closure.

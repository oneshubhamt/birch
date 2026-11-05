# Rustnote 2b.32 — Vault Session Query Compile Closure Plan

## Audit finding

The 2b.31 Vault navigation/autosave correction introduced one production call to `PrivacyCoordinator::contains_note_key`. That helper is intentionally compiled only for tests / `internal-test-support`, so a normal application build fails with E0599 even though test-feature builds can see it.

## Objectives

1. Keep raw/test-oriented key-presence inspection gated to tests.
2. Add one always-compiled semantic query for production: whether a note has an active, non-revoking Vault session.
3. Route encrypted autosave admission through PrivacyCoordinator using that semantic query.
4. Preserve Lock revocation semantics: a revoking or absent session must read as inactive.
5. Make no changes to cryptography, Vault object format, schema, search, attachments, editor state, or locked-note label behavior.
6. Increment the source build to 2b.32 and extend inherited validator build allowlists without changing their behavioral expectations.
7. Add a build-specific static guard so production code cannot regress to the cfg-gated test helper.

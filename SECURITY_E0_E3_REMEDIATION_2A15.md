# Rustnote 0.12.5 build 2a.15 — targeted E0–E3 remediation

This pass addresses three closure-blocking findings from a de-novo audit of build 2a.14. It is deliberately a remediation pass, not a substitute for another independent full E0–E3 closure audit.

## Lock cleanup ownership

`VaultRuntime::remove_note_key` now publishes revocation and waits for active permits, but only snapshots registered plaintext paths. It does not remove the note session/key or unregister cleanup obligations until every registered plaintext path has been successfully removed. If any cleanup fails, the session remains revoking and all cleanup state remains registered for retry.

Regression: `failed_lock_cleanup_retains_session_and_plaintext_for_retry` injects a deterministic deletion failure, verifies that the revoking session and path tracking survive, and then verifies that a later Lock retry removes the plaintext and session.

## Protected attachment materialization ownership

`materialize_protected_attachment` now registers the allocated destination with the active Vault read permit before creating the file or writing decrypted bytes. The tracked transient path spans decrypt/authentication and SHA-256/size verification. Only a successful, still-publishable session may atomically promote it into the materialized-attachment registry. Failure cleanup errors are surfaced and failed cleanup remains registered with VaultRuntime.

Regressions: `tracked_materialization_cannot_promote_after_revocation_begins` covers the Lock race; `concurrent_materialization_reuses_canonical_path_without_orphaning_duplicate` covers duplicate concurrent materialization without registry replacement/orphaning; `failed_protected_attachment_authentication_leaves_no_untracked_plaintext` corrupts protected ciphertext and verifies that failed authentication leaves no materialized plaintext in the Vault temp owner.

## Emergency-journal pre-quarantine privacy

Startup now removes deterministic final, temporary, previous and quarantine records for every protected note before parsing any recovery candidate. This prevents a corrupt protected-note journal from being newly moved into quarantine after an earlier quarantine-only sweep. The protected sweep runs again after recovery as defence in depth. Journal privacy deletion also rejects an existing directory/special node at an expected file path instead of treating it as an absent file.

Regressions: `startup_discards_corrupt_protected_records_before_quarantine` covers corrupt final/tmp/previous files; `startup_fails_closed_when_protected_root_record_cannot_be_removed` covers a deterministic cleanup failure.

## Targeted QA

Required gates for this pass are: static E0/E1/E2/E3 contract validators, the 2a.15 remediation regression guard, source scans for stale late-registration/unregister-before-delete patterns, formatting/compile/clippy/tests when a Rust toolchain is available, and a final re-audit of the three changed ownership boundaries.

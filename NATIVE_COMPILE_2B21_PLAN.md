# Rustnote 2b.21 — Native Test State/Lifetime Closure Plan

## Audit findings

1. `overlapping_multi_note_transition_fails_without_partial_admission` validates all-or-nothing admission correctly, but then probes the free note while the first guard still owns the coordinator-wide storage mutation mutex. The observed hang is test-induced global serialization, not partial admission.
2. `protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables` retains the returned `RevisionRestoreAuthorization` and then calls Lock. That authorization intentionally retains the Vault reader, so Lock must wait; the test was contradicting the lifetime contract it is supposed to preserve.
3. `protected_recovery_cleanup_removes_known_plaintext_record` establishes the emergency-journal durable namespace marker but raw-writes into the namespace root without creating that root. Durable namespace authority deliberately does not materialize a plaintext directory by itself.
4. Ordinary attachment/export Protect-lifetime and derivative-race tests use a two-second completion deadline. On the native debug build, Argon2-backed protection routinely exceeds that deadline after the blocking resource is released, producing false Timeout failures.
5. `ensure_note_protection_ready` treats every library-scoped `managed_plaintext_staging` row (`note_id IS NULL`) as a blocker for one note's Protect operation. Orphan/quarantine and portability ownership can legitimately remain library-scoped after the target note's own attachment mutation has settled, causing permanent false rejection of Encrypt.

## Objectives

- Preserve coordinator-wide storage serialization and fix the multi-note regression test to observe admission rollback without blocking on that unrelated mutex.
- Preserve retained Restore authorization semantics and explicitly release the authorization before a graph-persistence test invokes Lock; retain the dedicated lifetime test as the authority for Lock waiting behavior.
- Materialize the managed emergency-journal root in the raw-fixture test only after durable authority is established.
- Keep finite timeout diagnostics while allowing realistic native debug-build cryptographic latency.
- Restrict the note Protect readiness barrier to durable work explicitly scoped to that note: attachment ingest jobs, attachment operations, and note-scoped managed plaintext staging.
- Do not weaken global/library privacy transitions, startup staging recovery, orphan ownership, portability ownership, or cross-library backup protections.
- Add a 2b.21 static closure guard and rerun inherited Vault/E0–E3 static and dependency-free behavioral QA.

## Native closure required on macOS

The authoritative native gate remains Cargo format/check, strict Clippy, the exact regressions from the user's native run, serial long-running tests, then the full workspace test suite. Native Rust execution is unavailable in the packaging sandbox and must not be claimed here.

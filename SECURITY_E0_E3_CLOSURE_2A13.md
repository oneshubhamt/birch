# Rustnote 0.12.5 build 2a.13 — E0–E3 adversarial behavioral closure

This pass is deliberately bounded. It does not add another Vault architecture. It attacks the 2a.12 security boundaries through the twelve agreed behavioral scenarios, adds direct regression coverage where proof was missing, and fixes concrete defects only if the attack demonstrates one.

## Behavioral attack matrix

| Scenario | 2a.13 acceptance evidence |
| --- | --- |
| Encrypt/read/cache/staging concurrency | `protect_waits_for_active_derivative_then_scrubs_cache_and_stage`; derivative gate producer/transition tests |
| Lock/read races | `lock_waits_for_scoped_payload_read_and_suppresses_late_publication`; runtime revocation tests; transport lease lifetime tests |
| Crash at meaningful transition points | all four durable journal states are covered: protect/prepared, protect/database_committed, unprotect/prepared, unprotect/database_committed |
| Failed filesystem cleanup | `protection_cleanup_failure_stays_journaled_until_recovery_succeeds` |
| Stale async completion | UI generation invalidation/static guards plus `vault_barrier_prevents_stale_plaintext_emergency_journal_recreation` |
| Restart after interruption | all four durable crash-state regressions reopen the library through normal `Storage::at_with_backup` initialization |
| Backup-directory reuse/change | existing durable backup-history/location-change regressions |
| Cross-library isolation | existing repository-owner and backup-history cross-library regressions |
| protected→unprotected→protected cycles | `protected_unprotected_protected_cycle_preserves_payload_and_residue_rules` |
| Attachment replacement/deletion/import races | image Replace routes through the same staged `ImportImages` workflow and note-scoped plaintext-ingress permit; durable ingest/remove operation barrier plus `pending_attachment_mutations_block_encrypt_until_their_durable_state_is_resolved` |
| WAL/FTS/revision/autosave residue | existing E3 v5 SQLite/FTS/WAL/autosave tests plus `revision_only_plaintext_is_removed_from_sqlite_and_wal_on_protection` |
| Temporary-file ownership / abnormal termination | existing v2 owner-lock, cleanup-claim, markerless-session, PID-reuse migration and setup-crash tests |

## Crash-state closure

The durable journal intentionally has only four canonical recovery states. 2a.13 now provides direct executable coverage for each:

1. **Protect / prepared** → roll back retired plaintext and orphan protected output.
2. **Protect / database_committed** → retain protected canonical state, remove retired plaintext and complete managed privacy cleanup on restart.
3. **Unprotect / prepared** → destroy unpublished incoming plaintext and retain protected canonical state.
4. **Unprotect / database_committed** → finish ordinary plaintext publication and retire obsolete ciphertext.

Crashes before a durable prepared row have not crossed a canonical persistence boundary; crashes after journal completion have already reached the terminal state. The pass therefore does not invent additional pseudo-phases merely to increase the test count.

## Cleanup-failure closure

A protection database commit may precede managed filesystem cleanup. If strict cleanup fails, protection returns an error, the canonical note remains protected, the `database_committed` journal row remains durable, and normal startup recovery retries finalization before ordinary integrity checks. The privacy marker is not treated as proof of success while cleanup remains incomplete.

## Transition-cycle closure

Repeated Protect → Lock → Unlock → Remove encryption → Protect cycles must preserve note and attachment payloads while restoring the same persistent-residue rules on every protected phase. Attachment ingest/removal operations remain separate durable barriers and must settle before Encrypt can commit.

## Bounded scope

2a.13 does not reopen 2a.12 architecture or add speculative removable-drive, hostile-filesystem, cross-user, allocator, browser-internal, or physical-media guarantees. It tests the agreed E0–E3 Rustnote-managed boundaries. Native Cargo/rustc/rustfmt/Clippy/Rust execution remains the user's acceptance gate by prior instruction; this environment performs source/static/security/architecture/browser/package QA and verifies that the executable Rust regressions are present and wired into the normal test tree.

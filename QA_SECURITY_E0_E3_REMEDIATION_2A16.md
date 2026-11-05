# Rustnote 0.12.5 build 2a.16 — canonical plaintext ownership QA

## Objective checks

- [x] Schema 18 provides durable generic plaintext staging ownership before library writes.
- [x] Async `.incoming` staging reserves ownership before copy and transfers it transactionally to `attachment_ingest_jobs`.
- [x] Direct `.incoming` staging reserves ownership before copy and transfers it transactionally to `attachment_operations`.
- [x] Failed security-sensitive staging deletion retains an authoritative cleanup owner.
- [x] Image `.ready` → `.claimed` rename updates registry ownership while the registry lock is held.
- [x] Portable import archive staging is owned before copy; only ownership reservation holds the storage mutex, while the durable library-scope row remains the long-lived Protect/backup barrier.
- [x] Portable bundle blobs are owned before first write and transfer ownership transactionally to canonical DB references.
- [x] Permanent delete and Trash purge acquire note plaintext transitions before the operation lock, drain producers, revalidate eligibility, and clean durable staging plus native derivative stages before database owner rows can disappear.
- [x] Portability undo/rollback creates or reuses durable blob-cleanup ownership in the same transaction that removes the final canonical database reference; failed unlink keeps that owner for retry.
- [x] Failed editable-import rollback is propagated rather than discarded best-effort.
- [x] Protect/startup/backup barriers account for unresolved generic staging.
- [x] Complete-archive/repository restore and rollback plaintext staging moved out of the active library into an OS-lock-owned sensitive-temp session; startup cleanup is fail-closed.
- [x] Attachment finalization no longer creates an unowned `.previous` sibling when replacing an invalid canonical blob.
- [x] Targeted namespace audit performed for `.incoming`, `.tmp`, `.ready`, `.claimed`, staging and import writers.

## Static / executable-in-this-environment QA

- `validate-security-e0-e3-ownership-2a16.sh`: PASS after the final delete/purge + portability-undo closure.
- `validate-security-e0-e3-closure.sh` in static-only mode: PASS.
- E0 static validator: PASS.
- E1 static validator: PASS.
- E2 static validator: PASS.
- E3 static validator: PASS after preserving the historical 2a.15 amendment marker.
- Clean-schema-18 vs schema-17 + 17→18 migration SQLite object equivalence: PASS (50 objects each, user_version 18).
- Independent SQLite owner-transfer semantics: PASS (rollback preserves generic owner; commit transfers it; note cascade cannot erase generic cleanup ownership).
- Final delete/purge re-audit: PASS after making purge pagination depend on the discovered candidate-page size rather than the post-revalidation deletion count.
- Added Rust regression: `permanent_delete_waits_for_plaintext_producer_then_scrubs_late_staging`.
- Added Rust regression: `undo_import_keeps_blob_cleanup_owned_until_unlink_succeeds`.

## Native Rust QA limitation

This execution environment does not provide `cargo`, `rustc`, or `rustfmt`. Therefore `cargo fmt --check`, `cargo check`, Clippy and Rust unit/integration tests cannot truthfully be reported as executed here. The source includes targeted Rust regressions so those gates can run unchanged on the Rust-equipped development machine.

## Remaining planned closure work

This QA does not certify E0–E3 complete. 2a.17–2a.19 remain as defined by the preceding audit/repair plan.

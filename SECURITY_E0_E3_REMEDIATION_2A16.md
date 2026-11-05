# Rustnote 0.12.5 build 2a.16 — canonical plaintext ownership

## Purpose

2a.16 addresses the connected E3 ownership defects found after 2a.15. It does not attempt the later backup-history, orphan-quarantine, UI privacy-generation, or protected-revision-restore work. Its narrow invariant is:

> No Rustnote-managed plaintext file may be created, copied, renamed, or materialized unless cleanup ownership already exists; failed cleanup may not discard that ownership.

## Audit and ownership model

The audit found two legitimate ownership domains rather than one universal mechanism:

1. **Process-private derivatives** — image edit/import and PDF staging live inside crash-recoverable app-private temp sessions and are registered in the in-memory derivative owner registry before file creation.
2. **Library-resident staging** — attachment `.incoming`, staged `.rustnote` imports, and newly extracted bundle blobs need durable SQLite ownership because they can survive a process crash in the library itself.
3. **Database-restore staging** — complete-archive/repository restore and rollback copies now live beneath a dedicated OS-lock-owned sensitive-temp process session rather than the active library. The session owner exists before any restore child is created; crash cleanup claims abandoned sessions before startup proceeds.

Schema 18 adds `managed_plaintext_staging` without a note foreign key so hard-delete cannot cascade away the last cleanup authority. Note-scoped attachment stages and library-scoped portability stages are differentiated explicitly.

## Attachment `.incoming` ownership

Both synchronous and asynchronous attachment ingestion now reserve a `managed_plaintext_staging` row before copy/clone begins.

- Async prepare transfers the generic owner to `attachment_ingest_jobs` in the same transaction that inserts the job.
- Direct ingestion transfers the generic owner to `attachment_operations` in the same transaction that inserts the add operation.
- Failed copy/sync/prepare cleanup deletes plaintext first and the generic owner second; cleanup failure leaves the owner durable.
- Recovery/finalization integrity failures use strict discard semantics and retain the ingest row when deletion cannot be proven.
- The legacy protected-ingest scrub now rejects a surviving non-removable staging object instead of deleting its database owner.
- `.incoming` maintenance treats generic ownership rows as tracked paths.

## Image `.ready` → `.claimed` ownership

Image claims now hold the derivative registry lock across filesystem rename and registry-path replacement. There is no fallible operation between a successful rename and `stage.path = claimed`.

All registry-backed staging cleanup uses strict deletion: `Ok(false)` is accepted only when the path is actually absent. A non-removable existing object keeps registry ownership rather than being forgotten.

## Portability/import ownership

The root `.portability-import-*.rustnote` archive is durably owned before it is copied into the library and is deleted before its owner row is released. The storage operation lock is held only while establishing that library-scoped owner; the durable row itself becomes the long-lived Protect/backup barrier during the potentially large copy/import, avoiding an unnecessary global mutex hold.

Portable-bundle blob extraction no longer writes an unowned `.tmp` sibling. For a new blob:

1. reserve a library-scoped `portability_blob` owner for the canonical blob path;
2. create the destination with `create_new`;
3. stream/hash/verify the plaintext under that owner;
4. transfer the owner in the same SQLite transaction that inserts the imported attachment/revision references.

A failed materialization attempts fail-closed cleanup of every blob created in that attempt. If cleanup cannot complete, ownership remains durable. A retry can adopt a valid previously owned blob and transfer that existing ownership atomically instead of leaving a stale startup-deletion instruction.

Portability rollback/Undo now follows the same deletion boundary as Trash: it drains note-scoped plaintext producers before the operation lock and retires note staging while owner rows still exist. If removing imported notes makes a canonical blob unreferenced, the rollback transaction first inserts/reuses a `portability_blob` cleanup owner and only then commits removal of the final database references. Post-commit unlink happens under that durable owner; failure is reported while the owner remains for retry/startup recovery. Failed editable import rollback is no longer ignored.

## Protect, backup, startup and deletion barriers

- Startup recovers `managed_plaintext_staging` before vault-operation recovery and refuses to continue if owned plaintext cannot be removed.
- Protect drains the note plaintext-derivative gate, takes the storage operation lock, retires note-scoped generic staging, then checks readiness.
- Protect readiness counts both note-scoped and library-scoped managed plaintext staging.
- Database snapshots/backups treat any generic plaintext staging row as unresolved work.
- Permanent delete and scheduled Trash purge enter the note plaintext-derivative transition **before** taking the storage operation lock, drain existing producers, revalidate deletion eligibility, then retire generic staging, ingest staging, attachment-operation staging, native image/PDF derivative stages, and Vault runtime plaintext before database rows can cascade away ownership. This closes the late-producer race discovered during the 2a.16 re-audit.
- Deferred maintenance no longer holds the operation lock while entering Trash purge; purge owns its transition-first lock hierarchy independently, avoiding transition/operation-lock inversion.

## Namespace audit classification

The 2a.16 sweep covered `.incoming`, `.ready`, `.claimed`, portability-import staging and relevant `.tmp` writers.

The following do not require `managed_plaintext_staging` because an equivalent stronger owner already exists:

- protected attachment materialization and protected image preview roots: Vault plaintext-path ownership;
- `.vault-incoming` protect/unprotect staging: durable `vault_operations` journal;
- process-private image/PDF staging: derivative registry plus crash-recoverable private temp-session ownership;
- complete-archive/repository restore staging and rollback snapshots: dedicated crash-recoverable private temp-session ownership, with fail-closed startup recovery;
- ordinary rendition/media caches: note-scoped derivative transition plus deterministic cache namespace scrub during Protect;
- explicit external atomic outputs: user-selected external output boundary, not a library persistence namespace.

Historical/offline backup-repository copies and attachment `.orphaned` quarantine remain intentionally outside this pass and belong to 2a.17. Temporary restore/rollback staging itself is closed here under process-session ownership.

## Targeted regressions

2a.16 adds or strengthens checks for:

- failed generic cleanup retaining its durable owner for a successful retry;
- generic attachment-stage ownership disappearing after atomic transfer to an ingest job;
- image claim registry path becoming the `.claimed` path immediately after rename;
- archive restore/rollback staging existing only beneath an OS-lock-owned sensitive-temp session and being fail-closed at startup;
- attachment finalization avoiding an unowned `.previous` plaintext sibling when replacing an invalid content-addressed destination;
- source-order guards proving owner reservation precedes attachment/bundle writes;
- source-order guards proving specialized/canonical ownership is created before generic ownership is released;
- permanent-delete/Trash-purge transition-before-operation-lock ordering and late-producer cleanup;
- portability undo canonical-reference → durable-cleanup-owner transfer, including failed-unlink retry;
- schema-17 → schema-18 migration equivalence with clean schema 18;
- inherited E0/E1/E2/E3 static security contracts.

## Scope boundary

2a.16 does **not** claim final E0–E3 closure. The already identified later work remains: historical/offline backup invalidation and `.orphaned` residue (2a.17), then UI/async privacy lifetime and protected revision restore (2a.18), followed by independent adversarial closure (2a.19).

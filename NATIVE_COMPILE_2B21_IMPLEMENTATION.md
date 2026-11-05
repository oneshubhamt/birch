# Rustnote 2b.21 — Native Test State/Lifetime Closure Implementation

## Implemented changes

### 1. Multi-note admission regression test no longer self-blocks

The test now drops the existing busy-note privacy guard before acquiring a full transition guard for the free note. This preserves the all-or-nothing proof: if failed multi-note admission had partially reserved the free note, that subsequent admission would still fail. Production coordinator ordering and the global storage mutation mutex are unchanged.

### 2. Protected revision graph test releases its retained reader before Lock

`restore_protected_revision_if_unlocked` intentionally returns `RevisionRestoreAuthorization`, which carries the retained Vault access beyond storage commit for protected UI projection/publication. The graph-persistence test now drops that authorization before calling Lock. The separate `protected_restore_normalization_reuses_retained_authorization_without_lock_cycle` test remains responsible for validating the retained lifetime contract.

### 3. Emergency-journal test creates the namespace root after authority exists

The raw cleanup fixture now calls `ensure_private_directory` for the namespace root after `journal_plaintext_namespace` establishes the external durable owner marker. No production namespace behavior changed.

### 4. Native debug completion deadlines widened

Three Argon2-backed concurrency tests retain their initial short assertion that Protect is blocked while plaintext lifetime is active, but use a 15-second completion deadline after releasing that lifetime. This prevents normal debug-build key-derivation latency from being misclassified as a deadlock while still failing finite hangs.

### 5. Note Protect readiness is note-scoped

`ensure_note_protection_ready` now blocks on:

- `attachment_ingest_jobs WHERE note_id = ?1`;
- `attachment_operations WHERE note_id = ?1`;
- `managed_plaintext_staging WHERE note_id = ?1`.

It no longer blocks on all `managed_plaintext_staging WHERE note_id IS NULL`. Those library-scoped rows can represent unrelated orphan/quarantine or portability ownership and are governed by their own durable cleanup/recovery boundaries. This removes a false permanent Encrypt blocker without relaxing target-note pending work.

### 6. Static validation updated

The inherited 2a.16 ownership guard now requires the note-scoped staging barrier and explicitly rejects reintroduction of the global `note_id IS NULL` Protect blocker. Build admission was advanced through 2b.21 without changing older schema/privacy branches.

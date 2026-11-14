# Managed plaintext lifetime and revocation

This document consolidates the durable E3/2a/2b rules for **where Rustnote-managed plaintext may exist, who owns it, and when authority may disappear**. Exact cryptographic requirements remain in [E0–E3](../contracts/README.md).

The central rule is:

> A Rustnote-managed plaintext file must have cleanup authority before the first byte exists, and that authority must survive until the file is conclusively absent or ownership transfers atomically to another authoritative owner.

## Ownership domains

Rustnote deliberately uses several ownership backends because their crash/lifetime requirements differ.

| Lifetime | Canonical owner |
| --- | --- |
| Library-resident attachment/import staging | Durable SQLite ownership rows exposed through `OwnedPlaintext` |
| Protect/Unprotect transition staging | Durable `vault_operations` journal + typed plaintext owner |
| Attachment mutation staging | Durable attachment-operation or attachment-ingest owner |
| Decrypted protected attachment/materialization | Vault session/path registry and live Vault authorization |
| Image/PDF process-private staging | OS-lock-owned private process session + derivative registry |
| Restore/archive temporary plaintext | OS-lock-owned private restore/session root |
| Emergency journal | `DurablePlaintextNamespace` outside the recursively mutable journal tree |
| Persistent image/media derivatives | Dedicated durable namespace ownership |
| Automatic-backup repository plaintext history (current v2) | External durable backup namespace marker + privacy-reset obligation |
| Explicit Save-As/export destination | External/user-owned boundary after deliberate handoff; not Rustnote-revocable managed plaintext |

A raw `PathBuf` is never itself proof of cleanup authority.

## Lifecycle rules

### Reserve before create

Typical library-resident lifecycle:

```text
reserve durable owner
-> create/copy plaintext
-> validate
-> transactionally transfer to specialized/canonical owner
-> retire predecessor only after transfer succeeds
```

A failed create/copy/validation attempts cleanup, but cleanup failure **keeps the owner alive**.

### Transfer without ownership gaps

Rename/copy handoffs must ensure the successor is owned before the predecessor can disappear. When a durable database owner changes, the owner transfer occurs in the same SQLite transaction that creates/removes the corresponding canonical or specialized reference.

Whole-library restore uses the SQLite database switch itself as the ownership handoff: incoming-only paths are pre-owned; old-library-only canonical paths receive cleanup ownership in the incoming database before the switch.

### Absence must be conclusive

Security-sensitive cleanup uses `managed_path_is_present()` / no-follow metadata semantics:

```text
metadata succeeds  -> present
NotFound           -> conclusively absent
any other error    -> absence not proven -> fail closed
```

`Path::exists()`-style boolean collapse is not acceptable for retiring sensitive ownership. Permission errors, symlink loops and I/O failures preserve the cleanup obligation.

### Filesystem type and containment

Managed namespaces reject unsafe path escape and unsupported node types. Current durable-namespace operations verify the namespace marker, real root/parent containment and symlink behavior before creating or strictly deleting managed children.

## PlaintextDerivativeGate

A separate plaintext-derivative authority prevents races between privacy transitions and producers such as:

- image rendition/media compatibility;
- image/PDF staging;
- attachment handoff/materialization;
- attachment/image/import mutation;
- persistent derivative generation.

Protect/Unprotect and destructive privacy-sensitive mutations freeze the relevant producer scope, drain already-authorized producers, then perform canonical mutation and strict cleanup. New producers must re-authorize after the transition.

This producer gate is not a second privacy-transition authority: `PrivacyCoordinator` owns its ordering with Vault revocation and storage serialization.

## Lock and protected-read lifetime

Lock is a revocation barrier for one protected-note session.

Current semantics:

1. transition admission publishes revocation;
2. new protected readers/compound plaintext operations are refused;
3. existing authorized protected lifetimes drain;
4. tracked decrypted materializations are removed fail-closed;
5. the note key/session is dropped only after cleanup succeeds;
6. only then may Lock report success.

A failed plaintext deletion keeps the session revoking and cleanup tracking intact for retry.

For PDF/media/attachment/native operations:

- authorization precedes opening the usable plaintext resource;
- the permit remains alive for the complete read/stream/dialog/handoff lifetime;
- cached PDF session state is metadata only; an open plaintext descriptor is request-scoped;
- media file handles and Vault authorization are bound together;
- Lock cannot complete while such a Rustnote-owned sensitive lifetime remains active.

Deliberate copies already handed to another application or a user-selected destination are outside the revocable internal boundary.

## Publication lifetime

Filesystem ownership is not sufficient for UI privacy. Decrypted values also require a publication lifetime.

Current scopes:

- **note-local:** editor, properties, current-note metadata, revision preview, attachment panel/actions;
- **multi-note:** list/search/backlinks/hierarchy/note picker/tag catalog/link-target registry;
- **library:** complete library replacement/Restore.

A note transition advances the target note generation and aggregate generation. Unrelated note-local state remains valid. Whole-library Restore invalidates all scopes.

Async work captures authorization before it starts and must acquire a current publication lease at the actual state handoff. A stale result is discarded.

## Protect / Unprotect crash model

The durable Vault-transition journal has two persisted phases per direction:

```text
Protect:   prepared -> database_committed
Unprotect: prepared -> database_committed
```

Recovery outcomes:

- Protect/prepared: restore ordinary plaintext locations, remove uncommitted ciphertext.
- Protect/database_committed: preserve protected canonical state, retire plaintext and complete residue cleanup.
- Unprotect/prepared: destroy unpublished plaintext staging, retain protected canonical state.
- Unprotect/database_committed: publish authenticated ordinary plaintext, retire obsolete ciphertext.

The journal row is removed only after finalization succeeds.

## Emergency journal and autosave

Protected-note plaintext is forbidden from durable emergency journaling.

Privacy transitions establish an autosave barrier before mutation. Startup recovery removes deterministic protected-note final/tmp/previous/quarantine records before parsing candidates and fails closed if known protected plaintext cannot be removed.

The journal namespace itself is durably owned outside the mutable journal subtree so partial recursive cleanup cannot destroy the cleanup authority first.

## Backups and historical plaintext

The **current managed backup repository is still v2/plaintext at this source baseline**, so E3 backup privacy obligations remain relevant.

The durable ownership model is:

```text
<user-selected parent>/
  .rustnote-backup-repository-owner   <- durable library-bound authority
  Rustnote Repository/                <- managed repository tree
```

The marker lives outside the recursively deleted repository. Protect publishes a durable privacy-reset obligation before purge. Offline/missing remembered repositories remain pending; Rustnote must not recreate a missing removable mount or treat an empty substitute path as the original. Normal backup operations are blocked while the current repository remains privacy-pending.

Cleanup sequence:

```text
publish privacy-reset obligation
-> verify external namespace authority
-> recursively remove managed repository
-> fsync parent and prove absence
-> recreate minimal clean repository under surviving authority
-> clear obligation
```

The planned encrypted v3 repository will eventually change the privacy reason for Protect-time purge, but it must not remove this protection until encrypted-only persistence and legacy plaintext cleanup are proven.

## SQLite / FTS / WAL residue

Protect scrubs ordinary sensitive rows under `PRAGMA secure_delete = ON` and full synchronous durability. FTS5 is rebuilt from live public rows before a verified WAL truncation.

Current canonical production sequence is **no-VACUUM**:

```text
secure_delete + synchronous FULL
-> notes_fts rebuild
-> verified WAL TRUNCATE
```

This sequence replaced the older per-Protect VACUUM method after the 2b.42.8 forensic experiment. The invariant remains that unique pre-protection plaintext markers must not remain in Rustnote-managed SQLite/WAL/FTS artifacts after successful finalization.

## Restore

Whole-library restore is stronger than a normal snapshot read.

Ordering:

```text
validate/decrypt source in crash-owned private stage
-> drain library plaintext producers
-> revoke/drain all Vault sessions and tracked materializations
-> acquire storage mutation authority
-> establish all incoming/old cleanup owners
-> install files under ownership
-> switch SQLite database ownership
-> validate/finalize
-> retire old-only plaintext
```

Rollback cannot recreate an ownership gap. If incoming-only plaintext cannot be retired before rollback, Rustnote fails closed rather than reinstalling a database that does not own those paths.

UI privacy is revoked and the live editor/autosave state is disposed before canonical library replacement; reconstruction after success or failure uses fresh aggregate publication authority.

## Failure semantics

The following do not authorize dropping ownership or weakening privacy:

- filesystem metadata ambiguity;
- cleanup failure;
- Key/Vault authorization race;
- stale UI completion;
- interrupted process;
- missing removable backup volume;
- corrupt ownership marker;
- failed Restore finalization;
- failed converter/import/export operation.

The system either completes cleanup/transfer, leaves an authoritative retry obligation, or fails closed.

## Physical and external boundaries

Rustnote does not claim deterministic erasure of allocator/WebView history, swap, SSD wear-levelled blocks, APFS/COW snapshots, or plaintext intentionally exported to another application/destination. Whole-device encryption and OS security remain important complementary controls.

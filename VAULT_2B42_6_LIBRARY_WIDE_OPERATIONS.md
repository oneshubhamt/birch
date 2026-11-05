# Rustnote Vault 2b.42.6 — Library-wide operations

## Scope

2b.42.6 separates **synchronizing the current live editor** from **authorizing the actual whole-library snapshot/durability phase**.

The pass covers:

- automatic backup;
- manual complete archive export;
- native-close `checkpoint_all` + database durability checkpoint;
- the explicit manual durability checkpoint;
- complete-archive and backup-repository Restore recovery.

It does **not** change Vault cryptography, granular protected-object storage, note-local publication, aggregate UI ownership, Vault-action behavior, profiling, VACUUM policy, or general dead-code cleanup.

## De-novo findings from the 2b.42.5 baseline

The baseline already had scoped note and multi-note publication authority, but whole-library workflows still used UI publication authority as a durability barrier. The implementation/audit cycle found five material ordering problems:

1. **Archive/shutdown source freshness.** A workflow could synchronize source A and begin its library-wide phase after selection had changed without synchronizing the newer source.
2. **Automatic-backup deferral.** A privacy race could abort a due backup after source synchronization and defer the next attempt instead of deterministically ordering behind the privacy transition.
3. **Restore recovery.** Post-Restore aggregate authority was captured only once. A legitimate concurrent transition could make that capture fail after the editor had already been disposed, abandoning canonical UI reconstruction.
4. **Executor/lease inversion.** Capturing a library lease before obtaining the serialized application-executor turn could park the lease behind an earlier Restore and create a circular wait.
5. **Shutdown `checkpoint_all` cycle.** `checkpoint_all` may perform protected note-local saves. Holding the read-side library lease while running it could let an admitted Lock wait for the lease while the protected save waited for Lock.

The audit also confirmed an exceptional transition path: protected revision Restore performs custom note-transition admission rather than calling the ordinary `admit_notes` helper. The whole-library barrier therefore covers that path explicitly.

## Final architecture

### 1. Dedicated whole-library operation authority

`PrivacyCoordinator` owns `WholeLibraryOperationLease`.

This is deliberately **not** a UI `PrivacyPublicationLease`:

```text
whole-library snapshot / raw durability request
        ↓
coordinator admission mutex
        ↓
no admitted note/library privacy transition?
        ↓ yes
active_whole_library_operations += 1
        ↓
WholeLibraryOperationLease
```

If a privacy transition begins after an operation lease already exists, transition admission is recorded first, affected publication generations are advanced/drained, and then the transition waits for existing whole-library operations to finish:

```text
existing library operation
        ↓
Lock / Protect / Unlock / Unprotect / Restore admitted
        ↓
new library operations blocked
        ↓
existing library lease drains
        ↓
privacy transition continues
```

This preserves the 2b.11 rule that earlier admitted/started work can finish while preventing newer library work from starving a pending privacy transition.

The drain is wired into:

- ordinary note transition admission;
- multi-note transition admission;
- whole-library Restore admission;
- exceptional protected-note revision Restore admission.

UI publication drainage remains a separate lifetime.

### 2. Executor ownership precedes privacy authority

Whole-library UI workflows first obtain one serialized `ApplicationExecutionTurn`.

```text
ApplicationExecutor turn
        ↓
source synchronization / prerequisite stages
        ↓
fresh whole-library authority
        ↓
actual snapshot/durability work
```

This prevents a privacy lease from being held by an operation that is merely waiting in the application queue.

`ApplicationExecutionTurn` is RAII-owned and supports staged blocking work without releasing queue ownership. Existing `ApplicationExecutor::execute` semantics remain detached once submitted, so component cancellation cannot make a later operation overlap work that is still running.

### 3. Narrow type-gated application capability

The coordinator authority is exposed upward only through:

```text
PrivacyOperationStore
        ↓
PrivacyOperationService
        ↓
WholeLibraryOperationLease
```

Only true whole-library application APIs require that opaque lease:

```text
RuntimeService::maybe_create_automatic_backup(&lease)
RuntimeService::durability_checkpoint(&lease)
ExportService::export_archive(&lease, ...)
```

`EditorService::checkpoint_all()` deliberately does **not** require it because that operation can perform note-local protected saves and must precede the read-side library lease during shutdown.

### 4. Canonical two-stage helper

Backup, archive and the explicit manual durability checkpoint use:

```text
already-acquired executor turn
        ↓
source-note / no-current-note authorization
        ↓
synchronize current live editor
        ↓
snapshot selection generation
        ↓
fresh whole-library operation lease
        ↓
selection still the synchronized source and authority was not delayed?
   yes → proceed
   no  → drop lease, synchronize source again, retry
```

The implementation is `synchronize_then_begin_whole_library_operation` plus the shared stage-two helper `begin_whole_library_operation_for_synchronized_selection`.

If stage two had to wait for any admitted privacy transition, the first subsequently available lease is intentionally discarded and the current source is synchronized again. This closes the same-selection freshness interval in which the user could have continued editing while authority was unavailable.

### 5. Automatic backup

```text
backup due
        ↓
acquire executor turn
        ↓
synchronize current source
        ↓
fresh whole-library operation lease
        ↓
create automatic repository snapshot
        ↓
release lease + executor turn
```

The old aggregate UI publication lease is gone. Backup and Lock/Restore now have deterministic coordinator ordering.

### 6. Manual complete archive

The file chooser remains outside the durability boundary. Once the user chooses a destination:

```text
acquire executor turn
        ↓
synchronize current source
        ↓
fresh whole-library operation lease
        ↓
ExportService::export_archive(&lease, destination)
```

Selection changes or a delayed privacy admission force fresh source synchronization before export.

### 7. Shutdown

Shutdown intentionally uses a three-stage model rather than carrying one library lease through `checkpoint_all`:

```text
acquire executor turn
        ↓
synchronize current source
        ↓
checkpoint_all
  (note-local saves; NO whole-library lease held)
        ↓
fresh whole-library operation lease
        ↓
raw WAL/durability checkpoint
        ↓
close window
```

The executor turn spans all stages, so Restore/snapshot work cannot interleave between them. If library authority was delayed by a privacy transition, the source synchronization + `checkpoint_all` stage is repeated before the final lease is accepted.

This is the key deadlock-prevention invariant: **never hold `WholeLibraryOperationLease` while `checkpoint_all` may need protected note-local save authority.**

The existing 15-second fail-safe remains: failure or timeout leaves Rustnote open rather than force-closing.

### 8. Explicit manual durability checkpoint

The `Cmd/Ctrl-S` whole-database durability path was an alternate ungated whole-library path. It now uses the same executor-first two-stage helper and typed lease as backup/archive.

### 9. Restore remains stronger and exclusive

Restore intentionally does **not** acquire `WholeLibraryOperationLease`, because retaining a read-side lease while requesting an exclusive library transition would deadlock.

Its architecture is:

```text
acquire executor turn
        ↓
source-note authority
        ↓
synchronize current editor
        ↓
UI library revocation
        ↓
clear selected plaintext + dispose live editor
        ↓
storage Restore
        ↓
coordinator LibraryPrivacyTransition::Restore
        ↓
replace canonical library + revoke Vault sessions
        ↓
release UI revocation
        ↓
wait for fresh multi-note publication authority + publication lease
        ↓
reconstruct UI from canonical state
```

The same fresh reconstruction path runs after a failed Restore. A transient competing privacy transition may delay reconstruction, but cannot cause a one-shot authorization miss to strand the editor in its revoked state.

### 10. Post-audit portability closure

A de-novo sweep found two portable-export paths that were not represented in the original 2b.42.6 validator. They are now explicit.

**Library portability** (`Settings -> Export library`) is a true whole-library plaintext snapshot and follows the same executor-first two-stage boundary as complete archive:

```text
choose destination
        ↓
acquire executor turn
        ↓
synchronize current source
        ↓
fresh WholeLibraryOperationLease
        ↓
typed export_library_scope(turn, &lease, ...)
```

The generic `PortabilityService::export_scope` rejects `PortabilityScope::Library`, so an ordinary note/notebook export call cannot accidentally bypass the whole-library capability.

**Notebook portability** is multi-note rather than library-wide. It therefore uses a narrower `MultiNotePlaintextOperationPermit` instead of blocking unrelated notes. Storage stabilizes the exact notebook note set using this ordering:

```text
optimistic notebook note-id set
        ↓
acquire scoped multi-note plaintext lifetime
        ↓
acquire storage operation lock
        ↓
re-read notebook note-id set
        ↓
set unchanged? no -> drop and retry
        ↓ yes
verify scope contains no protected note
        ↓
export while permit + storage lock remain held
```

A Protect/Lock/Unprotect/Delete transition on an included note cannot cross the export lifetime, while an unrelated note transition remains concurrent. Scope membership changes are revalidated under storage serialization before any plaintext is emitted.

## Targeted concurrency outcomes

### Backup while B locks

Either ordering is linearized:

```text
backup lease first → Lock B is admitted, blocks newer library work, waits for backup
Lock B first        → backup waits, then re-synchronizes its source before snapshot
```

Unrelated note-local UI remains governed by its scoped publication authority.

### Archive while editing A

A is synchronized before archive authority. If selection changes or privacy authority is delayed, the current source is synchronized again before export.

### Close with dirty protected A

The live editor is synchronized and `checkpoint_all` finishes protected note-local saves before the raw whole-library durability lease is acquired. The serialized executor turn prevents another application operation from interleaving across the boundary.

### Restore with unlocked protected notes

UI library revocation occurs before replacement. Storage obtains the exclusive library privacy transition, which invalidates all publication scopes, revokes Vault sessions and advances the library generation.

### Failed Restore

UI reconstruction waits for fresh post-transition aggregate authority and runs on both success and error paths.

## Non-goals retained for later passes

- 2b.42.7: profiling only;
- 2b.42.8: VACUUM forensic experiment;
- 2b.42.9: dead-code / architecture cleanup;
- 2b.42.10: independent de-novo closure audit;
- 2b.42.11: final native QA and promoted package.

`build-id.txt` intentionally remains `2b.41`; 2b.42.6 is a verified subpass checkpoint, not the final promoted 2b.42 build.

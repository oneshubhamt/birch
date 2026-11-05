# Rustnote Vault 2b.8 — Plaintext Authority Closure

Baseline: `v0.12.5 build 2b.7`  
Resulting source build: `v0.12.5 build 2b.8`

## Scope

This pass is a bounded consolidation repair for the three plaintext-lifecycle families left open by the independent whole-contract audit. It does not redesign the Vault object graph, protected-note format, publication-token model, search architecture, or ordinary-note persistence.

## 1. External output transaction authority

Native Save-As and storage exports now share `native::output`.

For every Rustnote-created hidden output sibling, the sequence is:

```
durable recovery record + live record lock
        ↓
typed OwnedPlaintext reservation
        ↓
materialize hidden .tmp / .previous / directory tree
        ↓
filesystem transfer
        ↓
parent-directory durability sync
        ↓
retire predecessor
        ↓
parent-directory durability sync
        ↓
retire recovery record
```

The recovery ledger lives in durable per-user application configuration storage; it is forbidden to fall back to OS temp. Startup reconciles abandoned transactions before normal application staging begins. A live process retains an OS file lock, so another Rustnote process cannot recover active work.

The authority also provides one destination reservation across processes, canonicalizes the destination parent to collapse path aliases, detects recovery-record corruption with a domain-separated SHA-256 checksum, preserves records while removable/offline destination parents are unavailable, and fails closed on ambiguous filesystem states.

`AtomicFileOutput`, `AtomicDirectoryOutput`, native Save-As, attachment atomic copy/replace, repository copies, Markdown/folder export, portability export, and archive output now delegate to this authority. The old Markdown age/name scavenger and duplicate native replacement state machine were removed.

SQLite archive snapshots are reserved beneath a process-session owner before SQLite materializes plaintext; the owner covers possible SQLite sidecars and startup cleanup.

## 2. Emergency-journal ownership consolidation

The emergency journal now establishes one `DurablePlaintextNamespace` before the journal directory is created or opened.

Its owner marker lives in the library data directory, outside `emergency-journal/`, so deleting part or all of the journal tree cannot delete the cleanup authority first.

Journal temporary files, final files, predecessor files, quarantine files, creation, replacement, deletion and quarantine moves now use `OwnedPlaintext`/typed namespace operations. The production journal no longer has an independent raw `fs::rename` / `OpenOptions::create_new` plaintext lifecycle.

Protected-note journal rejection and startup protected-residue scrubbing remain unchanged and fail closed.

## 3. Persistent derivative ownership and retirement

Ordinary-note image renditions and media-compatibility cache files now have durable namespace owners established before converter/remux output can be materialized.

Temporary outputs are reserved first, converter output is verified as a real regular file, and publication/deletion uses typed namespace operations. Persistent invalidation markers are owned through the same media-cache namespace.

Protected renditions remain ephemeral under their existing Vault/session owner; this pass does not create a second owner for that lifetime.

Attachment retirement ordering was strengthened so persistent image/media derivatives are removed strictly before the final canonical or quarantine plaintext owner disappears. If derivative cleanup fails, the existing canonical/quarantine owner remains as the durable retry anchor. Derivative producers are drained/frozen through a coordinator-owned cleanup guard before destructive attachment/revision-recovery mutations.

## 4. Additional de-novo hardening

The post-implementation independent audit found and repaired one defect in the new namespace abstraction: strict subtree deletion previously had weaker containment semantics than reservation and could also fail unnecessarily for an already-absent nested subtree.

`DurablePlaintextNamespace::remove_subtree_strict` now:

- treats an already-absent subtree as successfully cleaned;
- verifies the real namespace root;
- resolves target parent/path containment to reject intermediate-symlink escapes;
- rejects unsupported filesystem types;
- performs strict removal and parent durability sync after an actual mutation.

Adversarial tests cover reservation symlink escape, strict-deletion symlink escape, absent nested cleanup, and marker-loss refusal for an existing namespace handle.

## Architecture result

The three formerly independent families now reduce to the intended authority model:

```
external export staging  ─┐
emergency journal        ├─> typed plaintext ownership
persistent derivatives   ┘

privacy-sensitive retirement
        ↓
PrivacyCoordinator derivative gate
        ↓
strict derivative cleanup
        ↓
canonical owner retirement
```

No old monolithic `ProtectedNotePayload` representation was reintroduced.

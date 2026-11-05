# Rustnote Vault 2b.4 — Granular protected-note cutover implementation

Build: Rustnote v0.12.5 build 2b.4

Governing specification: **Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model**.

## Closure status

2b.4 is implemented in source. The monolithic protected-note representation is no longer a production persistence path. Protected notes use the granular Vault graph:

```text
vault_note_roots
  -> VaultManifest
       -> NotePrivateState
       -> MetadataState
       -> AttachmentIndex
       -> immutable RevisionObject chain

AttachmentIndex
  -> independently encrypted attachment blobs
```

## Objective verification

- [x] **O1 — Single protected representation.** `vault_note_roots` + `vault_objects` are the canonical protected-note state representation. Schema 21 removes `protected_notes` and `protected_note_blobs`; the only legacy references are the deliberate one-way migration guard/drop SQL.
- [x] **O2 — Granular current graph.** Production uses `VaultManifest`, `NotePrivateState`, `MetadataState`, `AttachmentIndex`, and immutable `RevisionObject`s.
- [x] **O3 — Granular Protect.** Protect classifies `SensitiveNoteData`, writes granular encrypted objects, installs the authenticated manifest root, replaces blob-routing refs, and scrubs ordinary plaintext persistence.
- [x] **O4 — Selective protected reads.** Added `VaultGraphHead` / `read_vault_graph_head`; metadata, attachment, revision-summary and routine mutation paths no longer hydrate unrelated current-state objects. Full graph hydration remains only where the operation genuinely needs all current classes (unlock authentication, unprotect, revision restore, complete note hydration, tests).
- [x] **O5 — Mutation locality.** Routine title/document saves open and rewrite `NotePrivateState`; tags open/rewrite `MetadataState`; attachment metadata opens/rewrites `AttachmentIndex`. Document-linked metadata/index objects are opened only when the document's link/attachment structure actually changes. A due revision may read the attachment index to construct its snapshot.
- [x] **O6 — O(1) revision append.** A new revision reads only the current revision head, appends one immutable `RevisionObject`, and commits a new manifest. It does not traverse/reseal prior history.
- [x] **O7 — Granular revision restore.** Restore reads the selected encrypted revision, captures the pre-restore state as a normal encrypted revision, writes new current objects, and leaves ordinary `note_revisions`, `note_tags`, and FTS empty for protected notes.
- [x] **O8 — Independent attachment bytes.** Large attachment bytes remain independently encrypted; attachment metadata updates do not rewrite ciphertext bytes.
- [x] **O9 — Opaque blob-routing ledger.** `vault_blob_refs` retains only ciphertext routing identity/size needed for locked-note lifecycle, backup and garbage collection.
- [x] **O10 — Granular Unprotect.** Unprotect reconstructs ordinary state/revisions/attachments from the encrypted graph and removes the Vault root/object graph through the canonical privacy transition.
- [x] **O11 — Unlocked editing parity.** Unlocked protected notes can use ordinary editing/mutation behavior; Locked and Trash remain read-only.
- [x] **O12 — Atomic root swap.** New objects and manifest are written inside one SQLite transaction; the new root is installed before superseded mutable objects/old manifest are deleted in that same transaction. Rollback preserves the previous graph.
- [x] **O13 — Fail-closed corruption + GC.** Missing referenced objects fail closed, authenticated small-object ciphertext tampering fails authentication, protected revision maintenance prunes unreachable objects, and stale encrypted blobs are reclaimed only after reference analysis.
- [x] **O14 — Monolithic machinery removed.** No production `ProtectedNotePayload`, monolithic payload writer, or alternate protected-note table writer remains.
- [x] **O15 — Dedicated adversarial guard.** `scripts/validate-vault-2b4-granular-cutover.sh` enforces the representation, locality, crash/root, corruption, GC, protected-editing and no-dual-model invariants.
- [x] **O16 — Inherited regression closure.** E0–E3, 2a.16–2a.19, 2b.1–2b.3, cumulative A1–A7/B1–B6 static architecture validation, shell/JavaScript syntax validation, and dependency-free browser/PDF behavior QA are green in this environment.

## Additional closure defects found and fixed

### 1. Full-graph reads survived on granular mutation paths

The WIP had already cut persistence into separate encrypted objects, but several common operations still called `read_vault_current_graph`, which decrypted `NotePrivateState + MetadataState + AttachmentIndex` together. Routine protected typing therefore still paid for unrelated object classes.

The final pass introduced a root/manifest-only `VaultGraphHead` boundary and moved routine mutation/read paths to selective object opens. This makes the implementation behavior match the 2b.4 locality/performance contract rather than merely having granular storage on disk.

### 2. UI leaked a storage-layer authorization type

`src/ui/note_metadata.rs` directly imported the storage `RevisionSnapshotResource`. The protected-preview lifetime itself was correct, but the dependency violated the established UI → application → storage boundary.

The application note service now owns an opaque `RevisionSnapshotResource` wrapper and exposes only `snapshot()` and `is_publishable()` to UI code. The Vault/storage permit remains below the application boundary.

### 3. Inherited guards were stale after the intentional schema/representation cutover

Several security/architecture guards still asserted schema 20, monolithic payload test names, the pre-2b.4 restore implementation, or brittle old source-layout counts. They were updated conditionally or structurally so earlier-build invariants remain checked while build 2b.4 is validated against its granular equivalent. No compatibility path was added to production code.

## Scope deliberately not entered

- 2b.5 performance benchmarking/tuning beyond removal of algorithmically obvious over-reading.
- Encrypted search/index infrastructure.
- Cloud sync/CRDT work.
- Permanent legacy protected-note reader or dual writer.


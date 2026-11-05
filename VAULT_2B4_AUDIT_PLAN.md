# Rustnote Vault 2b.4 — Granular protected-note cutover

Baseline: Rustnote v0.12.5 build 2b.3.
Governing specification: Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model.

## Audit findings

The 2b.1 object vocabulary exists (`VaultManifest`, `NotePrivateState`, `MetadataState`, `AttachmentIndex`, `RevisionObject`) but production protected-note persistence is still the 2a monolithic `ProtectedNotePayload` in `storage/protection.rs`.

Current production behavior still has these 2a constraints:

- Protect serializes the entire note, all metadata, attachments and complete revision history into one encrypted payload.
- Runtime normalization reseals the full payload.
- Revision creation/restore reads and rewrites the full payload.
- Attachment metadata and blob mappings live inside the full payload.
- Sources/citations/PDF annotations are read-only while protected.
- Note title/document/tags and attachments are blocked while protected, even when unlocked.
- `protected_notes` and `protected_note_blobs` remain the canonical encrypted-note/root and ciphertext-retention tables.
- The 2b.1 `vault_objects`/`vault_note_roots` store is test-only/dead-code staged infrastructure.

## Finite 2b.4 objectives

1. Make `vault_note_roots` + `vault_objects` the sole canonical protected-note state representation.
2. Replace the 2a monolithic payload with a loaded granular graph composed of `VaultManifest`, `NotePrivateState`, `MetadataState`, `AttachmentIndex`, and immutable `RevisionObject`s.
3. Protect writes initial granular objects and atomically installs the manifest root; no monolithic writer remains.
4. Unlock/hydration/read paths open only the granular objects required by the requested data.
5. Protected title/document autosave rewrites `NotePrivateState` only; metadata changes rewrite `MetadataState` only; attachment metadata rewrites `AttachmentIndex` only.
6. Revision creation appends one immutable `RevisionObject` and a new manifest; revision count/history must not affect ordinary protected autosave.
7. Revision restore reads the selected immutable revision, appends a pre-restore checkpoint, rewrites current state objects, and never repopulates ordinary plaintext revision tables.
8. Protected attachment bytes remain independently encrypted/streamed. Metadata-only mutation must not rewrite attachment bytes.
9. Replace the 2a ciphertext retention table with a narrow opaque Vault blob-routing ledger sufficient for backup, hard deletion and garbage collection while notes are locked.
10. Unprotect reconstructs ordinary note/revision/metadata/attachment rows from the granular graph and removes the encrypted root/object graph in the same canonical transition.
11. Enable ordinary editing behavior for **unlocked** protected notes while preserving locked-note and Trash read-only policy.
12. Implement crash-consistent root swaps and remove superseded mutable objects only after the new root is installed within the same SQLite transaction.
13. Add stale-object garbage collection and corruption/missing-object fail-closed validation.
14. Remove `ProtectedNotePayload`, its serializer/open/validation/checkpoint machinery, the monolithic `protected_notes.payload` writer, and obsolete 2a mutation/read-only branches.
15. Add a dedicated 2b.4 validator plus adversarial tests for mutation locality, revision-history independence, manifest commit boundaries, corruption/missing objects, blob lifecycle, protect/unprotect cycles, and protected editing.
16. Re-run inherited E0–E3, 2a.16–2a.19, 2b.1–2b.3 validators and dependency-free behavioral QA; document native Rust-toolchain gates if unavailable.

## Scope constraints

- No 2b.5 performance tuning beyond eliminating algorithmically obvious full-history rewrites.
- No encrypted search infrastructure.
- No cloud/sync redesign.
- No compatibility framework for obsolete development builds. Schema cutover may explicitly reject a pre-2b.4 library that still contains monolithic protected-note rows rather than retain a permanent legacy reader.

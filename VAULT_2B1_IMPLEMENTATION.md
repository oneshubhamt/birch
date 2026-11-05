# Rustnote Vault 2b.1 — Canonical privacy specification and encrypted object-store foundation

Build: `0.12.5 / 2b.1`
Baseline: `0.12.5 / 2a.19`

## Scope

2b.1 implements only the foundation defined by the frozen Vault 2b contract. Production Protect, Unlock, autosave, revision and attachment mutation paths intentionally remain on the proven 2a.19 protected payload until the single 2b.4 cutover. There are no production dual writes.

## Implemented architecture

### Canonical privacy authority

`src/security/sensitive_data.rs` now provides one logical classification authority for note-owned protected state:

- `PublicProtectedNoteStub` — explicit public structural state only;
- `NotePrivateState` — title, title style and canonical editor document;
- `MetadataState` — tags, outgoing links, sources/citations and PDF annotations;
- `AttachmentIndex` — current sensitive attachment metadata plus encrypted-blob mappings;
- `RevisionObject` — one immutable encrypted revision snapshot;
- `SensitiveNoteData` — a non-serializable logical aggregate used to prove classification completeness.

`SensitiveNoteData::classify` exhaustively destructures both `Note` and `NoteDocument` without a catch-all. A future domain field therefore forces an explicit privacy decision at compile time. Markdown/plain-text projections and attachment-name projections are classified as sensitive-derived values rather than duplicated in the new object graph.

The public-persistence specification also classifies current SQLite note-owned surfaces. The 2b.1 static validator derives direct and transitive note ownership from the schema and rejects an unclassified new note-owned table.

### Granular encrypted object vocabulary

The foundation defines:

- `VaultManifest`;
- `NotePrivateState`;
- `MetadataState`;
- `AttachmentIndex`;
- `RevisionObject`;
- opaque random `VaultObjectId` values.

The logical `SensitiveNoteData` aggregate deliberately does not implement `Serialize`/`Deserialize`, preventing it from casually becoming another monolithic encrypted payload.

### Authenticated object identity

Each encrypted object is sealed with authenticated context containing:

- a fixed Rustnote Vault-object context prefix/version;
- object kind;
- payload format version;
- owning note ID;
- opaque object ID.

The corresponding Vault domain is also kind-specific. Moving ciphertext between note IDs, object IDs, logical kinds or format versions therefore fails authentication.

### SQLite object store

Schema version 19 adds only:

- `vault_objects(note_id, object_id, ciphertext)`;
- `vault_note_roots(note_id, format_version, key_envelope, manifest_object_id)`.

Small-object kind, payload version and mutation timestamps are not duplicated as plaintext database metadata. The root's `(note_id, manifest_object_id)` foreign key guarantees that a root cannot refer to an object outside its note namespace or to a missing object.

### Atomic root primitive

Encrypted component objects are inserted before the root is installed. `install_vault_note_root` authenticates and validates the exact manifest ciphertext before updating the root within the caller's SQLite transaction. A committed old root remains authoritative until a new root commit succeeds.

Temporary serialized plaintext buffers used by generic object seal/open operations are explicitly overwritten after use. Revision snapshot checksum serialization buffers are also overwritten on success and relevant error paths.

## Post-implementation audit correction: revision-history scaling

The first implementation draft stored every revision object ID in `VaultManifest` and scanned all revision references during root installation. That would have made ordinary protected-note commits grow with revision-history length, violating invariant I7.

The draft was rejected before release.

The final 2b.1 foundation represents revisions as an immutable encrypted predecessor chain:

```text
manifest
  revision_head
  revision_floor
  revision_count

revision N -> predecessor N-1 -> ... -> retained floor
```

Normal note-state/root commits therefore remain O(1) in revision-history length. The floor permits pruning the oldest retained prefix without rewriting newer immutable revision objects. Full chain traversal occurs only when revision history itself is requested/validated.

## Deliberate non-cutover boundary

The 2a.19 monolithic `ProtectedNotePayload` remains the only production protected-note persistence representation in 2b.1. It reuses the new shared source/PDF/blob DTOs where this removes duplication, but it does not write `vault_objects` or `vault_note_roots`.

The new object-store writer/root APIs have no production callers outside `src/storage/vault_objects.rs`. This is enforced by the 2b.1 static validator.

The monolithic production representation is scheduled for removal in 2b.4, not maintained as a permanent compatibility architecture.

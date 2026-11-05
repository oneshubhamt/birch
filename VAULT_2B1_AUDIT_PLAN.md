# Rustnote Vault 2b.1 — Pre-edit audit and finite implementation plan

Baseline: `0.12.5 / 2a.19`

## Audit map

The 2a.19 `ProtectedNotePayload` contains seven logical privacy classes:

1. **Current note private state** — title, title style, canonical editor document, plus derived Markdown/plain-text projections carried by `Note`.
2. **Private metadata** — tags, outgoing note links, linked source records, citations, PDF annotation JSON.
3. **Current attachment metadata** — attachment identity, filename/MIME/image/edit metadata, plaintext blob identity and lifecycle metadata.
4. **Revision history** — revision identity/number/timestamp plus immutable `RevisionSnapshot` JSON.
5. **Revision attachment references** — attachment/blob references derivable from each immutable revision snapshot.
6. **Blob mapping** — plaintext hash/size to independently encrypted ciphertext hash/size.
7. **Public structural state** — note identity/routing/list-state/timestamps and sanitized attachment/database stubs required while locked.

Ordinary persistent representations scrubbed by Protect are `notes` sensitive columns, `notes_fts`, `note_tags`, `note_revisions`, `revision_attachment_refs`, outgoing `note_links`, `note_sources`, note-owned `citations`, note attachment `pdf_annotation_pages`, sensitive attachment metadata and sync-queue payloads. `note_parent`, notebook placement and list-state flags remain public structural state under the established 2a contract.

## Canonical 2b mapping

| Existing logical data | 2b destination |
| --- | --- |
| title, title style, canonical editor JSON | `NotePrivateState` |
| Markdown body, plain-text projection | sensitive-derived; regenerate, do not duplicate in Vault objects |
| tags, outgoing links, sources/citations, PDF annotations | `MetadataState` |
| current attachment records + plaintext↔ciphertext blob catalog | `AttachmentIndex` |
| each immutable revision snapshot | one `RevisionObject` in an immutable predecessor chain; manifest retains only head/floor/count |
| separate revision-attachment-ref table | represented by revision snapshot attachments; validated against `AttachmentIndex.blobs` |
| current object references + O(1) retained revision-chain head/floor/count | `VaultManifest` |
| note routing/list-state/timestamps | explicit `PublicProtectedNoteStub` allowlist |

## Finite pass objectives

1. Add an exhaustive, default-private `SensitiveNoteData` specification authority and explicit public-stub allowlists.
2. Define final 2b object vocabulary and typed opaque object IDs.
3. Define stable authenticated object context binding note ID + object kind + object ID + object format version.
4. Add final SQLite foundation tables for encrypted small objects and note roots; schema version 19 with 18→19 migration.
5. Implement generic typed seal/open storage helpers whose payload validators independently verify note identity and object invariants.
6. Define and validate `NotePrivateState`, `MetadataState`, `AttachmentIndex`, `RevisionObject`, and `VaultManifest` formats.
7. Add atomic root installation/read helpers without switching production protected-note callers yet.
8. Add focused Rust tests and a dependency-free static 2b.1 validator for format vocabulary, schema, context binding and deny-by-default specification.
9. Keep all 2a.16–2a.19 security validators green after allowing the new build/schema version without weakening their substantive checks.
10. Re-audit the changed source against this checklist and package only after targeted and regression QA.

### Post-audit privacy refinement

The new SQLite object table deliberately persists only `note_id`, opaque `object_id`, and authenticated ciphertext. Object kind, per-object format version, and object mutation timestamps are not separate plaintext columns: kind/version are already authenticated by the Vault domain + object AAD + encrypted payload, while plaintext timestamps would unnecessarily reveal protected activity. The root keeps only the minimum routing material needed to locate and open the encrypted graph: note ID, root format version, wrapped key envelope, and opaque manifest ID.

## Explicit pass boundary

2b.1 does **not** migrate Protect/Unlock/autosave/revision/attachment callers to the new store. It creates the final foundation that 2b.4 will cut over to. It does not introduce the 2b.2 privacy coordinator or the 2b.3 publication-token abstraction.

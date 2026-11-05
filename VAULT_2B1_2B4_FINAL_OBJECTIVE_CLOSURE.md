# Vault 2b.1–2b.4 Final Objective Closure

Baseline: Rustnote v0.12.5 build 2b.5.1 revision-restore publication closure.

Governing specification: **Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model**.

This closure was performed because an independent source audit found that the core object store was sound but a few callers still sat outside the abstractions mandated by passes 2b.2–2b.4. No new security architecture was introduced. The existing authorities were completed and callers were migrated behind them.

## Audit findings closed

1. Some protected mutations still knew the Vault-reader → storage-lock ordering.
2. Protected async publication gaps remained in tag mutation, revision-history refresh, attachment removal/status, export completion/status, and editor link-popover handoffs.
3. Protected tag command APIs still routed through ordinary `note_tags` instead of encrypted `MetadataState`.
4. Revision Restore retained a special post-Restore reader, but its remaining storage serialization still leaked lock-order knowledge to storage.
5. Several selected-note publication surfaces used valid publication leases but did not bind the note/editor generation into the token itself.
6. Earlier validators did not reproduce these omissions.

## 2b.1 — Canonical privacy specification + object-store foundation

No production redesign was required in this closure.

Re-audit confirms:

- `SensitiveNoteData` remains the canonical privacy classification authority.
- the public protected-note stub remains explicit/allowlisted;
- the protected scrub plan is mechanically driven by the canonical specification;
- `VaultManifest`, `NotePrivateState`, `MetadataState`, `AttachmentIndex`, and immutable `RevisionObject` remain the live small-object vocabulary;
- encrypted small objects remain SQLite BLOBs;
- authenticated object context remains bound to note identity, object identity, object kind, and format/context version;
- no production `ProtectedNotePayload`, `protected_notes`, or `protected_note_blobs` representation exists.

## 2b.2 — PrivacyCoordinator + unified plaintext authority

### Canonical protected mutation authorization

`PrivacyCoordinator` now owns three mutation-specific boundaries in addition to ordinary privacy transitions:

- `ProtectedNoteMutationGuard` — one unlocked protected-note mutation;
- `ProtectedNotesMutationGuard` — one storage mutation spanning multiple unlocked protected notes;
- `RetainedProtectedNoteMutationGuard` — storage serialization for an already-retained protected reader after an authorized mutation such as Restore.

Storage callers no longer acquire a fresh Vault reader and then manually acquire the storage mutex. The coordinator owns that ordering.

### Canonical protected Restore boundary

`ProtectedNoteRestoreGuard` now owns the exceptional Restore acquisition sequence:

1. acquire the existing unlocked Vault reader while note admission is clear;
2. admit the `Restore` privacy transition;
3. drain plaintext producers through the transition;
4. serialize the storage mutation;
5. release Restore admission while retaining the exact Vault reader needed for the already-authorized post-Restore normalization lifetime.

If another privacy transition wins after reader acquisition, Restore fails closed and the temporary reader is released.

Post-Restore normalization calls `begin_retained_protected_note_mutation`, which takes the storage boundary and rechecks whether the retained permit has been revoked. A Lock that wins first therefore causes normalization to fail closed rather than deadlock or write behind revocation.

### Protected mutation callers migrated

The combined guard is used by protected note saves, protected metadata changes, protected attachment mutation, protected revision maintenance, protected tag catalog mutation, protected runtime normalization, and protected Restore.

Plaintext ownership remains through `OwnedPlaintext`; this closure does not reintroduce a second cleanup/ownership system.

## 2b.3 — Publication-token consolidation

### Semantic note publication scope

Feature code no longer composes raw library/note generation checks. `ui/privacy.rs` owns the composition and exposes semantic note publication capture/scope APIs.

A note-scoped token binds:

- UI privacy generation;
- coordinator privacy authorization;
- library generation;
- note/editor generation.

Raw `.with_library_generation()` / `.with_note_generation()` composition remains private to `ui/privacy.rs`.

### Remaining async publication surfaces migrated

The following previously uncovered surfaces now retain note-scoped authorization and use a publication lease for the actual protected-derived state handoff:

- protected tag removal;
- global tag rename/delete result publication;
- revision-retention/history refresh;
- protected attachment removal, including decrypted attachment filename/status;
- note export completion/path/status;
- editor link-popover selected text/current URL capture;
- async link insertion into the protected editor.

Backlinks, note picker/search, hierarchy, properties, sources/citations, revision preview and other selected-note panels now also bind both library and note/editor generations into the token itself. Request counters remain only for ordering/debounce, not privacy authorization.

### Restore publication remains two-phase

The 2b.5.1 Restore repair remains intact:

- pre-Restore token validates pre-mutation work;
- Restore admission invalidates it;
- post-Restore publication captures fresh coordinator authorization while preserving the operation's original UI/library/note generations;
- the returned restored state is published only under the fresh lease.

## 2b.4 — Granular protected-note cutover

### Protected tag command cutover

`remove_tag_from_note` now first attempts a protected metadata mutation. For a protected note, the tag is changed only inside encrypted `MetadataState`; no `note_tags` relationship is recreated.

Global tag rename/delete now:

1. identifies the current protected-note set;
2. obtains one fail-closed multi-note protected mutation guard before changing ordinary or protected state;
3. rechecks the protected-note set under the protected mutation boundary;
4. mutates every unlocked protected note's encrypted `MetadataState` in the same storage transaction as the ordinary catalog mutation;
5. fails before any mutation if any protected note cannot be authorized.

This intentionally favors consistency and privacy over partial global tag mutation. Because locked encrypted metadata cannot be inspected, a global rename/delete requires all protected notes to be unlocked.

Direct storage and `NoteStore` capability tests exercise protected remove/rename/delete through the actual tag APIs and verify that protected tag relationships remain absent from plaintext `note_tags`.

### Granular object model retained

The closure does not alter the successful 2b.4 object model:

- typing/autosave targets current private state rather than revision history;
- metadata operations target encrypted metadata state;
- attachment metadata targets the encrypted attachment index;
- revisions remain immutable encrypted objects;
- attachment bytes remain independent streaming encrypted blobs;
- manifest/root publication remains transactional;
- corrupt/missing objects fail closed;
- unreachable encrypted objects remain garbage-collectable;
- no dual monolithic protected representation has been reintroduced.

## Cleanup and architectural result

The closure adds no compatibility framework, repository hierarchy, duplicate epoch, cleanup registry, or alternate protected representation.

The key architectural result is that ordinary protected mutation callers no longer encode the Vault/storage ordering themselves, while UI feature callers no longer assemble the generation recipe for selected-note publication themselves.

## Native release-gate limitation

This execution environment does not provide `cargo`, `rustc`, or `rustfmt`. Therefore Rust compilation, Clippy, formatting verification, and the 254 discovered Rust tests are **not claimed as executed** here. They remain the required native macOS release gate.

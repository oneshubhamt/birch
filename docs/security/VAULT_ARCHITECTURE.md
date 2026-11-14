# Vault architecture — current canonical model

**Application baseline:** Rustnote v0.12.6 / build 2b.43.3 source snapshot.  
**Purpose:** current architectural reference, not a replacement for the exact [E0–E3 contracts](../contracts/README.md) or a claim of native security certification.

The Vault architecture is the result of the E0–E3 security series, 2a.6–2a.19 remediation, the 2b.1–2b.12 architecture consolidation, and the 2b.42 scoped-publication/closure series. Historical pass reports are summarized in [VAULT_EVOLUTION.md](VAULT_EVOLUTION.md); managed plaintext lifetime is specified operationally in [PLAINTEXT_LIFETIME.md](PLAINTEXT_LIFETIME.md).

## Permanent invariants

The current implementation is organized around seven architectural invariants:

| Invariant | Current meaning |
| --- | --- |
| **I1 — ownership precedes plaintext** | No Rustnote-managed plaintext path is created or materialized unless an authoritative cleanup owner already exists. |
| **I2 — ownership survives plaintext** | Cleanup ownership cannot retire until absence is conclusively proven or ownership transfers atomically to another authority. Filesystem observation errors are not treated as absence. |
| **I3 — one privacy-transition authority** | `PrivacyCoordinator` owns transition admission, conflict handling, reader/producer revocation and storage-mutation ordering. Feature code does not compose those primitives manually. |
| **I4 — conflicts fail closed** | Same-note, multi-note and whole-library privacy mutations are admitted under one coordination model; conflicting transitions cannot silently queue through an unsafe authorization gap. |
| **I5 — publication requires current authorization** | Decrypted/protected-derived UI state requires current note/multi-note publication authorization and an actual publication lease at the handoff. |
| **I6 — sensitive persistence is deny-by-default** | `SensitiveNoteData` and `ProtectedPublicPersistenceSpec` define what protected state is sensitive and mechanically drive ordinary-persistence scrubbing. |
| **I7 — protected mutation cost scales with what changed** | Common protected edits operate on granular encrypted objects rather than re-encrypting the whole note or traversing full revision history. |

These invariants are more durable than any one historical pass number.

## Canonical protected-note representation

The production protected-note store is granular. The old monolithic `ProtectedNotePayload`, `protected_notes`, and `protected_note_blobs` persistence model is no longer the steady-state representation.

Current SQLite schema version is **21**. Protected state is rooted by:

```text
vault_note_roots
  -> encrypted VaultManifest
       -> NotePrivateState
       -> MetadataState
       -> AttachmentIndex
       -> RevisionIndex
       -> immutable RevisionObject objects

AttachmentIndex
  -> independently encrypted attachment blobs
```

The current source constants use Vault manifest format **2** and Vault note format **3**. Each small Vault object is authenticated against its note, object kind, object ID and format context. A committed root points to a complete authenticated graph; replacing the root and retiring superseded mutable objects occurs in the same SQLite transaction.

### Privacy classification

`src/security/sensitive_data.rs` is the current classification authority.

- `PublicProtectedNoteStub` contains deliberately public structural/routing state only.
- `NotePrivateState` contains private title/title-style/canonical editor state.
- `MetadataState` contains sensitive tags, links, sources/citations and PDF annotation state.
- `AttachmentIndex` contains sensitive attachment metadata and encrypted-blob mappings.
- `RevisionIndex` is a compact encrypted retained-revision locator.
- `RevisionObject` stores an immutable encrypted revision snapshot.
- Derived Markdown/plain-text projections are regenerated rather than duplicated as independent Vault persistence.

The classification is intentionally exhaustive: adding note/document fields forces the privacy authority to be reconsidered rather than silently inheriting a plaintext policy.

## Cryptographic boundary

Low-level production cryptography remains isolated in `crates/rustnote-vault`.

Vault v1 uses the fixed profile established by E0/E1 and 2b.12:

- AEGIS-256;
- 256-bit keys;
- random 256-bit nonces for bounded objects/key envelopes;
- the full 256-bit authentication tag;
- random 256-bit note keys;
- Argon2id v=19, 64 MiB, 3 passes, 4 lanes, 128-bit salt, 256-bit wrapping key;
- authenticated domain, object identity, version and purpose context;
- streamed protected blobs with a random 192-bit prefix plus a 64-bit record counter.

Passwords protect random note keys; passwords are not direct content-encryption keys. Managed-backup cryptography is a separate domain and key hierarchy; see [backup authority](../contracts/BACKUP_AUTHORITY.md).

## PrivacyCoordinator

`src/security/privacy_coordinator.rs` is the semantic authority for privacy synchronization. Its current responsibilities include:

- note, multi-note and whole-library transition admission;
- plaintext-derivative producer gating;
- Vault reader revocation/drain;
- protected mutation serialization;
- compound plaintext-operation authorization;
- whole-library durability/snapshot leases;
- note-local and multi-note publication authorization;
- scoped publication lease drainage.

Operations that require multiple privacy authorities receive them from the coordinator at one linearization point rather than composing lock order in feature code.

Examples:

- `PlaintextOperationPermit` binds a note derivative lifetime and current Vault authorization.
- `ProtectedNoteMutationGuard` binds Vault access with storage mutation serialization.
- multi-note protected mutations acquire the complete reader set under one admission critical section.
- `WholeLibraryOperationLease` guards snapshot/durability phases but is deliberately not held across note-local `checkpoint_all` saves.
- protected revision Restore has an exceptional retained-reader path whose ordering remains coordinator-owned.

## Scoped publication model

The former single global UI privacy epoch evolved into three scopes:

```text
library generation
multi-note generation
per-note generation
```

A transition for note B invalidates B-local and aggregate/multi-note publication without invalidating unrelated A-local publication. A whole-library Restore invalidates all scopes.

`UiPrivacyState` mirrors these scopes for immediate fail-closed rendering. `PrivacyPublicationToken` combines UI-local scope freshness with coordinator-backed authorization; the actual protected-derived handoff acquires a short publication lease.

Durable rules:

- source-note synchronization and target-note publication use separate authorization;
- note-local panels/caches depend on the selected note's privacy generation;
- aggregate list/search/backlink/tag surfaces depend on multi-note authority;
- stale asynchronous results are discarded rather than published;
- selected-note Vault actions do not force unrelated notes through the target transition;
- canonical post-transition publication uses fresh target authorization;
- protected native dialogs/resources retain authorization for their complete sensitive lifetime.

## Revision architecture

The early granular design used a predecessor chain; 2b.11 introduced an encrypted `RevisionIndex` to avoid history walks for list/preview/restore.

Current behavior:

- normal protected current-state/metadata/attachment saves do not traverse revision history;
- checkpoint append writes one immutable `RevisionObject` plus a small replacement `RevisionIndex`;
- list UI opens only the compact encrypted index;
- preview/restore resolves and decrypts the selected retained revision only;
- direct object existence is not authorization: selected handles must still belong to the current retained index;
- protected revision restore retains authorization through storage mutation, editor replacement/normalization and final UI publish/discard.

## Ordinary content remains ordinary

Vault is selective encryption, not a rewrite of Rustnote.

- ordinary notes remain SQLite/FTS5-backed;
- the editor remains ProseMirror;
- PDF rendering remains PDF.js;
- ordinary attachments retain their normal storage model;
- persistent FTS contains ordinary content only;
- protected content does not enter persistent FTS while locked or unlocked;
- whole-database encryption, encrypted notebooks, encrypted global search and cloud sync are not implied by the Vault subsystem.

## Current SQLite residue policy

The original E3 contract required `secure_delete + FTS rebuild + WAL truncate + VACUUM + WAL truncate`. Later 2b.42.8 forensic testing led to one canonical production sequence:

```text
PRAGMA secure_delete = ON
PRAGMA synchronous = FULL
-> FTS5 rebuild from current live rows
-> verified wal_checkpoint(TRUNCATE)
```

The per-Protect `VACUUM` and duplicate post-VACUUM checkpoint were removed. This is a later implementation amendment to the *method*, not a weakening of E3's requirement that Rustnote-managed SQLite/FTS/WAL plaintext residue be removed.

## Current-source boundaries worth preserving

- `src/security/sensitive_data.rs` — privacy classification and scrub specification.
- `src/security/privacy_coordinator.rs` — transition/publication/compound-authorization authority.
- `src/security/plaintext.rs` — typed managed plaintext ownership and durable namespaces.
- `src/storage/vault_objects.rs` — encrypted object graph/root authority.
- `src/storage/protection.rs` — Protect/Unprotect persistence and residue scrub.
- `src/ui/privacy.rs` — UI scoped privacy state and publication tokens.

Do not reintroduce a second transition coordinator, monolithic protected-note persistence, feature-local publication-generation recipes, raw managed-plaintext lifecycle, or compatibility machinery for obsolete development-only Vault formats.

# Rustnote Vault 2b.11 — Atomic Privacy Authority Closure

## Baseline

Rustnote v0.12.5 build 2b.10.

This closure pass repairs issues found by the independent de-novo audit of the full 2b contract. It does not add a second privacy architecture or compatibility layer.

## Frozen objectives

1. Eliminate the one-note derivative-permit -> Vault-reader circular-wait window.
2. Eliminate partial reader acquisition for multi-note protected mutations.
3. Make full-lifetime Open / Reveal / Save As / export / PDF / media / attachment flows retain one coordinator-issued compound authorization.
4. Remove superseded caller-side assembly of plaintext-derivative and Vault authorization.
5. Replace revision preview/restore history walks with direct retained revision resolution.
6. Preserve authoritative retained-membership semantics for direct revision handles.
7. Keep normal protected autosave independent of revision-history length.
8. Harden I1 by confining nominal durable ownership construction to canonical durable-ledger modules.
9. Add adversarial/static guards for both deadlock classes, reverse lock order, direct revision lookup, retained membership, hot-path locality, and constructor ownership boundaries.
10. Re-run the complete available inherited Vault, E0-E3, architecture, browser, and PDF QA before packaging.

## Implementation

### 1. Atomic one-note compound authorization

`PrivacyCoordinator::acquire_plaintext_operation()` now acquires, while holding the same admission mutex:

- the note plaintext-derivative permit; and
- the currently available Vault note reader.

It returns a single `PlaintextOperationPermit` that retains both lifetimes. A privacy transition can therefore linearize only before or after this pair, never between the two acquisitions.

`begin_plaintext_operation_mutation()` serializes storage for an already-authorized operation without re-entering transition admission. This is essential: re-entering admission after a Lock has been admitted would recreate the original circular wait.

### 2. Atomic multi-note protected mutation

`PrivacyCoordinator::begin_protected_notes_mutation()` canonicalizes the complete note set, waits for all relevant admission scopes to be clear once, acquires every Vault reader while still inside that one admission critical section, then acquires storage serialization before releasing admission.

This prevents a library or overlapping note transition from entering between reader A and reader B.

### 3. Full sensitive lifetime uses the compound permit

The attachment/file-command, PDF export, note export, attachment transport, media server, compatibility worker, attachment protocol, and attachment-ingress paths now retain or pass the single compound `PlaintextOperationPermit` rather than independently acquiring a derivative permit and later requesting Vault access.

Standalone production derivative acquisition was removed; the narrow method remains test-only for gate-specific adversarial tests.

### 4. Direct encrypted revision resolution

A compact encrypted `RevisionIndex` is now a first-class Vault object and is referenced from `VaultManifest`.

Each retained entry contains only:

- opaque encrypted revision object ID;
- revision number;
- snapshot byte size; and
- creation timestamp.

The index contains no revision snapshot plaintext. Revision list UI reads only this compact encrypted object. Preview/restore resolve the selected opaque handle through the current index and decrypt only that selected `RevisionObject`.

Direct object existence alone is not authorization: an otherwise valid same-note orphan/stale revision object is rejected unless it is a member of the current authenticated retained index.

### 5. Mutation locality

Normal protected note-state/metadata/attachment-index saves leave `revision_index` unchanged. A checkpoint append writes:

- one immutable `RevisionObject`; and
- one small replacement `RevisionIndex`.

Revision retention/pruning is allowed to walk retained history because it is maintenance work, not the typing/autosave hot path.

### 6. Vault format

The new manifest graph is intentionally a new development format:

- `VAULT_MANIFEST_FORMAT_VERSION = 2`
- `VAULT_NOTE_FORMAT_VERSION = 3`
- Vault cryptographic domain `RevisionIndex = 10`

No old/new dual writer or compatibility architecture was added.

### 7. I1 constructor hardening

The existing durable ownership implementation already persists or verifies the authoritative ledger before constructing `OwnedPlaintext`. A crate-visible "reservation token" would not be genuinely unforgeable inside the same crate and would add ceremony without stronger authority.

Instead, 2b.11 freezes every production nominal durable constructor to exactly one canonical ledger module:

- staging -> `src/storage/plaintext_staging.rs`
- Vault-operation journal -> `src/storage/protection.rs`
- attachment-operation journal -> `src/storage/attachments.rs`
- attachment-ingest journal -> `src/storage/attachment_ingest.rs`
- external-output journal -> `src/native/output.rs`

The inherited 2b.8 and E0-E3 validators continue to prove the ordering inside those authorities. A future caller cannot silently widen the constructor surface without failing the 2b.11 closure guard.

## Architectural result

The remaining 2b coordination rule is now:

> Any operation requiring more than one privacy authority receives the complete authority from `PrivacyCoordinator` at one linearization point; callers do not compose privacy lock order themselves.

This closes the two deadlock classes found by the independent audit while preserving the existing ownership, publication-token, granular object-store, and ordinary-note performance architecture.

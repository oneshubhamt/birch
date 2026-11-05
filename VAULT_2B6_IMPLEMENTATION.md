# Rustnote Vault 2b.6 — Independent De-Novo Architectural & Security Closure

## Status

**Source-level 2b.6 implementation and independent closure audit complete. Native Rust release qualification remains mandatory because this execution environment has no `cargo`, `rustc`, or `rustfmt`.**

Contract build ID: `2b.6`

Implementation baseline: `rustnote_v0.12.5_build_2b.5.3_consolidation_performance_closure_source`

Governing scope: Pass 2b.6 of the original **Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model** contract.

2b.6 began from I1–I7 and the source tree itself. Existing 2b.1–2b.5 validators were treated as regression evidence only, not as proof of correctness. No architectural expansion was performed; only defects discovered by the independent audit were repaired.

## 1. Audit objectives

The finite closure objectives were:

1. reconstruct I1–I7 independently from production source;
2. attack every managed plaintext creation, destruction and ownership-transfer boundary;
3. reconstruct privacy-transition admission, revocation/drain and storage-mutation ordering;
4. audit every protected asynchronous UI publication boundary and post-transition reauthorization path;
5. reconstruct encrypted-object/root commit semantics and crash boundaries;
6. re-audit revisions, attachments, backups, import/export, autosave, SQLite/WAL/FTS, restore, hard deletion, startup recovery and protected↔unprotected cycles;
7. perform the six mandatory source sweeps: direct SQLite writers, sensitive filesystem writers, transition callers, raw `PathBuf` plaintext lifecycle, UI publication, and obsolete protected representation/dead code;
8. fix only defects found, add independent regression guards, rerun cumulative security/architecture/behavioral QA, and compare the final tree against the 2b.5.3 baseline.

## 2. Defect found and fixed — I2 absence proof was not fail-closed

### Problem

The canonical plaintext authority and durable transfer-retirement path still used `Path::exists()` for several security-sensitive lifecycle decisions.

`Path::exists()` does not prove absence. Metadata failures such as permission/I/O/path-resolution errors are collapsed into `false`. At an ownership-retirement boundary this creates an unsafe semantic:

```text
metadata check fails
        ↓
Path::exists() == false
        ↓
code may treat plaintext as absent
        ↓
durable cleanup ownership can retire
```

That contradicts I2:

> Ownership cannot disappear before plaintext disappears.

### Repair

`OwnedPlaintext` now owns one conclusive path-state primitive:

```text
symlink_metadata(path)
    success  → entry exists
    NotFound → entry absent
    any other error → fail closed
```

Production `OwnedPlaintext` no longer uses `Path::exists()` for lifecycle decisions.

The following operations now propagate metadata failures rather than interpreting them as absence:

- copy destination precondition;
- failed clone fallback;
- copy-and-hash destination precondition;
- transferred durable owner retirement;
- uncreated durable owner retirement;
- managed plaintext unlink/non-existence confirmation;
- prepared restore-transfer reconciliation.

### Durable row retirement is transactional

Both uncreated-owner release and restore predecessor retirement now use an immediate SQLite transaction:

```text
begin transaction
        ↓
prove predecessor path absent
        ↓
validate successor ownership where applicable
        ↓
delete durable owner row
        ↓
typed owner accepts retirement
        ↓
commit
```

If the path cannot be inspected conclusively, the transaction does not commit and the cleanup obligation survives for recovery.

### Regression coverage

A new Unix adversarial unit test, `metadata_failure_is_not_treated_as_plaintext_absence`, uses a symlink loop to force a deterministic metadata error. It proves that:

- the error is not converted into `false`;
- an uncreated durable owner cannot retire;
- the typed owner remains in `Reserved` state.

The independent 2b.6 validator additionally forbids production `.exists()` in the canonical plaintext authority and requires the transactional retirement sequence.

## 3. Defect found and fixed — encrypted-object retirement authority was still caller-influenced

### Problem

`commit_vault_manifest_root` accepted a caller-supplied list of superseded encrypted objects. Existing callers happened to populate the list correctly, but the primitive itself permitted a future stale/malformed caller to nominate an object for deletion.

That weakened the intended architecture:

> ordinary callers should not implement the object store's security semantics themselves.

The first repair removed the deletion list. A final re-audit then identified that callers still supplied the old manifest from which deletion was derived. That still left deletion provenance partly caller-controlled.

### Final repair

`commit_vault_manifest_root` now accepts only:

```text
transaction
note key
currently committed root
new manifest
```

The object store itself decrypts/authenticates the old manifest using `root.manifest_object_id` and derives superseded mutable objects from exactly three old→new comparisons:

```text
NotePrivateState
MetadataState
AttachmentIndex
```

The caller supplies neither deletion candidates nor the manifest used as deletion provenance.

`RevisionObject` identities are outside this normal mutable-object retirement authority. Revision deletion remains an explicit revision-pruning/garbage-collection concern rather than something an autosave/root-commit caller can request accidentally.

The old manifest object is retired only after the new root is installed inside the same SQLite transaction. Crash semantics remain old-complete-graph or new-complete-graph.

### Performance effect

The stronger boundary adds only one authenticated read/decrypt of the small current manifest during a protected graph commit:

```text
O(manifest)
```

It does not traverse revisions, attachments, or history and therefore preserves I7. The change deliberately favors a small constant operation at the security authority boundary over trusting caller-supplied deletion provenance.

## 4. Independent I1–I7 reconstruction

| Invariant | 2b.6 conclusion |
|---|---|
| I1 — ownership precedes plaintext | Holds. Managed attachment/staging/session materialization routes through `OwnedPlaintext`; durable reservations precede materialization. |
| I2 — ownership survives until plaintext is gone | **Strengthened in 2b.6.** Absence is now `NotFound`-only; metadata errors fail closed; durable retirement is transactionally coupled to the absence proof. |
| I3 — one privacy transition authority | Holds. Feature/storage code does not assemble low-level Vault revocation + derivative transition + storage lock ordering; `PrivacyCoordinator` owns it. |
| I4 — conflicting transitions fail closed | Holds. Same-note and library-vs-note admission is coordinator-owned; Lock/Protect revocation/drain occurs after admission and before mutation authority. |
| I5 — protected publication requires current authorization | Holds. Actual decrypted UI handoffs acquire a `PrivacyPublicationLease`; Restore discards pre-transition authority and captures a fresh post-Restore token. |
| I6 — sensitive persistence deny-by-default | Holds. `SensitiveNoteData` and `ProtectedPublicPersistenceSpec` remain the classification/scrub authority; protected persistence uses the narrow public stub. |
| I7 — protected mutation cost scales with what changed | Holds. Autosave does not hydrate revision history; revision append is head-based; metadata/index mutation is separated; object retirement remains constant-size. |

No architectural contradiction requiring amendment of the governing 2b contract was found.

## 5. Mandatory six closure sweeps

### 5.1 Direct SQLite writer sweep

No sensitive SQLite writer was found outside the storage layer.

Granular Vault table mutation remains concentrated in the Vault object/protection implementation:

```text
vault_objects
vault_note_roots
vault_blob_refs
```

Ordinary plaintext tables remain legitimate for ordinary notes, import, pruning and explicit Unprotect reconstruction. Protected steady-state revision creation/restore does not repopulate ordinary `note_revisions`.

### 5.2 Direct sensitive filesystem writer sweep

Remaining raw filesystem mutation was classified as one of:

- implementation inside the typed plaintext/native privacy authority;
- encrypted/ciphertext attachment or backup/archive data;
- Rustnote-owned backup repository state with its own ownership proof;
- explicit external user-selected export destination;
- directory creation;
- tests.

Managed attachment ingestion and protected materialization remain typed-owned. No second current managed-sensitive-plaintext lifecycle was found.

### 5.3 Privacy-transition caller sweep

No low-level Vault revocation or plaintext-derivative transition primitive escapes `src/security/`. Feature/storage callers request semantic transitions from `PrivacyCoordinator`.

Hard delete, protected restore, Protect/Lock/Unprotect, multi-note rollback and whole-library restore continue through coordinator admission.

### 5.4 Raw `PathBuf` plaintext-lifecycle sweep

The canonical production plaintext authority and durable staging lifecycle contain no `Path::exists()` retirement decision after 2b.6.

Typed ownership continues to cover:

- durable staging;
- durable Vault operations;
- durable attachment operations;
- durable attachment ingest;
- Vault-session materialization;
- process-private materialization.

### 5.5 UI publication sweep

Protected async publication was reconstructed independently across:

- note picker/search;
- backlinks;
- revision preview;
- metadata/properties;
- attachment commands;
- editor restore/install handoff;
- native-sensitive lifetimes.

Bare `is_publishable()` uses found in these flows are preflight/cancellation/status decisions or are followed by an actual leased handoff. Protected plaintext state publication itself remains lease-gated.

The protected revision restore flow correctly captures a fresh publication token after the Restore transition invalidates the pre-Restore token.

### 5.6 Old protected representation/dead-code sweep

Production source contains no active:

```text
ProtectedNotePayload
protected_notes
protected_note_blobs
```

There is no steady-state dual protected-note representation.

## 6. High-risk behavioral surfaces re-audited

The independent source audit reconstructed the following boundaries directly rather than accepting prior validator success as proof:

- Protect / Lock / Unlock / Unprotect / Delete / Restore overlap;
- library restore versus note-level transitions;
- reader and plaintext-producer drain;
- attachment ingestion/finalization/removal/materialization;
- import rollback and external export ownership;
- protected revision append/restore/pruning;
- manifest commit before/after crash boundaries;
- missing/corrupt encrypted object fail-closed behavior;
- backup/snapshot blocking while Vault operations are pending;
- protected FTS/SQLite/WAL residue scrub;
- emergency-journal denial for protected drafts;
- startup managed-plaintext and Vault-operation recovery order;
- protected→unprotected→protected cycle;
- hard deletion and cleanup ordering;
- protected async publication after privacy invalidation.

The new 2b.6 guard independently requires the corresponding source invariants and adversarial test corpus rather than chaining earlier Vault validators.

## 7. Final architecture result

The final security shape remains the intended 2b architecture:

```text
PrivacyCoordinator
    ├── transition admission / conflict authority
    ├── reader + publication revocation/drain
    └── protected mutation serialization

OwnedPlaintext
    └── managed plaintext ownership/lifecycle authority

SensitiveNoteData / ProtectedPublicPersistenceSpec
    └── default-private persistence classification

Vault object store
    ├── authenticated current root/manifest
    ├── NotePrivateState
    ├── MetadataState
    ├── AttachmentIndex
    └── immutable RevisionObjects

PrivacyPublicationToken
    └── decrypted UI publication lifetime
```

2b.6 did not add another lock, epoch, cleanup registry, protected representation or filesystem convention.

## 8. Final source delta from 2b.5.3

The substantive production Rust changes are confined to four files:

```text
src/security/plaintext.rs
src/storage/plaintext_staging.rs
src/storage/protection.rs
src/storage/vault_objects.rs
```

Other source-tree changes are:

- build ID `2b.6`;
- independent 2b.6 audit/QA documentation;
- one new independent 2b.6 validator;
- inherited validators taught to recognize `2b.6`, with the 2b.2/2a.16 ownership assertions strengthened so they no longer require the superseded insecure `Path::exists()` behavior.

No feature expansion or broad refactor was introduced.

## 9. Release boundary

All static/adversarial and dependency-free QA available in this environment is green; see `QA_VAULT_2B6_INDEPENDENT_CLOSURE.md`.

This environment cannot execute the native Rust gate. Final release qualification on macOS must still run formatting, compilation, Clippy and the complete Rust test suite. A native failure reopens 2b.6.

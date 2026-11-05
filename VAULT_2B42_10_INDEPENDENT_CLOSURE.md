# Rustnote Vault 2b.42.10 — Independent Closure Audit

## Status

**Independent source/security audit: PASS.**

No substantive I1–I7 or E0–E3 security `PARTIAL`/`FAIL` remains in the audited source after the repairs below.

**Native execution gate: PENDING on macOS.** This environment has no Rust toolchain, so `cargo fmt/check/clippy/test` and the dependency-backed editor behavior suite remain mandatory before 2b.42.10 is declared fully native-verified. Do not treat source/static PASS as a substitute for that gate.

This pass did not start 2b.42.11.

## Method

The audit was reconstructed from the governing invariants and the original E0–E3 contracts. Existing validators were run only after the independent source audit and repairs; they were regression evidence, not proof.

The direct sweeps covered:

- plaintext creation and destruction;
- canonical plaintext ownership and transfer;
- sensitive filesystem writers and path observations;
- direct SQLite writers;
- raw `PathBuf` plaintext lifetimes;
- Vault transition callers and admission ordering;
- publication paths, async completions and UI caches;
- backups and backup ownership;
- attachments and attachment metadata;
- revisions;
- FTS / SQLite / WAL residue;
- staging, native handoffs and process-private temporary ownership;
- Restore;
- shutdown;
- obsolete/alternate protected persistence.

## Substantive defect repaired — ambiguous filesystem absence

The WIP audit correctly identified a real I2/I4/E3 defect class: security-sensitive ownership code used boolean existence checks where `false` can mean either `NotFound` or a metadata-observation failure.

The canonical rule is now explicit in `src/security/plaintext.rs`:

```text
symlink_metadata succeeds
→ Present

NotFound
→ Absent
→ ownership may treat the path as gone

any other metadata error
→ observation failed
→ preserve ownership
→ fail closed
```

`managed_path_is_present(&Path) -> io::Result<bool>` is the shared narrow primitive. The existing `OwnedPlaintext` lifecycle delegates to the same rule.

### Repaired ownership/lifecycle boundaries

The strict observation rule is now used where ambiguous absence could retire, replace, reclaim, or lose sensitive ownership, including:

- Protect rollback and retired plaintext restoration;
- Protect/Unprotect attachment blob staging/finalization decisions;
- Vault materialized protected-attachment registry lookup/promotion;
- durable attachment-ingest cleanup;
- durable attachment-operation staging/materialization/cleanup;
- native strict staging cleanup;
- native handoff replacement;
- Restore/database-snapshot destination and attachment retirement handling;
- revision attachment restoration checks;
- portable-bundle canonical blob placement;
- non-Unix emergency-journal durable replacement;
- process-private temporary cleanup-claim and owner-marker arbitration.

### Additional defect found during the de-novo sweep

`native/temp_owner.rs` treated owner-marker metadata failure as an invalid/missing owner via a boolean helper. That could make an ambiguous owner look reclaimable. It is now tri-state:

- valid regular owner marker → evaluate lock ownership;
- `NotFound` → no marker;
- all other errors → potentially live / not cleanup-authorizing.

The cleanup-claim check after acquiring a temporary-session owner lock was also moved to the strict path-observation primitive.

### Additional cleanup defect found by the repeated de-novo certification audit

A workspace-wide single-occurrence function sweep found one obsolete public façade,
`Storage::add_derived_image_attachment_file`, with no Rust caller anywhere in the workspace.
The live image-edit path already uses `add_derived_image_attachment_file_authorized`, so the
unused unauthorised façade was removed rather than retained as speculative public API. The
2b.42.10 source validator and browser contract now assert that it remains absent.

This was a CLEER/dead-code defect, not a security vulnerability. No behavior or authority
ordering changed.

### Intentionally retained ordinary `.exists()` uses

The repository-wide classification did **not** mechanically replace every boolean existence check. Remaining production uses are outside the sensitive ownership-retirement rule, including:

- backup-repository quarantine/pruning conveniences while an external durable namespace owner remains live;
- export filename/directory collision allocation;
- portability export asset-destination collision checks;
- emergency-journal quarantine filename allocation;
- application UI destination-name suggestion.

Those checks do not retire or transfer a sensitive plaintext owner. The Vault backup privacy-reset path itself uses the external `DurablePlaintextNamespace` and strict recursive removal, so an ambiguous child observation cannot retire backup cleanup authority.

## Adversarial regression coverage added

The repaired defect class now has direct regressions proving:

- true `NotFound` is accepted as conclusive absence;
- metadata failure is not treated as plaintext absence;
- ambiguous Vault materialization does not replace the registered canonical path;
- rejected duplicate materialization remains tracked and is cleaned on retry;
- ambiguous attachment-ingest cleanup keeps its durable row;
- ambiguous attachment-operation cleanup keeps its durable row;
- Protect rollback refuses ambiguous absence and succeeds after repair/retry;
- native strict cleanup rejects ambiguous absence and accepts true `NotFound`;
- ambiguous process owner-marker metadata never authorizes cleanup.

Unix `ELOOP` fixtures are used where a deterministic ambiguous metadata failure is required.

## Final I1–I7 matrix

| Invariant | Status | Independent conclusion |
|---|---|---|
| I1 — ownership precedes plaintext | **PASS** | `OwnedPlaintext`, durable staging rows, Vault operation ownership and process-private owners remain reservation/ownership-first; no post-hoc ownership escape hatch was found. |
| I2 — ownership cannot disappear before plaintext disappears | **PASS** | Repaired defect class now uses `NotFound`-only absence proof; durable rows/registry/process owners survive ambiguous observations and cleanup failures. |
| I3 — privacy transitions have exactly one authority | **PASS** | `PrivacyCoordinator` remains the semantic transition authority; low-level Vault revocation primitives do not escape the security layer. |
| I4 — conflicting transitions fail closed | **PASS** | Note/multi-note/library admission remains mutually coordinated; Restore is exclusive; ambiguous filesystem ownership observations now also fail closed. |
| I5 — plaintext publication requires current authorization | **PASS** | Scoped publication authorization + UI generations + short publication leases remain required before publication; transition drain invalidates stale work. |
| I6 — sensitive persistence is deny-by-default | **PASS** | `SensitiveNoteData` / `ProtectedPublicPersistenceSpec` remain the exhaustive classification and scrub authority; direct SQLite authority remains confined to storage. |
| I7 — protected mutation cost scales with what changed | **PASS** | Granular Vault objects remain separated by private state/metadata/attachment index/revision; normal protected save does not hydrate full revision history. |

## Final E0–E3 matrix

| Contract | Status | Independent conclusion |
|---|---|---|
| E0 | **PASS** | Vault cryptographic construction remains isolated and unchanged by this pass; source retains AEGIS-256 with the full 32-byte tag, random scope keys and zeroizing key/plaintext buffers. |
| E1 | **PASS** | Password wrapping remains Argon2id-derived wrapping of a random Vault key; domain/context binding and authenticated object/blob formats remain inside `rustnote-vault`. |
| E2 | **PASS** | Granular encrypted note/object persistence and independently encrypted attachment blobs remain canonical; obsolete monolithic protected persistence is absent. |
| E3 | **PASS** | The discovered ownership/residue gap is repaired; protected plaintext lifetime remains covered across SQLite/FTS/WAL, revisions, attachments, staging, handoffs, caches and managed backups. |

## Required audit-surface matrix

| Surface | Status | Result |
|---|---|---|
| Plaintext creation | **PASS** | Managed sensitive materialization is preceded by typed or durable ownership. |
| Plaintext ownership | **PASS** | Canonical owner state machines retain authority until conclusive absence/transfer. |
| Filesystem writers | **PASS** | Sensitive lifecycle writers use typed ownership/strict cleanup; remaining boolean collision checks do not retire ownership. |
| SQLite writers | **PASS** | Direct `rusqlite` authority remains confined to `src/storage`; protected persistence routes through canonical storage boundaries. |
| Raw `PathBuf` lifetimes | **PASS** | Sensitive paths are paired with a live Vault/derivative permit or durable/process owner for their usable lifetime. |
| Vault transitions | **PASS** | One coordinator authority; admission precedes revocation/drain/storage mutation. |
| Publication paths | **PASS** | Current scoped authorization and publication lease required. |
| Async completions | **PASS** | Stale generation/session completions are discarded rather than published. |
| UI caches | **PASS** | Protected persistent media compatibility caching remains disabled; protected ephemeral resources are permit-scoped. |
| Backups | **PASS** | Privacy reset uses durable external namespace ownership and strict recursive purge; snapshot creation is blocked by pending Vault operations. |
| Attachments | **PASS** | Blob bytes, names/metadata, ingest, operations and materializations remain authorization/ownership-scoped; ambiguous cleanup preserves durable authority. |
| Revisions | **PASS** | Protected revisions remain encrypted object state; restoration validates attachment presence fail-closed. |
| FTS | **PASS** | Protected public rows are scrubbed and FTS is rebuilt; no protected plaintext search surface is retained. |
| Staging | **PASS** | Durable and process-private staging has ownership before plaintext and strict retirement. |
| Restore | **PASS** | UI publication is revoked/disposed before exclusive library Restore; database/plaintext ownership is transferred/rolled back under surviving authority. |
| Shutdown | **PASS** | Note-local synchronization precedes the final whole-library lease/durability checkpoint, avoiding the prior lock-order hazard. |

## Direct contract sweeps

| Sweep | Status |
|---|---|
| Direct SQLite writers | **PASS** |
| Direct sensitive filesystem writers | **PASS** |
| Privacy-transition callers | **PASS** |
| Raw `PathBuf` plaintext lifecycles | **PASS** |
| UI publication | **PASS** |
| Dead/alternate protected persistence | **PASS** |

## Validator maintenance discovered during regression QA

Two inherited validators were stale rather than production code:

1. `validate-security-e2.sh` recognized the schema-v21 2b architecture only through build `2b.38`, so build `2b.41` incorrectly fell into the obsolete schema-v20 branch. Its internal build classification now recognizes 2b revisions structurally.
2. `validate-security-e3.sh` still required `INLINE_OWNER_FILE_NAME`, even though 2b.42.9 deliberately removed the obsolete inline backup-owner architecture and requires the external durable namespace owner. The contradictory marker requirement was removed.

After these validator repairs, cumulative E0–E3 static closure passes again without weakening the current architecture.

## Closure boundary

2b.42.10 has **no known substantive security PARTIAL/FAIL at source/architecture level**. The only outstanding item is native execution verification on macOS. Until that native gate passes, this source is a **closure candidate**, not a final native-certified release.

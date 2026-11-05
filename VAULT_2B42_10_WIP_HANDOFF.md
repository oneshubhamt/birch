# Rustnote Vault 2b.42.10 — Independent Closure Audit WIP Handoff

## Status

**WIP / NOT CLOSED.**

This package is based on the latest verified **2b.42.9 cleanup** source. No 2b.42.10 production repair has been committed in this handoff package yet. The work completed so far is the de-novo audit and concrete defect discovery described below.

Do not treat existing 2b/E0–E3 validators as proof. Pass 2b.42.10 must reconstruct the security model directly from the governing contracts and inspect the implementation independently.

## Primary governing contracts

Audit against both axes:

1. **2b governing contract / I1–I7**
   - I1 — ownership precedes plaintext
   - I2 — ownership cannot disappear before plaintext disappears
   - I3 — privacy transitions have exactly one authority
   - I4 — conflicting transitions fail closed
   - I5 — plaintext publication requires current authorization
   - I6 — sensitive persistence is deny-by-default
   - I7 — protected mutation cost scales with what changed

2. **Original E0–E3 encryption/security contract**
   - E0/E1: crypto choice, authenticated formats, key derivation/wrapping, crypto isolation
   - E2: protected note behavior/persistence and attachment model
   - E3: plaintext-equivalent residue/lifetime closure across notes, attachments, revisions, journals, FTS, backups, caches, previews, staging, exports, etc.

The user explicitly requested that 2b.42.10 be checked against **both I1–I7 and E0–E3**.

## Required audit surfaces

Inspect de novo:

- plaintext creation
- plaintext ownership
- filesystem writers
- SQLite writers
- raw `PathBuf` lifetimes
- Vault transitions
- publication paths
- async completions
- UI caches
- backups
- attachments
- revisions
- FTS
- staging
- Restore
- shutdown

Also perform the contract-mandated direct sweeps:

- direct SQLite writer sweep
- direct sensitive filesystem writer sweep
- privacy-transition caller sweep
- raw `PathBuf` plaintext-lifecycle sweep
- UI publication sweep
- dead/alternate protected-persistence sweep

## Defect class already discovered

### Root issue: ambiguous filesystem absence can violate I2 / I4 / E3

A number of sensitive cleanup/ownership paths use `Path::exists()` (or an equivalent boolean existence test) before retiring, replacing, or recovering authoritative plaintext ownership.

Rust `Path::exists()` returns `false` not only for `NotFound`, but also when metadata cannot be obtained for other reasons. For security-sensitive managed plaintext, this is insufficient.

The required rule is:

```text
NotFound
→ conclusively absent
→ ownership may be retired / replacement may proceed

metadata says file exists
→ ownership remains

PermissionDenied / ELOOP / I/O / other metadata error
→ absence is NOT proven
→ ownership remains
→ fail closed
```

A durable cleanup authority must never disappear merely because filesystem observation was ambiguous.

### Confirmed high-priority locations to inspect/fix

#### 1. Protect rollback / retired plaintext restoration

`src/storage/protection.rs`

`restore_retired_plaintext_blobs(...)` currently contains:

```rust
if !blob.plaintext.path().exists() {
    continue;
}
```

If metadata fails, this can incorrectly skip restoration while the retired plaintext may still exist. The Vault-operation owner can later be retired, producing an I2/E3 ownership gap.

Also inspect the surrounding Protect rollback/recovery code such as `restore_retiring_operation` and `stage_exclusive_plaintext_blobs` for the same fail-open existence pattern.

#### 2. Vault materialized protected-attachment registry

`src/security/runtime.rs`

Both `VaultPlaintextPath::promote_materialized_attachment(...)` and `VaultRuntime::materialized_attachment(...)` filter registry entries with `path.exists()`.

If an existing decrypted materialization produces a metadata error, the code can treat it as absent and publish/replace another path, potentially losing authoritative cleanup tracking for the old decrypted plaintext.

This is a direct I2/E3 concern and must fail closed without creating duplicate/unowned materializations.

#### 3. Durable attachment ingest cleanup

`src/storage/attachment_ingest.rs`

`discard_attachment_ingest_job(...)` currently checks:

```rust
if path.exists() {
    ... owned.remove_file()?;
}
self.delete_attachment_ingest_job(&job.id)
```

An ambiguous metadata error may bypass `remove_file()` and then delete the durable ingest row. That can retire cleanup ownership before plaintext absence is proven.

#### 4. Durable attachment-operation cleanup

`src/storage/attachments.rs`

Inspect at minimum:

- `discard_staged_attachment(...)`
- `discard_unreferenced_attachment_operation_blob(...)`
- `materialize_attachment_blob(...)`

Several paths use `exists()` before cleanup/rename decisions while the attachment-operation row is the durable authority. They need the same conclusive-absence semantics.

#### 5. Process/native staging strict cleanup

`src/native/staging.rs`

`remove_plaintext_file_strict(...)` currently does:

```rust
let removed = privacy_remove_file(path)?;
if !removed && path.exists() { ... }
```

Audit whether `privacy_remove_file` plus the subsequent `exists()` can turn an observation error into apparent absence. The strict cleanup helper should be a canonical fail-closed boundary, not a boolean existence shortcut.

### Additional locations to inspect before declaring the defect class closed

A repository-wide `Path::exists()` sweep found other production occurrences in:

- `src/storage/backup_repository.rs`
- `src/storage/backup_support.rs`
- `src/storage/backups.rs`
- `src/storage/history.rs`
- `src/storage/portable_bundle.rs`
- `src/storage/portability.rs`
- `src/autosave/emergency_journal.rs`
- `src/native/handoff.rs`
- `src/native/temp_owner.rs`
- `src/application/portability.rs`

Do **not** mechanically replace every `exists()` call. Classify each occurrence:

- sensitive managed plaintext lifecycle / ownership decision → must be conclusive/fail-closed;
- ordinary ciphertext/static/non-sensitive path or test assertion → may be harmless;
- allocation/collision convenience → review for TOCTOU but not automatically an I2 issue.

## Preferred repair architecture

Avoid scattered ad-hoc `symlink_metadata` checks.

Introduce or reuse one small internal helper at the canonical sensitive-filesystem/ownership boundary with semantics equivalent to:

```rust
enum PathPresence {
    Present,
    Absent,
}

fn managed_path_presence(path: &Path) -> io::Result<PathPresence> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(PathPresence::Present),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(PathPresence::Absent),
        Err(error) => Err(error),
    }
}
```

Exact naming/design may differ. Keep it small and local. Do not create a generic filesystem framework.

Where an existing `OwnedPlaintext` method can own the entire observation/delete/retirement decision, prefer moving the rule there so callers cannot reproduce it incorrectly.

## Required adversarial regression coverage for this defect class

Use deterministic ambiguous-filesystem states where possible. A symlink loop is a useful Unix/macOS fixture because metadata resolution can fail with `ELOOP` while the path is not conclusively absent.

Tests should prove:

1. ambiguous metadata does **not** retire durable ownership;
2. ambiguous metadata does **not** replace an existing Vault materialized attachment registry entry;
3. ambiguous metadata causes the operation to fail closed;
4. retry after the filesystem problem is repaired still succeeds;
5. true `NotFound` remains accepted as already absent;
6. no new unowned plaintext is created during error recovery.

Keep tests portable where practical; Unix-specific adversarial fixtures may be `#[cfg(unix)]` if necessary, with generic unit coverage for the classification helper.

## Preliminary audit results before repair

These are **preliminary**, not final certification:

| Invariant | Preliminary status | Notes |
|---|---|---|
| I1 | PASS so far | Reservation-first `OwnedPlaintext` model remains structurally present. Continue full creator sweep. |
| I2 | **FAIL / substantive defect found** | Ambiguous filesystem absence can allow ownership retirement/replacement before absence is proven. |
| I3 | PASS so far | `PrivacyCoordinator` remains the canonical transition authority. Complete caller sweep. |
| I4 | PARTIAL | Transition state is fail-closed, but filesystem ownership errors above are not consistently fail-closed. |
| I5 | PASS so far | Scoped publication-token architecture appears coherent. Complete async/UI sweep. |
| I6 | PASS so far | `SensitiveNoteData` remains exhaustive/default-private. Complete SQLite writer sweep. |
| I7 | PASS so far | Granular object mutation architecture remains intact. Performance was measured in 2b.42.7. |

| E-contract | Preliminary status | Notes |
|---|---|---|
| E0 | PASS so far | No crypto primitive defect discovered in the preliminary independent crypto sweep. |
| E1 | PASS so far | Crypto remains isolated in `rustnote-vault`; authenticated/context-bound formats and key wrapping appear intact. |
| E2 | PASS so far | Granular protected persistence and independent attachment blobs remain intact. |
| E3 | **FAIL / substantive defect found** | Ownership/lifetime ambiguity can leave managed plaintext without a surviving authoritative cleanup obligation. |

Do not turn these preliminary rows into the final 2b.42.10 scorecard until the full audit is complete and repairs are re-audited.

## 2b.42 history / frozen baseline

Completed before this pass:

- 2b.42.1 scoped publication authority
- 2b.42.2 source/target navigation authorization
- 2b.42.3 note-local UI authority
- 2b.42.4 multi-note UI authority
- 2b.42.5 note-scoped Vault action controller
- 2b.42.6 library-wide operation authority
- 2b.42.7 release profiling
- 2b.42.8 no-VACUUM forensic experiment and production removal after native PASS
- 2b.42.9 dead-code/architecture cleanup and removal of obsolete development compatibility

2b.42.7 key performance finding:

- small Protect was dominated by backup repository reset (~350–400 ms warm), not VACUUM;
- attachment-heavy Protect/Unprotect dominated by attachment encrypt/decrypt/finalize;
- VACUUM was only ~15–20 ms and was removed only after the native forensic no-VACUUM test passed;
- later architectural work is planned to make all Rustnote-managed automatic backups encrypted at rest, but **do not implement that inside 2b.42.10**.

## 2b.42.10 completion rule

After repairs:

1. re-run the de-novo audit from the contracts, not from validators;
2. produce final I1–I7 PASS/PARTIAL/FAIL table;
3. produce final E0–E3 PASS/PARTIAL/FAIL table;
4. produce per-surface PASS/PARTIAL/FAIL table for all requested audit surfaces;
5. document every substantive defect found and exact repair;
6. add targeted adversarial tests;
7. run applicable inherited regression QA only **after** independent reasoning is complete;
8. package a latest 2b.42.10 candidate;
9. require Mac native `fmt → prepare → check → clippy -D warnings → full tests → browser tests` before final closure.

If a new defect is found, fix it in 2b.42.10 and re-audit the affected invariant/surface. Do not move to 2b.42.11 with any PARTIAL/FAIL that represents a substantive security hole.

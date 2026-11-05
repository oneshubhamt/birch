# QA — Rustnote 0.12.5 build 2a.17 persistent residue and backup closure

## Scope

This QA record covers only pass 2a.17. The pass addresses persistent Rustnote-managed plaintext residue after local protection: remembered/offline automatic-backup repositories, attachment `.orphaned` quarantine, and regression preservation of the 2a.16 portability/import staging ownership model. It does not claim final E0–E3 closure; UI/async plaintext lifetime and protected revision restore remain planned for 2a.18, followed by the independent 2a.19 closure audit.

## Implemented invariants checked

### Durable backup privacy obligations

- Backup-location history is version 3 and records `pending_privacy_reset_directories` per library.
- Vault cleanup publishes privacy-reset obligations for all remembered backup directories before attempting repository invalidation.
- An unavailable remembered repository remains pending while local protection can complete.
- A missing pending mount path is not recreated by Vault cleanup, library open, or Settings location persistence.
- An empty substitute directory at the same path is not accepted as proof that the remembered Rustnote repository returned.
- Pending cleanup is cleared only after positive repository ownership proof, directory-lock acquisition, repository purge, and publication of minimal clean-state owner/index metadata.
- A currently configured removable repository that disappears after `Storage` opened remains privacy-pending; the live instance blocks repository reconciliation, snapshot creation, pruning, inspection and restore until reconnect + reopen.
- Backup status may expose the unresolved count without touching/recreating a pending current repository.

### Attachment orphan quarantine

- New protection derives protected attachment plaintext SHA-256 identities and strictly removes matching `attachments/.orphaned` artifacts by deterministic hash suffix.
- Unrelated quarantine entries are preserved.
- Matching unexpected filesystem objects or failed deletion are hard failures; protection cannot report residue cleanup complete.
- E3 privacy marker v6 performs a one-time conservative Rustnote-owned orphan-quarantine scrub for libraries protected before the targeted hash mapping existed.

### Portability/import staging regression preservation

- 2a.16 `managed_plaintext_staging` remains the sole ownership authority for staged `.portability-import-*.rustnote` archives and newly extracted bundle blobs.
- No duplicate 2a.17 import cleanup subsystem was introduced.
- Ownership remains before first plaintext write and transfer to canonical references remains transactional.

## Executed validation in this environment

The following cumulative static security command completed successfully:

```text
scripts/validate-security-e0-e3-closure.sh
```

Observed result:

```text
Security E0 static contract passed (build 2a.17; Cargo checks skipped by RUSTNOTE_E0_STATIC_ONLY=1).
Security E1 static contract passed (build 2a.17; Cargo checks skipped by RUSTNOTE_E1_STATIC_ONLY=1).
Security E2 static contract passed (build 2a.17; Rust compilation skipped by RUSTNOTE_E2_STATIC_ONLY=1).
Security E3 static contract passed (build 2a.17; Rust compilation skipped).
E0-E3 2a.16 canonical plaintext ownership guard passed.
E0-E3 2a.17 persistent residue and backup closure guard passed.
E0-E3 2a.17 cumulative targeted remediation regression guard passed.
```

The 2a.17 guard checks source ordering/invariants and verifies that targeted Rust regression tests are present for offline repositories, substitute paths, current-repository disappearance, reconnect cleanup, pending status, targeted orphan cleanup, E3-v6 migration, and inherited 2a.16 portability ownership.

## Source/package hygiene checks

Before packaging, the source tree is checked for:

- shell-script parse validity under the available shell;
- diff whitespace errors against the 2a.16 baseline;
- accidental build output, cache directories, editor/OS metadata, or nested archives;
- expected build id and required 2a.17 security/QA artifacts;
- package root naming and ZIP integrity after creation.

## Native Rust validation limitation

This execution environment does not provide `cargo`, `rustc`, or `rustfmt`. Therefore the following native gates are **not represented as executed here**:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

They remain required on the normal Rustnote development machine before treating 2a.17 as native compile/test certified. Static validation is not a substitute for those native gates.

## Result

Within the 2a.17 scope, the source-level implementation and cumulative static security contracts satisfy the persistent-residue and backup-closure objectives. Final E0–E3 certification is intentionally deferred until 2a.18 and the independent 2a.19 adversarial closure audit are complete.

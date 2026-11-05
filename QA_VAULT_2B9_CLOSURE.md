# Rustnote Vault 2b.9 — Backup Ownership Closure QA

## Objective result

The pass closes the backup-repository I2 defect identified by the independent 2b.8 audit at source/architecture level.

### Original failure reconstructed

2b.8 could execute:

```text
privacy-reset obligation durable
→ recursively delete Rustnote Repository
→ delete in-tree .rustnote-repository-owner
→ crash while snapshot/blob plaintext remains
→ restart sees obligation but no ownership proof
→ remaining plaintext cannot be safely deleted
```

### 2b.9 result

The authoritative marker is external to the recursively deleted child. Repository deletion cannot delete its own authority. Cleanup proves root absence before the privacy obligation is retired. Steady-state writes and strict cleanup revalidate the marker and never silently recreate a missing owner.

## Independent source re-audit

The de-novo audit is intentionally separate from the accumulated validators. It verifies:

- external marker is a sibling of `Rustnote Repository`;
- `BackupRepositoryOwner` steady-state type is absent;
- old root-level/version-1 repository purge engine is absent;
- current repository writers use verified namespace authority;
- the only raw layout acquisitions are inside namespace establishment/verification helpers;
- Vault purge uses `DurablePlaintextNamespace::remove_tree_strict()`;
- repository absence is explicitly proven after purge;
- privacy-reset obligation is published before deletion begins;
- obligation is cleared only after clean repository recreation/publication;
- immediate 2b.8 inline ownership is one-way adoption only;
- `DurablePlaintextNamespace::open_existing()` never recreates missing authority;
- strict namespace cleanup revalidates authority before destructive work;
- restart tests cover partial child deletion and root-absent-before-obligation-clear states.

Result: PASS at source/static level.

## Runnable regression QA

The following gates were run in their documented dependency-free/static modes because the sandbox has no Rust toolchain:

- Architecture A1–A7: PASS
- Architecture B1–B6: PASS
- Dioxus 0.7.10 static baseline: PASS
- Security E0–E3 static contracts: PASS
- E0–E3 ownership 2a.16: PASS
- E0–E3 residue 2a.17: PASS
- E0–E3 UI lifetime 2a.18: PASS
- E0–E3 adversarial closure 2a.19: PASS
- Vault 2b.1 foundation: PASS
- Vault 2b.2 privacy authority: PASS
- Vault 2b.3 publication token: PASS
- Vault 2b.4 granular cutover: PASS
- Vault 2b.5 consolidation: PASS
- Vault 2b.6 independent closure: PASS
- Vault 2b.7 closure repair: PASS
- Vault 2b.8 plaintext-authority closure: PASS
- Vault 2b.9 backup-ownership closure: PASS

The B-suite dependency-free browser behavior checks also passed when invoked by the static architecture chain.

## Native release gate not executable here

This environment does not contain `cargo`, `rustc`, or `rustfmt`. The following are therefore **NOT claimed as passed** and remain required on macOS against the exact packaged source:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

The embedded 2b.9 Rust adversarial tests will execute as part of that native test gate.

## Scope audit

Production-source changes are confined to:

- `src/security/plaintext.rs` — canonical durable-namespace verify-only reopen and strict cleanup revalidation;
- `src/storage/backup_repository.rs` — external backup namespace authority and purge/recovery consolidation;
- `src/storage/mod.rs` — startup ownership-order clarification;
- `src/storage/protection.rs` — remove obsolete root-level repository compatibility test and assert non-ownership instead.

No Vault object-format, editor, search, FTS, publication-token, or transition-coordinator redesign is part of 2b.9.

# Rustnote 0.12.5 build 2a.10 — E0–E3 behavioral final-closure QA

## Objectives

- [x] Fix protected attachment placeholder CHECK-constraint violation.
- [x] Rebuild FTS5 during E3 SQLite forensic cleanup before WAL truncation/VACUUM.
- [x] Advance `vault.e3_privacy_scrub` from v3 to v4.
- [x] Apply the v4 SQLite forensic scrub to existing 2a.9 libraries even when no protected note currently remains.
- [x] Fail closed for automatic/safety/rollback snapshots and complete archives while `vault_operations` is non-empty.
- [x] Reject imported snapshot/archive databases that themselves contain unfinished vault operations.
- [x] Add direct behavioral Rust regressions for attachment commit, DB/WAL FTS residue, v3→v4 migration, pending-vault snapshot refusal, and backup-database plaintext absence.
- [x] Preserve the E0/E1 cryptographic format and all 2a.6–2a.9 hardening.

## Static/source QA

The packaging pass must run E0, E1, E2, E3 and cumulative 2a.10 source validators, cumulative A1–A7/B1–B6 architecture guards, shell/JavaScript syntax checks, TOML parsing, SQLite schema-equivalence checks, changed-Rust structural scans, and final ZIP extraction/integrity comparison.

## Native acceptance

This sandbox does not contain the pinned Rust/Cargo toolchain. The following remain mandatory on the development Mac before executable-release acceptance:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
cargo audit
```

The new behavioral regressions are part of `cargo test --workspace --all-features --locked`; they must not be substituted by static source-marker validation.

## Sandbox closure results

Completed against the finalized 2a.10 source before packaging:

- E0/E1/E2/E3 static security contracts: **PASS**.
- Cumulative 2a.10 E0–E3 source closure validator: **PASS**.
- A1–A7 / B1–B6 cumulative static architecture chain: **PASS**.
- Dependency-free editor/PDF behavioral suite reached by B6: **36/36 PASS**; B6 PDF subset: **16/16 PASS**.
- Independent SQLite execution of the actual schema-17 attachment CHECK with the protected placeholder (`embedded = 0`, non-null `unreferenced_at`): **PASS**.
- Independent SQLite FTS5 forensic fixture using the exact 2a.10 cleanup sequence (`rebuild` → WAL truncate → `VACUUM` → WAL truncate): unique deleted marker absent afterward: **PASS**.
- Persistent-snapshot call-graph audit: all Rustnote-managed database snapshot creation flows through `create_database_snapshot`; archive/snapshot validation independently rejects unfinished `vault_operations`: **PASS**.
- Shell syntax: **29/29 PASS**.
- First-party JS/MJS syntax: **39/39 PASS**.
- TOML parsing: **6/6 PASS**; `Cargo.lock` TOML parse: **PASS**.
- Rust lexical delimiter/structure scan: **132/132 PASS**.
- Rust trailing-whitespace scan: **PASS**.
- Schema definition/migrations, `rustnote-vault`, editor source/runtime, PDF source/runtime, root `Cargo.toml`, `Cargo.lock`, toolchain and Dioxus configuration are byte-for-byte unchanged from 2a.9 unless explicitly listed in the 2a.10 diff; schema remains version 17.

The Rust regression tests added by this pass are executable source tests but were **not executed in this sandbox** because Rust/Cargo is unavailable. Native acceptance below remains mandatory before executable-release acceptance.


## Final package verification

After this QA record was frozen, the release archive was rebuilt and verified from a fresh extraction. Acceptance criteria are: ZIP integrity **PASS**; exact source/archive membership **283/283**; E0–E3 2a.10 closure validator **PASS** on the extracted copy; and cumulative B6 static architecture chain **PASS** on the extracted copy.

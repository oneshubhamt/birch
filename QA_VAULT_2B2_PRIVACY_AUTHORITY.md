# Rustnote Vault 2b.2 — QA and closure record

Build: `0.12.5 / 2b.2`
Baseline: `0.12.5 / 2b.1`

## Pass verdict

**Source/static/dependency-free closure: PASS.**

**Native Rust release gate: NOT EXECUTED in this sandbox** because `cargo`, `rustc` and `rustfmt` are unavailable.

The 2b.2 source therefore qualifies as the completed implementation artifact for this environment, with native Mac compilation/tests explicitly required before treating it as a release-ready binary build.

## Dedicated 2b.2 validation

`bash scripts/validate-vault-2b2-privacy-authority.sh` — **PASS**.

The validator checks:

- build 2b.2 identity and required authority modules;
- one coordinator-owned transition architecture;
- note/multi-note/library admission guards;
- canonical privacy-transition ordering;
- Unlock participation in the same admission authority;
- all-target multi-note derivative freeze and Vault revocation before drain;
- migration of Protect/Lock/Unlock/Unprotect/Delete/purge/rollback/restore callers;
- absence of old manual privacy choreography outside `src/security`;
- `OwnedPlaintext` lifecycle/backends;
- owner-before-create staging order;
- migration of high-risk managed plaintext producers;
- narrow constructor and sensitive-rename authority;
- recoverable prepared-transfer ordering and startup reconciliation;
- typed cleanup of ordinary ledger-backed staging;
- exactly-once Vault materialized-plaintext promotion;
- fail-closed clone/copy fallback;
- adversarial coverage markers;
- schema-19→20 migration wiring;
- executable SQLite structural equivalence of clean schema 20 vs migrated schema 19→20.

## Inherited security validation

The final tree passes:

- `validate-vault-2b1-foundation.sh`;
- E0–E3 **static** contracts through the cumulative closure runner;
- 2a.16 canonical plaintext ownership guard;
- 2a.17 persistent residue/backup guard;
- 2a.18 UI privacy lifetime/revision-restore guard;
- 2a.19 independent adversarial closure guard;
- cumulative E0–E3 closure guard.

The direct E0, E1, E2 and E3 scripts reach their native Cargo phase and then stop with `cargo: command not found`. Their static portions pass through `validate-security-e0-e3-closure.sh`, which invokes their supported static-only modes.

Inherited validators were updated only where 2b.2 intentionally supersedes implementation markers—for example build/schema values, direct revocation calls and prepared-transfer behavior. The security property is retained or strengthened: 2a.19 now rejects production reappearance of the old direct transition choreography.

## Dependency-free regression QA

Final results:

- shell syntax with `bash -n`: **35/35 scripts PASS**;
- first-party JS/MJS `node --check`: **44/44 files PASS**;
- canonical editor document specification: **PASS — 21 nodes / 14 marks**;
- editor document contract corpus: **PASS — 36 cases**;
- dependency-free browser/editor/PDF behavior suite: **PASS — 36/36 tests**.

## Differential architecture QA

The broad A/B architecture scripts were executed on both the untouched 2b.1 baseline and the final 2b.2 tree using the tools available here.

The outcome is identical between baseline and 2b.2:

- A0 cannot run its behavioral safety net without Cargo;
- chained A1–A5/A7/B2–B5 scripts depend on unavailable `zsh` and/or their native predecessor suite;
- A6, B1 and B6 reproduce the same pre-existing baseline failures;
- the A7 static closure portion itself passes before it reaches the unavailable A6 chained invocation.

No new differential architecture-validator failure is introduced by 2b.2.

## Final source audit

Compared with untouched 2b.1, the pass changes **22 Rust source files** and **12 validation scripts**, adds the 2b.2 implementation/QA records, and advances `build-id.txt` to `2b.2`.

Final sweeps found:

- no `TODO`, `FIXME`, `HACK`, `dbg!`, `todo!` or `unimplemented!` residue in changed Rust files;
- no production caller outside `src/security` directly invoking old Vault revocation or derivative-transition primitives;
- no newly added production raw managed-sensitive create/remove/rename bypass outside `OwnedPlaintext` itself;
- remaining relevant raw filesystem calls in changed production files are ciphertext handling, narrowly scoped pre-ledger legacy-residue recovery, ordinary/non-sensitive filesystem behavior, or code outside Rustnote-managed sensitive plaintext ownership.

## Adversarial cases added/retained for native execution

Rust source contains targeted tests for:

- same-note conflicting transition rejection;
- library-vs-note exclusion;
- overlapping multi-note all-or-nothing admission;
- multi-note producer freeze ordering;
- multi-note reader-revocation ordering;
- Lock waiting for an existing protected reader;
- Protect waiting for an existing plaintext producer and blocking a new one;
- failed process-private rename preserving predecessor ownership;
- failed Vault-operation rename preserving predecessor ownership;
- delete failure preserving ownership/retiring state;
- durable cleanup failure preserving retry ownership;
- durable filesystem rename failure retaining prepared predecessor/successor state;
- corrupt transfer records failing closed;
- restart before filesystem rename completing the prepared transfer;
- restart after filesystem rename finalizing the already-completed transfer;
- successor owner pinning while a prepared predecessor remains.

These tests are present but cannot be compiled/run in this sandbox without the Rust toolchain.

## Native Mac release gate

From the extracted 2b.2 source directory, run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
bash scripts/validate-vault-2b2-privacy-authority.sh
bash scripts/validate-vault-2b1-foundation.sh
bash scripts/validate-security-e0-e3-closure.sh
```

Then run the project's normal macOS preparation/release validation chain used for the application build.

## Closure conclusion

The 2b.2 architecture now has one privacy-transition authority and one typed managed-plaintext ownership concept at the caller boundary. The most important adversarial defect discovered during implementation—the crash-before-filesystem-rename prepared-transfer case—was corrected before packaging, and static validation now protects that recovery contract. No 2b.3 or 2b.4 architectural work is included in this pass.

# QA — Vault 2b.42.6 Library-wide operations

## Objective check

| Objective | Result |
| --- | --- |
| Separate source-editor synchronization from whole-library authorization | PASS |
| Dedicated non-publication whole-library privacy barrier | PASS |
| Executor scheduling precedes whole-library privacy capture | PASS |
| Automatic backup uses the two-stage boundary | PASS |
| Manual complete archive uses the two-stage boundary | PASS |
| Shutdown runs `checkpoint_all` before the read-side library lease, under one executor turn | PASS |
| Explicit manual durability path is not left ungated | PASS |
| Restore keeps the stronger exclusive library revocation model | PASS |
| Successful and failed Restore recover under fresh aggregate authority | PASS |
| Lock/Protect/Unlock/Unprotect and Restore cannot cross an active library operation | PASS |
| Exceptional protected-note Restore admission is covered | PASS |
| Delayed privacy authority forces source re-synchronization before snapshot | PASS |
| Settings Library portability uses executor -> source sync -> fresh whole-library authority | PASS |
| Generic portability API cannot export the whole library without typed authority | PASS |
| Notebook portability stabilizes an exact multi-note plaintext lifetime without blocking unrelated notes | PASS |
| No 2b.42.7+ profiling/VACUUM scope mixed into this pass | PASS |

## Dedicated 2b.42.6 checks

`./scripts/validate-vault-2b42-6-library-wide-operations.sh`

- **15/15 architectural contracts: PASS**

`node --test tests/js/library-wide-privacy-contract.test.mjs`

- **13/13 targeted behavior/source contracts: PASS**

The dedicated contracts cover executor-before-privacy ordering, delayed-authority source re-synchronization, backup/Lock ordering, archive source synchronization, shutdown `checkpoint_all` ordering, typed API gating, both Restore admission paths, failed-Restore UI recovery, Library portability authority, stabilized Notebook portability scope, and unrelated-note concurrency for the multi-note plaintext lifetime.

## 2b.42 regression chain

- 2b.42.1 scoped publication core: **PASS**
- 2b.42.2 navigation source/target: **PASS**
- 2b.42.3 note-local UI: **PASS**
- 2b.42.4 multi-note UI: **PASS**
- 2b.42.5 Vault-action controller: **PASS**
- 2b.42.6 library-wide operations: **PASS**

## Applicable inherited Vault/security guards

- 2b.1 foundation: **PASS**
- 2b.4 granular cutover: **PASS**
- 2b.5 consolidation: **PASS**
- 2b.6 independent I1–I7 closure: **PASS**
- 2b.8 plaintext authority: **PASS**
- 2b.9 backup ownership: **PASS**
- 2b.11 atomic privacy authority: **PASS**
- 2b.12 AEGIS/privacy-disclosure: **PASS**
- E0–E3 2a.16 canonical plaintext ownership: **PASS**
- E0–E3 2a.17 persistent residue / backup closure: **PASS**

The zsh-shebang validators were run through `bash` in this Linux environment; their bodies are bash-compatible and passed. They should run normally under their native zsh shebang on macOS.

## Browser/PDF regression suite

`node scripts/run-browser-behavior-tests.mjs`

- dependency-free behavior suite after post-audit repairs: **85/85 PASS**
- includes the new 2b.42.6 library-wide contract suite
- editor-build dependencies are absent in this environment, so dependency-required editor tests remain part of the later native Mac gate

## Source-quality checks

- no `TODO`, `FIXME`, `HACK`, `dbg!`, `todo!`, `unimplemented!`, debug-print, or broad dead-code suppression was added by 2b.42.6;
- whole-library application call sites were re-swept for alternate ungated backup/archive/durability paths;
- Restore call sites were re-swept for accidental read-side library leases;
- `Cargo.lock` remains unchanged from the 2b.42.5 baseline.

- shell syntax: **82/82 PASS**;
- JS/MJS syntax: **50/50 PASS**;
- changed Rust delimiter/lexical structural audit: **12/12 PASS**;
- source artifact sweep (`target`, `node_modules`, `.git`, `__pycache__`, `.pyc`, `.DS_Store`, logs/temp files): **clean**.

ZIP extraction integrity and SHA-256 are verified outside the source tree after packaging.

## Native Rust status

This environment has no Rust toolchain (`cargo`, `rustc`, and `rustfmt` are unavailable), so the following remain deliberately pending for the Mac/native closure gate:

```bash
cargo fmt --all
cargo fmt --all -- --check

./scripts/prepare-rustnote-mac.sh
./scripts/validate-prepared-runtime.sh

cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1

node scripts/run-browser-behavior-tests.mjs --require-editor-deps
```

No native compilation, strict Clippy, Rust-test or editor-dependency success is claimed by this checkpoint.

## Closure assessment

2b.42.6 meets its scoped architectural objective on the assistant-side after the post-audit repair: source synchronization and whole-library authorization are distinct; executor ownership precedes privacy capture; backup/archive/durability/Library portability are gated by the intended authority; Notebook portability uses an exact multi-note plaintext lifetime; shutdown does not hold a library read lease while performing note-local protected saves; Restore remains exclusive; and failed/delayed operations recover under fresh authority rather than caller-specific bypasses.

The next planned pass is **2b.42.7 — profiling only**.

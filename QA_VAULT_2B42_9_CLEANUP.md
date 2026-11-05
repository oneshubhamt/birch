# QA — Vault 2b.42.9 dead code + architecture cleanup

## Scope

2b.42.9 is a cleanup-only pass on the final 2b.42.8 no-VACUUM closure baseline. It removes dead-code suppression, obsolete development-build compatibility/adoption paths, duplicated helpers, stale build-number comments, and inherited validators that still encoded the superseded global publication model. It does not redesign encryption, publication authority, backup reset ordering, Restore, plaintext ownership, attachment ownership, or the 2b.42.8 residue sequence.

## Production cleanup completed

- removed broad `dead_code` suppression from the granular Vault object specification;
- retained strict editor-schema parsing while representing parse-only fields explicitly as ignored `_` fields;
- removed `DurablePlaintextNamespace::adopt_existing_with_proof` and the post-hoc nonempty-namespace adoption escape hatch;
- removed the superseded inline automatic-backup owner marker/adoption convention;
- retained one canonical backup ownership authority: the external durable namespace marker;
- removed backup-history v1/v2 acceptance and the empty-library-ID migration/fallback path;
- made current repository/history metadata strict rather than compatibility-defaulted;
- collapsed the one-use current-editor publication helper into `EditorSynchronizationContext`;
- removed stale build-number comments where they described superseded implementation history rather than current invariants;
- modernized inherited 2b.2/2b.3 and 2a.16/2a.18/2a.19 guards so they validate the current scoped architecture instead of obsolete global/adoption contracts.

Legitimate crash recovery, durable ownership, current-format identity validation, and fail-closed behavior are preserved.

## Dependency-free QA — PASS

Executed against the final canonical 2b.42.9 tree:

- dedicated 2b.42.9 JS cleanup contracts: **6/6 PASS**;
- dedicated 2b.42.9 source validator: **14/14 PASS**;
- complete 2b.42.1–2b.42.9 validator chain: **PASS**;
- dependency-free browser/editor/PDF behavior suite: **105/105 PASS**;
- inherited Vault validators 2b.1, 2b.4, 2b.5, 2b.6, 2b.8, 2b.9, 2b.10, 2b.11 and 2b.12: **PASS**;
- inherited 2b.7 closure-repair aggregate, including modernized 2b.2 and 2b.3 entry points: **PASS**;
- E0–E3 2a.16 canonical plaintext ownership: **PASS**;
- E0–E3 2a.17 persistent residue/backup closure: **PASS**;
- E0–E3 2a.18 scoped UI lifetime: **PASS**;
- E0–E3 2a.19 adversarial closure: **PASS**;
- shell syntax: **86/86 PASS**;
- repository JS/MJS syntax outside dependency/generated directories: **53/53 PASS**;
- artifact/dependency hygiene sweep: **PASS**;
- stale production cleanup-marker sweep: **PASS**.

`validate-architecture-a0.sh` was not executable to completion in the assistant environment because it requires the prepared generated editor runtime. The validator correctly instructed that `./scripts/prepare-rustnote-mac.sh` must run first. This is retained as part of the native/prepared-runtime gate rather than bypassed.

## Baseline diff audit

Compared byte-for-byte with `rustnote_v0.12.5_build_2b.42.8_no_vacuum_closure_source`:

- `Cargo.toml`: **unchanged**;
- `Cargo.lock`: **unchanged**;
- `build-id.txt`: **unchanged**;
- files added for 2b.42.9 documentation/QA: **4**;
- files removed: **0**;
- existing files changed: **23**, consisting of targeted production cleanup plus validator/test modernization.

No dependency change, lockfile drift, build-ID bump, production VACUUM restoration, runtime migration switch, or alternate publication/ownership authority was introduced.

## Native macOS gate — REQUIRED

Native Rust gate: pending on macOS.

The assistant environment has no Rust toolchain and no prepared editor runtime, so native compilation is not claimed. Run on macOS:

```text
cargo fmt --all
cargo fmt --all -- --check

./scripts/prepare-rustnote-mac.sh
./scripts/validate-prepared-runtime.sh

cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1

node scripts/run-browser-behavior-tests.mjs --require-editor-deps

./scripts/validate-architecture-a0.sh
./scripts/validate-vault-2b42-9-cleanup.sh
```

2b.42.10 must not begin until the native gate is green.

## Package integrity

The final 2b.42.9 archive is built only from the canonical latest tree used for the QA results above. After extraction, every regular file is compared byte-for-byte and by permission mode with the packaged source tree. The release archive must satisfy:

- regular files: **524/524 byte-identical**;
- permission modes: **524/524 identical**;
- executable files preserved: **91**;
- no `target`, `node_modules`, `__pycache__`, `.DS_Store`, log, swap, or temporary build contamination.

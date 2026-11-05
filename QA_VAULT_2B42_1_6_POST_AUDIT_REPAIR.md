# QA — Vault 2b.42.1–2b.42.6 post-audit repair closure

## Purpose

This record closes the two semantic gaps found by the de-novo audit performed after the original 2b.42.6 checkpoint. It does not replace the individual 2b.42.1–2b.42.6 architecture/QA records; it records the repair and the post-repair re-audit.

## De-novo pass scorecard

| Pass | Result | Post-repair evidence |
| --- | --- | --- |
| 2b.42.1 — scoped publication core | PASS | Library, multi-note and note-local generations/authorizations remain distinct; note and multi-note publication leases remain coordinator-owned; library revocation invalidates/drains all scopes; obsolete global publication capture APIs remain absent. |
| 2b.42.2 — navigation source/target | PASS | Source checkpoint and fresh target authorization remain separate; target publication flows through target-note authority; the old combined `EditorService::switch_note` workflow remains absent. |
| 2b.42.3 — note-local UI | PASS | Note-local editor/properties/sources/revisions/attachment UI remain keyed to note identity and note privacy generation; note-local publication scopes do not carry list generation. |
| 2b.42.4 — multi-note UI | PASS | Aggregate list/backlink/hierarchy/picker/link-target/source/tag state remains under multi-note authority and centralized aggregate recovery; unrelated note-local editor authority remains independent. |
| 2b.42.5 — Vault-action controller | PASS after repair | The unselected-B -> selected-B checkpoint race is closed by `synchronize_for_note_mutation`: a silent checkpoint that observes B become live-selected must loop through a live-editor checkpoint and return the exact no-await revocation-boundary selection snapshot. |
| 2b.42.6 — library-wide operations | PASS after repair | Automatic backup, complete archive, shutdown durability, explicit durability, Restore and Settings Library portability use the intended whole-library ordering. Notebook portability now uses a stabilized exact multi-note plaintext lifetime rather than an unrelated-note global barrier. |

## Repair 1 — Vault target becomes live-selected during checkpoint

The original 2b.42.5 controller resampled selection after an unselected target checkpoint but did not guarantee that a newly live target editor had itself crossed a durability checkpoint.

The repair makes the synchronization boundary authoritative:

```text
unselected B
-> silent canonical B checkpoint
-> B became selected?
   no  -> return exact boundary snapshot
   yes -> live-editor B checkpoint
          -> B still selected?
             yes -> return exact boundary snapshot
             no  -> re-evaluate target
-> no await
-> B UI revocation
```

`NoteMutationSynchronization { selection_generation, target_selected }` is consumed directly by the Vault controller. The controller no longer opens a second selection-sampling gap between durability and revocation.

## Repair 2 — Library portability whole-library authority

Settings Library export is now a first-class whole-library operation:

```text
choose destination
-> acquire ApplicationExecutionTurn
-> synchronize current source editor
-> fresh WholeLibraryOperationLease
-> export_library_scope(turn, &lease, ...)
```

The generic `PortabilityService::export_scope` rejects `PortabilityScope::Library`, preventing an alternate ungated application route.

## Repair 3 — Notebook portability scoped multi-note lifetime

Notebook export is multi-note, not whole-library. It therefore uses an exact scoped lifetime:

```text
optimistic notebook note-id set
-> atomic MultiNotePlaintextOperationPermit for that set
-> storage operation lock
-> re-read notebook note-id set
-> set changed? drop + retry
-> reject protected scope
-> export while permit + storage lock remain alive
```

The permit is built from atomic `PlaintextDerivativeGate::acquire_notes`. A privacy transition on an included note linearizes before or after the export; unrelated notes remain concurrent.

## Post-repair QA

- 2b.42.1 validator: PASS
- 2b.42.2 validator: PASS
- 2b.42.3 validator: PASS
- 2b.42.4 validator: PASS
- 2b.42.5 validator: PASS
- 2b.42.6 validator: PASS (15 contracts)
- 2b.42.5 targeted behavior: 11/11 PASS
- 2b.42.6 targeted behavior: 13/13 PASS
- inherited editor UI contract: 10/10 PASS
- complete dependency-free browser/PDF suite: 85/85 PASS
- 2b.1 / 2b.4 / 2b.5 / 2b.6 / 2b.8 / 2b.9 / 2b.11 / 2b.12 applicable guards: PASS
- E0–E3 2a.16 ownership guard: PASS
- E0–E3 2a.17 residue/backup guard: PASS
- shell syntax: 82/82 PASS
- JS/MJS syntax: 50/50 PASS
- changed Rust delimiter/lexical structural audit: 12/12 PASS
- `Cargo.lock`: byte-identical to the original 2b.42.6 checkpoint
- source artifact/debug/debt sweep: clean

## Native gate status

This environment has no Rust toolchain, so native formatting, compilation, strict Clippy and Rust tests are intentionally not claimed. They remain required on macOS before final 2b.42 promotion:

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

## Closure assessment

On the assistant-side evidence available here, 2b.42.1 through 2b.42.6 are architecturally closed after the post-audit repairs. No further publication/privacy redesign is indicated before 2b.42.7. Native Rust QA remains the explicit external gate rather than an assistant-side claim.

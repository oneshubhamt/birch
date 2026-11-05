# QA — Vault 2b.42.2 Navigation source/target separation

## Objective audit

| Objective | Result | Evidence |
|---|---|---|
| Source A authorized/checkpointed before target C | PASS (source/static) | `select_note` ordering + dedicated JS/static validator |
| Fresh target C authority before target load/publication | PASS (source/static) | target capture → load → freshness → lease → publish ordering |
| Lock B does not invalidate A/C note-local authority | PASS (composed invariant) | 2b.42.1 scoped coordinator tests + 2b.42.2 target path uses note-local tokens |
| Dirty A checkpoints before C | PASS (source/static); native execution pending | navigation ordering + existing application checkpoint test |
| No-current-note edge preserves pending flush | PASS (source/static) | no-source branch + `flush_without_selected_note` contract |
| Rapid A→B→C supersedes stale requests | PASS (source/static) | `note_selection_generation` advanced and rechecked before/after load |
| Failed source checkpoint prevents target load | PASS (source/static) | target loop occurs only after successful source synchronization result |
| Target stale before publication is discarded/retried | PASS (source/static) | post-load `target_token.is_publishable()` and lease checks |
| Section/notebook navigation uses fresh aggregate authority | PASS | two-stage source sync → multi-note lease helper |
| List reconciliation cannot publish full target under list token | PASS | fallback is `Option<NoteId>` and re-enters `select_note` |
| Old combined `EditorService::switch_note` architecture absent | PASS | validator + source search |

## Assistant-side QA observed

- `validate-vault-2b42-1-scoped-publication-core.sh`: PASS
- `validate-vault-2b42-2-navigation-source-target.sh`: PASS
- navigation JS contract: 8/8 PASS
- relevant dependency-free navigation/list/PDF tests: 35/35 PASS
- shell-script syntax: 78/78 PASS before documentation-only additions
- JS syntax: PASS
- Vault 2b.11 guard: PASS
- independent 2b.6 I1–I7 guard: PASS
- Vault 2b.12 guard: PASS
- E0–E3 2a.16 ownership guard: PASS
- E0–E3 2a.17 residue guard: PASS
- modified Rust delimiter/lexical structural sweep: PASS

## Known broader-test state

`scripts/run-browser-behavior-tests.mjs` still fails one dependency-free `editor-ui-contract.test.mjs` test because that historical test asserts the old global Vault-navigation publication API. The same failure is reproduced on the untouched 2b.42.1 input checkpoint, so it is not a 2b.42.2 regression. Updating the Vault-controller contract belongs to the later Vault-action pass rather than this navigation pass.

Editor-build/ProseMirror dependency-backed tests cannot run in this environment because the pinned `node_modules` tree is absent.

## Native gate still required

This environment has no Rust toolchain. Native closure therefore requires macOS execution of formatting, check, strict Clippy, and Rust tests. No native compile claim is made by this report.

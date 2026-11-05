# QA — Rustnote Vault 2b.42.11 Final QA + Package

## Environment capability

Available in this QA environment:

```text
bash
python3
Node.js 22.16.0
```

Unavailable:

```text
cargo
rustc
editor-build dependencies / node_modules
```

Accordingly, native Rust compilation and dependency-backed browser execution are explicitly not claimed here.

## Results

| Gate | Result |
|---|---|
| Source archive extraction | PASS |
| Shell syntax | PASS — 88 scripts after adding final validator |
| JS/MJS syntax | PASS — 54 files |
| 2b.1–2b.12 selected core inherited contracts | PASS |
| Modernized inherited 2b.13/2b.26/2b.31/2b.35–2b.41 objectives | PASS |
| 2b.42.1 | PASS |
| 2b.42.2 | PASS |
| 2b.42.3 | PASS |
| 2b.42.4 | PASS |
| 2b.42.5 | PASS |
| 2b.42.6 | PASS |
| 2b.42.7 profiling | PASS |
| 2b.42.7 native/Clippy source guard | PASS |
| 2b.42.8 | PASS |
| 2b.42.9 | PASS |
| 2b.42.10 independent closure | PASS — 16/16 contracts |
| E0 static | PASS |
| E1 static | PASS |
| E2 static | PASS |
| E3 static | PASS |
| 2a.16 ownership closure | PASS |
| 2a.17 residue closure | PASS |
| 2a.18 scoped UI lifetime | PASS |
| 2a.19 cumulative adversarial closure | PASS |
| Dependency-free browser/source behavior suite | PASS — 111/111 |
| Final pre-S1 source sweep | PASS |
| Production source diff from 2b.42.10 | PASS — none |
| Final ZIP extract/hash integrity | PASS — 533 files byte-identical |
| Native `cargo fmt/check/clippy/test` | PENDING MAC |
| Prepared runtime validation | PENDING MAC |
| Dependency-backed browser suite | PENDING MAC |
| Release profiling/manual adversarial UX | PENDING MAC |

## Historical validator remediation

Final QA found inherited validators that were stale after the scoped-publication and note-scoped-action redesign. They were repaired as QA infrastructure. Production code was not changed.

The repaired validators now assert the current behavior rather than the obsolete implementation:

```text
note-local + aggregate autosave publication
note-scoped Vault action busy state
no forced target selection
fresh target authorization
selected/unselected target checkpointing
current selection/view-generation separation
current Dioxus task-scope ownership
```

## Final static closure command

```bash
./scripts/validate-vault-2b42-final-pre-s1-closure.sh
```

Result:

```text
Vault 2b.42.11 final pre-S1 static closure passed.
```

## Browser/source behavior result

```text
tests: 111
pass: 111
fail: 0
```

The runner reported that editor-build dependencies were absent, so this is the dependency-free behavior suite only. The Mac gate must rerun with `--require-editor-deps`.

## Native closure requirement

2b.42.11 is not finally promoted until the Mac native gate is clean. A native failure is not deferred to S1; it must be repaired within 2b.42.11 and the affected QA rerun.

## Final package integrity

The final source archive is named:

```text
rustnote_v0.12.5_build_2b.42.11_final_pre_s1_source.zip
```

It was extracted into a fresh directory and every regular file was compared by relative path and SHA-256 digest against the packaged source tree.

```text
533 files compared
0 missing
0 extra
0 mismatched
PASS
```

# Rustnote 2a.19 — Independent adversarial closure QA

Build: **0.12.5 / 2a.19**

## Scope

This QA record covers the source/static and non-Rust executable validation available in the present environment. It deliberately does **not** claim native Rust compilation or tests, per project-owner instruction.

## Static/security validation

The final QA set includes:

- E0 static threat/format/KDF contract validator;
- E1 static cryptographic-isolation validator;
- E2 static protected-persistence validator;
- E3 static plaintext-residue/lifetime validator;
- 2a.16 canonical plaintext-ownership regression guard;
- 2a.17 persistent residue/backup-obligation regression guard;
- 2a.18 UI privacy-lifetime/protected-revision regression guard;
- new 2a.19 independent adversarial closure guard;
- cumulative E0–E3 closure script in static-only mode;
- shell-script syntax validation;
- first-party JavaScript syntax/tests available without missing external packages;
- source sweeps for direct SQLite constructors, security-sensitive best-effort unlink, and active restore use of generic `.previous` replacement.

The 2a.19 validator specifically checks the new revocation-transition ordering/exclusivity, whole-library derivative/Vault gates, restore ownership handoff, protected revision authorization lifetime, inherited adversarial regressions and direct SQLite authority.

## Targeted regression coverage

Source-level Rust regressions added/retained for the affected surfaces include:

- same-note conflicting revocation transitions are rejected while the first storage boundary remains active;
- whole-library and note-level revocations reject overlap in both directions and block Unlock for the active transition;
- library plaintext transition drains all note producers and blocks new producers;
- whole-library restore retires old-only canonical plaintext;
- incoming restore DB is seeded with old-only cleanup ownership before the snapshot switch;
- shared-hash restore repair uses current canonical ownership rather than a dangerous generic owner;
- protected revision restore reseals the encrypted payload without repopulating ordinary revision plaintext;
- hard delete waits for plaintext producer lifetime before staging cleanup/deletion;
- protected→unprotected→protected residue invariants;
- FTS/WAL/backup plaintext absence;
- offline historical backup privacy obligation;
- corrupt protected emergency-journal pre-quarantine handling;
- attachment ingestion and generic staging cleanup ownership inherited from 2a.16–2a.18.

These Rust tests are source regressions in this environment; they are not represented as executed native tests.

## Native Rust toolchain intentionally not run

No claim is made for:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The project owner explicitly asked 2a.19 not to spend time on native Rust toolchain validation in this environment. The static validators are configured to use their `RUSTNOTE_E*_STATIC_ONLY=1` paths where applicable.

## Executed non-Rust QA

The final repaired tree was exercised with the validation available in this environment. Results:

```text
Security E0 static contract passed (build 2a.19; Cargo checks skipped by RUSTNOTE_E0_STATIC_ONLY=1).
Security E1 static contract passed (build 2a.19; Cargo checks skipped by RUSTNOTE_E1_STATIC_ONLY=1).
Security E2 static contract passed (build 2a.19; Rust compilation skipped by RUSTNOTE_E2_STATIC_ONLY=1).
Security E3 static contract passed (build 2a.19; Rust compilation skipped).
E0-E3 2a.16 canonical plaintext ownership guard passed.
E0-E3 2a.17 persistent residue and backup closure guard passed.
E0-E3 2a.18 UI privacy lifetime + protected revision restore guard passed.
E0-E3 2a.19 independent adversarial closure guard passed.
E0-E3 2a.19 cumulative independent adversarial closure guard passed.
```

Additional non-Rust QA:

```text
Shell syntax: 33 scripts passed bash -n.
First-party JavaScript syntax: 43 JS/MJS files passed `node --check`.
Browser behavior: 36 tests passed, 0 failed.
Python syntax: no first-party Python files to validate.
```

The browser runner reported that editor-build dependencies are not installed in this environment, so it executed the dependency-free editor/PDF behavior set only. This is recorded as an environment limitation rather than silently treated as full editor-runtime coverage.

The B6 static architecture validator also reports the same pre-existing `src/ui/note_metadata.rs` UI→storage implementation dependency in both the untouched 2a.18 baseline and the final 2a.19 tree. During 2a.19 QA, a new revision-restore UI→storage dependency introduced by the repair was found and removed by wrapping the storage authorization in an opaque application-layer token. Therefore 2a.19 introduces no additional B6 layering regression; the unchanged baseline B6 finding is outside this security-closure pass.

The final source sweep also found no production `rusqlite::Connection`/`Connection::open` constructor outside `src/storage`, no ignored security-sensitive unlink in the newly changed critical regions, and no active restore path using the old generic `.previous` replacement mechanism.

## Architecture regression probe

A broader non-Rust architecture probe was also run because 2a.19 touched storage/application/UI boundaries. The A1–A4 validators reached and passed their own static boundary checks before nested validator execution hit this environment's missing `/bin/zsh`. B4's dependency-free behavioral suite passed 36/36.

Two architecture validators fail **identically on the untouched 2a.18 baseline** and therefore are not 2a.19 regressions:

- A6 expects 17 public native operations but both 2a.18 and 2a.19 currently expose 18;
- B1 flags the pre-existing `src/ui/note_metadata.rs` direct `crate::storage` reference in both 2a.18 and 2a.19.

2a.19 does not widen those baseline architecture issues. In particular, the new revision-lifetime authorization is wrapped at the application layer before the UI receives it. These pre-existing architecture-validator items should be handled in a later architecture cleanup rather than expanding this security closure pass.

## Final QA verdict

**PASS for the requested 2a.19 source/static and available non-Rust closure scope.** The independent adversarial audit found real defects, those defects were repaired, the affected surfaces were re-audited, and the cumulative E0–E3 source-level guards pass on the final tree.

This verdict intentionally excludes native Rust compilation, Clippy and Rust test execution, exactly as requested by the project owner. It also inherits the project's prior explicit treatment of E0 representative-device benchmarking as an external acceptance item rather than fabricating empirical results in this environment.

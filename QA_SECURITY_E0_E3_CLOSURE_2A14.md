# Rustnote 0.12.5 build 2a.14 — independent final closure QA

## Audit method
- [x] Reconstructed E0–E3 from original contracts rather than from 2a.13 validator markers.
- [x] Traced persistent SQLite/WAL/FTS state directly.
- [x] Traced attachment bytes/metadata, revisions, sources/citations, PDF annotations and tags.
- [x] Traced emergency journal including quarantine.
- [x] Traced automatic backups, complete archives and plaintext export guards.
- [x] Traced image/media caches, WebView previews, image/PDF staging and native handoffs.
- [x] Traced Lock/read permit lifetime through UI and native protocol consumers.
- [x] Traced protect/unprotect durable crash states and repeated transition cycles.
- [x] Traced temporary ownership/abnormal-termination semantics.

## Concrete findings fixed
- [x] PDF annotation-pages/index retain protected `ManagedAttachment` lease through response construction.
- [x] Protected stale emergency-journal deletion failure propagates instead of warning-and-continuing.
- [x] Target-scoped emergency-journal quarantine is removed at Vault autosave barrier and on startup for already protected notes.

## Regression evidence added
- [x] Protected PDF annotation response guard keeps Lock waiting until response scope drops.
- [x] Protected recovery cleanup failure is deterministic and fail-closed.
- [x] Protected recovery cleanup removes known plaintext record.
- [x] Startup removes legacy quarantined plaintext for a protected note.
- [x] Quarantine cleanup leaves unrelated note quarantine untouched.

## Native acceptance policy
Cargo/rustc/rustfmt/Clippy/native Rust tests are intentionally not executed here by user instruction. The E0 representative-hardware benchmark is likewise empirical evidence that must come from a representative native target if not already accepted.

## Non-Rust acceptance results
- [x] Independent E0–E3 2a.14 static regression closure passed.
- [x] E0 static contract passed.
- [x] E1 static contract passed.
- [x] E2 static contract passed.
- [x] E3 static contract passed with the 2a.14 regressions.
- [x] A1–A7 and B1–B6 cumulative static architecture closure passed.
- [x] Dependency-free browser/editor/PDF behavioral suite: 36/36 passed.
- [x] Rust regression tests discovered in source by architecture guards: 182.
- [x] JavaScript/MJS syntax: 45/45 files passed.
- [x] Shell syntax: 29/29 files passed.
- [x] TOML parsing: 6/6 files passed.
- [x] No merge-conflict markers in first-party source.
- [x] 2a.13→2a.14 diff has no whitespace errors.
- [x] Baseline comparison confirms production changes are limited to `src/attachment_protocol.rs`, `src/autosave/emergency_journal.rs`, and `src/storage/protection.rs`.

These results intentionally exclude Cargo/rustc/rustfmt/Clippy/native Rust execution and the representative-hardware E0 benchmark by prior user instruction.

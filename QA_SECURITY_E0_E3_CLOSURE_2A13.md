# Rustnote 0.12.5 build 2a.13 — adversarial behavioral closure QA

## Scope
Bounded attack of the twelve agreed E0–E3 behavioral scenarios. No new security architecture is introduced in this pass.

## Behavioral regression matrix
- [x] Encrypt/read/cache/staging concurrency regression present.
- [x] Lock/read revocation/publication regressions present.
- [x] All four durable protect/unprotect crash states have direct recovery regressions.
- [x] Post-commit managed-filesystem cleanup failure/restart retry regression present.
- [x] Stale autosave/UI publication barriers covered.
- [x] Restart recovery uses normal library reopening in the newly added crash cases.
- [x] Backup location change/reuse regressions present.
- [x] Cross-library backup ownership/isolation regressions present.
- [x] Repeated protected→unprotected→protected cycle regression present.
- [x] Image replacement uses the same gated staged-image import path; pending attachment ingest/removal vs Encrypt regression present.
- [x] SQLite/WAL/FTS/revision/autosave residue regressions present.
- [x] V2 temp ownership/abnormal-termination regressions present.

## Native acceptance policy
The Rust regression tests are source-complete but are intentionally not executed in this environment, per user instruction. Cargo/rustc/rustfmt/Clippy/native Rust tests remain the user's native acceptance gate.

## Non-Rust acceptance results
- [x] E0–E3 2a.13 static security closure passed.
- [x] A1–A7 and B1–B6 cumulative static architecture closure passed.
- [x] Rust regression tests discovered by architecture guards: 177.
- [x] Dependency-free browser/editor/PDF behavior suite: 36/36 passed.
- [x] JavaScript/MJS syntax: 45/45 files passed.
- [x] Shell syntax: 29/29 files passed.
- [x] TOML parsing: 6/6 files passed.
- [x] `git diff --check` passed.
- [x] No merge-conflict markers in first-party non-document source.

These results intentionally exclude Cargo/rustc/rustfmt/Clippy/native Rust execution by user instruction.

## Adversarial outcome
- No production Rust defect was demonstrated by the twelve bounded attacks; production Rust remains unchanged from 2a.12.
- Three adversarial-evidence weaknesses were corrected: cleanup-failure injection is deterministic rather than Unix-permission-dependent, and the prepared-protect / committed-unprotect crash regressions now use normal library reopen so all four durable crash states exercise startup recovery.
- 2a.13 therefore strengthens executable behavioral evidence and closure guards without extending the 2a.12 Vault architecture.

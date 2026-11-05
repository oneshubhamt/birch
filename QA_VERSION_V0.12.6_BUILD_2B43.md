# QA — Rustnote v0.12.6 / build 2b.43 promotion

Promotion-only pass from the closed 2b.42.11 pre-S1 baseline.

## Result

- Canonical Cargo package version: `0.12.6` — PASS
- Committed build ID: `2b.43` — PASS
- Full runtime identifier derivation: `0.12.6+2b.43` — PASS
- macOS `CFBundleShortVersionString`: `0.12.6` — PASS
- macOS `CFBundleVersion`: `0.12.6` — PASS
- Root `rustnote` `Cargo.lock` package version: `0.12.6` — PASS
- Historical/current-lineage validator acceptance for `2b.43`: PASS
- Shell syntax: 89/89 PASS
- JavaScript/module syntax: 54/54 PASS
- E0–E3 cumulative static closure on `2b.43`: PASS
- Browser/source behavior suite: 111/111 PASS
- Production behavior changes: none

Native Cargo/Clippy/Rust tests remain an external Mac gate because this sandbox has no Rust toolchain.

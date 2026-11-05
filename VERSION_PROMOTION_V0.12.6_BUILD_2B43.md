# Rustnote v0.12.6 / build 2b.43 promotion

## Purpose

Promote the fully closed 2b.42.11 pre-S1 source baseline to the next application version/build without changing production behavior.

## Canonical identifiers

- Application version: `0.12.6`
- Build ID: `2b.43`
- Full runtime identifier: `0.12.6+2b.43`

## Changed authority files

- `Cargo.toml`
- root `rustnote` package entry in `Cargo.lock`
- `build-id.txt`
- `bundle/macos/Info.plist`

Historical architecture/QA documents retain their original version/build references as provenance.

Inherited validators whose current-lineage build acceptance previously ended at `2b.41` were extended to recognize `2b.43`. Their behavioral assertions were not weakened.

## QA

- shell syntax: PASS
- JavaScript/module syntax: PASS
- 2b.42 final pre-S1 inherited/static chain: PASS through all substantive validators; long monolithic invocation may exceed sandbox command limits, so its cumulative E0–E3 tail was also run directly and passed
- E0–E3 cumulative static closure: PASS on build `2b.43`
- browser/source behavior suite: 111/111 PASS
- promotion-specific metadata validator: PASS

Native Rust compilation/Clippy/tests remain a Mac gate because this sandbox does not provide `cargo`/`rustc`.

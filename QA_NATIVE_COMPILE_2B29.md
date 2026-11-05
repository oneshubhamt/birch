# Rustnote 2b.29 — QA

## Targeted source checks

- Cargo deployment target is exactly macOS 12.0 and forced project-wide: PASS.
- AEGIS remains exactly 0.9.15 with no `pure-rust` feature enabled: PASS.
- macOS `Info.plist` declares `LSMinimumSystemVersion = 12.0`: PASS.
- prepare and bundle workflows validate the deployment contract before compiling: PASS.
- one centralized Mach-O validator inspects the actual executable minimum with `otool`; preparation and app-bundle validation both reuse it: PASS (source/syntax); native artifact inspection remains a Mac gate.
- no competing deployment-target export exists in Rustnote's macOS scripts: PASS.
- all shell scripts pass `bash -n`: PASS.

## Inherited/static QA available in the sandbox

- Vault 2b.1 through 2b.6: PASS individually.
- Vault 2b.7 targeted closure: PASS before its recursive inherited re-run reached the sandbox command-time limit.
- Vault 2b.8, 2b.9 and 2b.10: PASS individually.
- E0-E3 cumulative static closure, including 2a.16 through 2a.19: PASS.
- Dependency-free editor/PDF behavioral suite: 36/36 PASS.
- No Rust source file differs from build 2b.28: verified.

## Native closure required on macOS

The sandbox does not provide the user's x86_64 macOS/Rust toolchain. Native confirmation therefore requires a fresh project-tree build on macOS and must verify that the former libaegis 13.1-vs-10.12 linker warning is absent, followed by Cargo check, strict Clippy, the complete Rust test suite, and release/bundle validation.

`validate-architecture-a0.sh` is also Cargo-backed and therefore cannot run in this sandbox; it remains part of the Mac release gate.

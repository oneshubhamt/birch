# Rustnote 2b.29 — macOS 12 / AEGIS deployment-target closure plan

## Audit finding

The native x86_64 release build linked Rustnote with Rust's default macOS 10.12 deployment baseline while libaegis native objects were compiled by the installed macOS 13.1 SDK. The resulting binary worked on macOS 12.7.6 but emitted a linker warning because the Rust and native-object minimum-version contracts disagreed.

The app bundle also did not declare `LSMinimumSystemVersion`, so packaging did not explicitly state the supported macOS baseline.

## Objectives

1. Keep the current optimized native AEGIS 0.9.15 dependency unchanged.
2. Define macOS 12.0 as one project-wide minimum deployment target for Cargo, rustc, native dependency build scripts, and the final linker.
3. Prevent ambient shell configuration from silently changing that target.
4. Declare the same 12.0 minimum in the macOS application bundle.
5. Validate the built Mach-O minimum version rather than trusting source configuration alone.
6. Preserve all inherited Vault/E0-E3 behavior and schema unchanged.

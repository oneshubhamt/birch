# Rustnote 2b.29 — macOS 12 / AEGIS deployment-target closure implementation

Build 2b.29 adds an explicit project-level macOS compatibility contract without changing cryptography or runtime behavior.

- `.cargo/config.toml` defines `MACOSX_DEPLOYMENT_TARGET = 12.0` with `force = true`. Cargo therefore supplies the same minimum target to rustc and native dependency build scripts such as libaegis, independent of the caller's ambient shell variable.
- `bundle/macos/Info.plist` declares `LSMinimumSystemVersion = 12.0`.
- `scripts/validate-macos-deployment-target.sh` verifies the Cargo target, bundle metadata, AEGIS 0.9.15 pin, absence of the slower `pure-rust` AEGIS feature, and absence of competing shell-script overrides.
- macOS preparation and app bundling run that deployment-target validator before compiling.
- `scripts/validate-macos-binary-deployment-target.sh` is the single Mach-O minimum-version authority; preparation validates `target/release/rustnote` immediately after building, and app-bundle validation reuses the same helper for the packaged executable.

AEGIS remains pinned to 0.9.15 and retains its optimized native backend. No Vault object format, schema, encryption primitive, key derivation, note behavior, or application runtime logic is changed.

# macOS application packaging — current contract

## Objective

Produce a normal macOS `Rustnote.app` through Dioxus 0.7.10 while preserving Rustnote's release optimizations and validating the actual bundle contract.

## Build path

`scripts/build-rustnote-macos-app.sh`:

1. verifies `dx 0.7.10`;
2. validates the committed Dioxus 0.7.10 Cargo baseline;
3. prepares PDF.js and the ProseMirror runtime;
4. runs `dx bundle --desktop --release --package-types macos`;
5. validates the generated bundle before copying it to `dist/Rustnote.app`;
6. validates the final copied bundle again.

No strip override is used. Dioxus 0.7.10 owns its supported asset-processing/stripping pipeline and Rustnote retains `strip = true` in `[profile.release]`.

## Bundle metadata contract

The custom `bundle/macos/Info.plist` is complete and explicitly contains:

- `CFBundleExecutable = rustnote`;
- `CFBundleIdentifier = local.rustnote.app`;
- `CFBundlePackageType = APPL`;
- Rustnote short/build version metadata;
- `LSMinimumSystemVersion = 12.0`, matching the Cargo/native deployment baseline;
- high-resolution support;
- local-network permission required by Rustnote's loopback media transport.

Because `dx 0.7.10` uses the custom plist as the bundle plist, required macOS keys must not be omitted.

## Asset contract

A valid bundle must contain non-empty first-class resources for:

- Rustnote main CSS;
- embedded KaTeX CSS;
- prepared ProseMirror editor runtime.

The validator accepts either original or Dioxus-hashed filenames and verifies file contents rather than searching the executable for implementation-specific asset strings.

## Deployment-target contract

Rustnote targets macOS 12.0 as its minimum supported macOS version. `.cargo/config.toml` forces `MACOSX_DEPLOYMENT_TARGET=12.0` so rustc and native dependencies such as libaegis compile against the same baseline. The shared binary deployment validator inspects the Mach-O load command with `otool` after the normal release build and again for the packaged app; source configuration and plist metadata alone are not sufficient acceptance evidence.

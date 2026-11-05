# Dioxus 0.7.10 stable baseline — build 1a.35

## Objective

Make Dioxus 0.7.10 the normal Rustnote baseline rather than an upgrade/migration state.

## Contract

- `dioxus` and `dioxus-desktop` are exactly pinned to `0.7.10`.
- `Cargo.lock` is committed already resolved to the Mac-verified Dioxus/Manganis 0.7.10 family.
- No preparation, validation, or packaging script may run `cargo update` or mutate `Cargo.lock`.
- Normal build/check/test paths remain `--locked`.
- `dx 0.7.10` is required for the macOS bundling path so CLI and application crates stay aligned.
- Rustnote keeps its normal release profile: Thin LTO, one codegen unit, and `strip = true`.
- Rustnote runtime/editor source is unchanged by this consolidation pass.

## Required QA

1. `./scripts/validate-dioxus-0.7.10-baseline.sh`
2. `cargo check --locked`
3. `cargo clippy --locked --all-targets -- -D warnings`
4. `cargo test --locked`
5. ordinary `cargo run --locked` launch
6. `./scripts/build-rustnote-macos-app.sh`
7. packaged `.app` launch and frontend/PDF/editor smoke test

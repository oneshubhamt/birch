# Development and local build — macOS primary

This guide owns **build and developer commands**. For the architecture see [Architecture](ARCHITECTURE.md); for interpreting a PASS see [QA](QA.md).

## Requirements

- macOS with Xcode Command Line Tools (`xcode-select --install`). Minimum configured deployment target: **macOS 12.0** (`.cargo/config.toml`).
- Rust installed using [rustup](https://rustup.rs), with `rust-toolchain.toml` selecting **1.97.1**, `rustfmt` and `clippy`.
- Node.js/npm (CI uses **Node 22**). Pinned JS dependencies are in `editor-build/`, not the repository root.
- Python **3.11+** (source validators use `tomllib`), `curl`, `tar`, `shasum` and Apple command-line tools.
- Optional for creating `Rustnote.app`: Dioxus CLI **0.7.10** (`dx`), matching the framework's pinned version.

## Prepare and run locally

```sh
cd birch
chmod +x scripts/*.sh
./scripts/prepare-rustnote-mac.sh
./scripts/run-rustnote-mac.sh
```

The preparation script verifies the deployment baseline, installs exact JS dependencies if needed, fetches a checksum-verified PDF.js package, generates the editor/PDF browser runtimes, builds release Rust with `--locked`, and validates the produced binary's macOS deployment target. **An internet connection is needed for the initial dependency/runtime preparation.** Generated artifacts are untracked; retain the source generators and lockfiles.

## Diagnostic and QA commands

Source-level checks (also available on Linux with Node.js and Python):

```sh
bash scripts/qa-release.sh
```

Dependency-backed ProseMirror/browser behavior:

```sh
npm ci --prefix editor-build --no-audit --no-fund
node scripts/run-browser-behavior-tests.mjs --require-editor-deps
```

Full native Rust check sequence (macOS, after toolchain setup):

```sh
./scripts/prepare-rustnote-browser-runtimes.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --release --locked
./scripts/validate-macos-binary-deployment-target.sh target/release/rustnote
```

The browser preparation step produces generated runtimes needed by Rust build. `qa-release.sh` composes the five independent concern suites once each; it does not traverse historical pass-numbered validators or run Cargo. Dependency-backed ProseMirror/bridge behavior and the native Cargo matrix have separate single owners in CI. Linux-only checks do not demonstrate macOS framework compatibility, Keychain behavior or native runtime privacy.

For diagnosis of concurrency-sensitive native failures, the all-feature test suite may also be rerun serially:

```sh
cargo test --workspace --all-features --locked -- --test-threads=1
```

Normal CI intentionally keeps Rust's default parallel test execution because shared-process/session lifetime races are themselves useful regression coverage. Q2 exposed and repaired one such private-temporary-root creation race.

## Unsigned local app bundle

Install the exactly matching Dioxus CLI 0.7.10, confirm `dx --version`, then:

```sh
./scripts/build-rustnote-macos-app.sh
open dist/Rustnote.app
```

The output is a **local unsigned development bundle**. No automated Apple signing, notarization or distribution-release publication is configured here.

## Change protocol

Before changing code: inspect current source and governing contracts; state objectives, invariants and performance/security implications; implement a coherent minimal change; independently check acceptance criteria; run targeted behavioral/negative tests; then execute broader QA after substantial changes. Historical pass documents and validators are provenance in Git, not executable compatibility surfaces. See [Contributing](../CONTRIBUTING.md) and the [archive/no-loss map](ARCHIVE.md).

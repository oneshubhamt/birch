# Development guide (macOS primary)

## Prerequisites

- macOS with Xcode Command Line Tools (`xcode-select --install`). The repository is configured for minimum deployment target **macOS 12.0**.
- Rust installed using [rustup](https://rustup.rs); `rust-toolchain.toml` selects **1.97.1** including `rustfmt` and `clippy`.
- Current Node.js LTS with npm. CI installs Node.js **22**. The JavaScript build packages are in `editor-build/`, not the repository root.
- Python **3.11+** (for `tomllib` in source validators), `curl`, `tar`, `shasum` and standard Apple command-line tools.
- Optional for creating a `.app`: Dioxus CLI **0.7.10** (`dx`). Do not mix CLI versions with the pinned Dioxus framework.

## First local build

```sh
cd birch
chmod +x scripts/*.sh
./scripts/prepare-rustnote-mac.sh
./scripts/run-rustnote-mac.sh
```

The preparation script validates the deployment baseline, installs exact npm dependencies as needed, fetches checksum-verified PDF.js assets, generates editor/PDF runtimes, and builds the Rust release target using `--locked`. Generated outputs are excluded from Git; **do not delete the source generators or lockfiles**.

## Developer QA

```sh
bash scripts/ci-source-checks.sh
npm ci --prefix editor-build --no-audit --no-fund
node scripts/run-browser-behavior-tests.mjs --require-editor-deps

./scripts/prepare-rustnote-browser-runtimes.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
./scripts/validate-macos-binary-deployment-target.sh target/release/rustnote
```

The source-only commands work on Linux with Node.js and Python; Rustnote is currently a **macOS-first desktop application** and Linux source validation does not prove Mac native compatibility.

## macOS application bundle

Install the exactly matching Dioxus CLI 0.7.10 according to its current installation instructions, check `dx --version`, then run:

```sh
./scripts/build-rustnote-macos-app.sh
open dist/Rustnote.app
```

This builds an **unsigned local development bundle**. No distribution signing, Apple notarization or release publication is configured.

## Where to change code

Refer to [Architecture](ARCHITECTURE.md). Keep each pass scoped, audit before changes, and verify affected Vault/UI/storage lifetimes. The root-level pass reports are historical evidence and exact-path static-validator inputs; don't move them as an incidental cleanup.

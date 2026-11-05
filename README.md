# Rustnote — Birch repository

**Application:** Rustnote · **GitHub repository:** `birch` · **Source baseline:** v0.12.6, build 2b.43 (through 2b.43.3 secure-key-store native fix 3).

Rustnote is a local-first, offline notes application built with Rust, Dioxus 0.7.10, SQLite, and a ProseMirror-based editor. This private repository holds Rustnote's active development source. The repository name **Birch** does not rename the Rust crate, application bundle, stored data formats, or cryptographic identifiers. Such a rename would require a separate, carefully reviewed architecture pass.

> **Development status:** This is a source development snapshot, not a signed or security-certified production release. The 2b.43.3 key-store integration is a foundation for later encrypted backup cutover; the 2b.43.4 backup persistence cutover is not present in this snapshot. Do not treat historical QA reports as proof of a current end-to-end verification.

## Getting started (macOS)

Supported macOS deployment target in this source: **12.0**. Building requires Apple's Xcode Command Line Tools, Rust (see `rust-toolchain.toml` for exact version), Node.js and npm. The Dioxus CLI 0.7.10 is additionally required to produce an `.app` bundle.

```sh
xcode-select --install    # If not already installed
cd birch
chmod +x scripts/*.sh
./scripts/prepare-rustnote-mac.sh
./scripts/run-rustnote-mac.sh
```

`prepare-rustnote-mac.sh` prepares pinned browser runtimes, performs a locked Rust release build, and validates the macOS deployment target. It downloads npm dependencies and a checksum-verified PDF.js archive; **an internet connection is needed for the first build**. Rust installation: https://rustup.rs/ . Node.js: https://nodejs.org/ . For creating `Rustnote.app`, install `dx` version 0.7.10 and run `./scripts/build-rustnote-macos-app.sh` (see [Development](docs/DEVELOPMENT.md)).

## Source map

| Directory | Role |
| --- | --- |
| `src/` | Desktop application, UI, SQLite, storage, search and platform adapters |
| `crates/rustnote-vault/` | Vault cryptographic primitives and privacy-related core |
| `crates/rustnote-media-compat/` | Media compatibility crate |
| `editor-src/`, `editor-spec/` | ProseMirror editor source and document contract |
| `editor-build/` | Locked JavaScript build dependencies |
| `pdf-src/`, `assets/` | PDF viewer source and static bundled application assets |
| `scripts/`, `tests/` | Preparation, contract validators, tests and checks |
| `bundle/macos/` | macOS bundle metadata |
| `docs/` | Current developer and repository-operation guidance |

## Verification

```sh
bash scripts/ci-source-checks.sh
# On macOS, after preparation:
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The GitHub Actions workflow exercises source and browser contracts on Linux and native Rust checks on macOS. **Native Keychain functionality, app signing, and device-level privacy behavior still require dedicated macOS validation**. See [QA and release gates](docs/QA.md).

## Documentation

- [Architecture overview](docs/ARCHITECTURE.md)
- [Development and testing](docs/DEVELOPMENT.md)
- [QA and release gates](docs/QA.md)
- [Current source status](docs/PROJECT_STATUS.md)
- [Security model and reporting](SECURITY.md)
- [Contribution workflow](CONTRIBUTING.md)
- [GitHub setup and initial publication](docs/REPOSITORY_SETUP.md)

The root-level `VAULT_*`, `QA_*`, `SECURITY_*`, `BACKUP_*`, and `NATIVE_COMPILE_*` documents are **historical build contracts, audits and implementation notes**. They are retained in place because existing source validators refer to them by exact path. `docs/` provides the current entry point; don't relocate or silently rewrite the historical records.

## License

MIT; see [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled third-party components and license information.

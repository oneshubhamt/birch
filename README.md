# Birch — Rustnote development repository

**Application:** Rustnote · **Rust package:** `rustnote` · **source baseline:** v0.12.6, build 2b.43 (through the 2b.43.3 secure-key-store native-fix snapshot).

Birch is the private development repository for **Rustnote**, a local-first, offline notes application using Rust, Dioxus 0.7.10, SQLite and ProseMirror. The repository name **Birch does not rename** the application, Rust crate, bundled app, existing data formats or cryptographic identifiers.

> **Development snapshot, not a signed or security-certified release.** The backup key authority and OS secure-store adapter introduced in 2b.43.3 are staged source components; the active managed-backup writer still uses repository **format v2**. The proposed encrypted v3 persistence/restore cutover (2b.43.4) is not present. See [documented source status](docs/PROJECT_STATUS.md).

## Quick start — macOS

The configured minimum deployment target is **macOS 12.0**. First preparation requires Xcode Command Line Tools, Rust/rustup, Node.js/npm and internet access for pinned dependencies and PDF.js runtime assets.

```sh
cd birch
chmod +x scripts/*.sh
./scripts/prepare-rustnote-mac.sh
./scripts/run-rustnote-mac.sh
```

For building an **unsigned local** `Rustnote.app`, install the matching Dioxus CLI **0.7.10** and follow the [development guide](docs/DEVELOPMENT.md). See [QA](docs/QA.md) for source, browser and native test commands and evidence requirements.

## Documentation

Start with the **[documentation index](docs/README.md)**, which separates current purpose-based guides from historical pass records:

- [Architecture and source map](docs/ARCHITECTURE.md)
- [Development, tooling and build steps](docs/DEVELOPMENT.md)
- [QA, CI and release gates](docs/QA.md)
- [Implemented/planned source status](docs/PROJECT_STATUS.md)
- [Security policy and vulnerability reporting](SECURITY.md)
- [Contribution and review discipline](CONTRIBUTING.md)
- [Third-party notices and licences](THIRD_PARTY_NOTICES.md)

The final [archive and no-loss map](docs/ARCHIVE.md) records the 239-file baseline, immutable historical locations, remaining validator fixtures and consolidation evidence.

**Do not treat historical QA reports as verification of the current branch.** Pass-specific history has been consolidated into current-purpose documentation. Exact baseline originals remain available through Git. Root pass-numbered files that remain are compatibility fixtures because current validators still read them directly.

## Licence

MIT; see [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled third-party obligations.

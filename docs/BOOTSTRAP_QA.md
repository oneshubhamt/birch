# Birch repository bootstrap QA (2026-11-05)

## Scope

Imported the original Rustnote v0.12.6, build 2b.43 / 2b.43.3 secure-key-store native fix 3 source archive into the `birch` working tree **without changing any original application source file**. New files are repository-only documentation, CI/governance and a source-QA runner. Git executable bits are set on shell/build scripts.

## Verified

- Original source archive SHA-256: `35b1a858fab68b741b2861079a0f5add2b6102c4ae0fe80f3b18e41a1af8c094`.
- All original regular archive entries have byte-identical content in the working tree after bootstrap.
- `validate-version-v0.12.6-build-2b43.sh`: PASS.
- `validate-backup-2b43-1-key-contract.sh`: PASS (12 contracts).
- `validate-backup-2b43-2-byte-format.sh`: PASS (18 objectives).
- `validate-backup-2b43-3-secure-key-store.sh`: PASS (29 contracts).
- `node scripts/run-browser-behavior-tests.mjs` without installed editor dependencies: PASS (111 tests).
- `bash scripts/ci-source-checks.sh`: PASS (shell syntax, JS syntax, contract validators, 111 browser tests).
- Git status, bundle manifest, YAML syntax, archive round-trip and Git object integrity: verified separately in packaging QA.

## Outstanding checks

- Native macOS compile, strict Clippy, all Rust tests and macOS deployment target (Rust and macOS framework toolchain unavailable here).
- Dependency-backed ProseMirror behavior tests (`npm ci` cannot run in this isolated environment).
- Production Keychain integration, privacy races, actual device testing, app signing/notarization.
- GitHub CI checks (require first push) and repository branch protection/security settings (require administrative UI verification).

A source-contract validator is a narrow assertion, not a proof of cryptographic correctness or production readiness.

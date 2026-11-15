# Quality assurance and release gates

A validator PASS demonstrates only its stated assertions in its stated environment and revision. It does **not** prove cryptographic correctness, implementation completeness or a release-ready application. The [project status](PROJECT_STATUS.md) separates actual source evidence, planned work and outstanding testing.

## Configured CI jobs

The workflow is [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). Triggers: pushes to **`main`**, pull requests, and manual dispatch. A push to this working branch alone does not trigger a workflow run under the current push filter.

| CI job | Environment | Gates |
| --- | --- | --- |
| Source and browser contracts | Ubuntu, Node.js 22, Python 3.12 | Syntax/source checks, dependency-free JS tests; `npm ci` and dependency-backed editor/browser tests |
| Native macOS build/test | Hosted `macos-15`, Node.js 22, Python 3.12, pinned Rust 1.97.1 | Prepare browser runtimes; `cargo fmt`; all-target/all-feature `cargo check`; strict all-feature Clippy; all-feature workspace Rust tests; release build; deployment-target binary check |

The `scripts/ci-source-checks.sh` runner checks shell and JS syntax, invokes exactly **four** baseline shell validators (version 2b.43, key-authority 2b.43.1, byte-format 2b.43.2 and secure-key-store 2b.43.3), then runs the concern-based `validate-native-current.sh` suite. That suite owns the current Dioxus/deployment source contracts and dependency-free browser/privacy/storage contract corpus. CI **does not execute the historical pass-numbered native validator chain**. Static/source-contract PASS results are regression evidence, not substitutes for the native macOS job.

See [Development](DEVELOPMENT.md) for exact commands. Record **commit, OS/version, CPU architecture, command, dependencies, exit/result, duration if measured, and untested or skipped surfaces** for every new evidence claim. Historical QA evidence is consolidated in [the 104-report ledger](qa/HISTORICAL_EVIDENCE.md). Exact baseline originals remain available through immutable Git links; only reports still read directly by validators remain at root.


## Current test/requirement traceability

| Property being checked | Primary current mechanism | Evidence type | What it does **not** establish alone |
| --- | --- | --- | --- |
| Package/build identity and baseline configuration | `validate-version-v0.12.6-build-2b43.sh`, Dioxus/deployment validators | Static/configuration | Runtime correctness or security |
| Editor document grammar | `validate-editor-document-spec.mjs`, `validate-editor-document-contract.mjs`, shared corpus | Executable JS/spec contract | Native Dioxus/WebView integration |
| Editor/PDF/browser behavior | `run-browser-behavior-tests.mjs`; dependency-backed mode with `--require-editor-deps` | Executable JS behavior/source contracts | Rust storage, Keychain, filesystem crash semantics |
| Architecture A0–B6 | `validate-architecture-*.sh` | Predominantly source/static guards; some chained behavior tests | End-to-end native correctness merely because patterns are present |
| E0–E3 security | `validate-security-*.sh` plus Rust tests embedded in affected modules | Static contract guards + executable Rust regressions when Cargo runs | Cryptographic proof, hostile OS, or hardware/device behavior |
| Vault I1–I7 / privacy authority | `validate-vault-*.sh` + Rust concurrency/ownership tests | Source/static guards + executable Rust tests | Current certification if only static mode ran |
| Backup 2b.43 key/format authority | `validate-backup-2b43-1/2/3-*.sh` | Contract/static/source tests | Deployed encrypted-v3 writer/restore, which is not yet integrated |
| Native build quality | `cargo fmt`, `cargo check`, strict `cargo clippy`, `cargo test` | Compiled native/toolchain QA | Supported-device matrix, signing/notarization, manual UI/adversarial behavior |
| Feature-gated native lifetime tests | all-target/all-feature `cargo check`, Clippy and workspace tests; notably `internal-test-support` binary tests | Compiled feature-matrix QA | Native OS/hardware integration beyond the hosted runner |
| macOS deployment | prepared-runtime build + Mach-O deployment validator | Native artifact inspection | Keychain privacy semantics or general application correctness |
| Keychain/OS privacy lifetime | Manual/adversarial supported-device testing | Native integration/manual | Cannot be replaced by source grep or mock secure-store tests |

**Static phrase/presence checks are regression guards, not behavioral proof.** Their strongest use is preventing a known architecture from silently drifting. Where a security requirement depends on ordering, concurrency, crash recovery, filesystem behavior, cryptographic round-trip, or OS integration, closure should include an executable behavioral/property test at the lowest useful boundary and a native integration test when the OS boundary matters.

See [historical evidence](qa/HISTORICAL_EVIDENCE.md) for how older reports mixed these evidence classes.

## Current native feature coverage

Q2 closes the earlier feature-matrix gap. The configured macOS CI now runs `cargo check --workspace --all-targets --all-features --locked`, strict Clippy with `--all-features`, and `cargo test --workspace --all-features --locked`. Feature-gated `internal-test-support` lifetime tests are therefore part of the normal hosted native gate rather than an optional follow-up.

The production release build remains `cargo build --release --locked`, so test-support features are exercised for verification without being enabled in the shipped binary. See [native toolchain history](development/NATIVE_TOOLCHAIN_HISTORY.md).

## Manual and adversarial gates before distribution

1. Exercise native macOS **Keychain** first creation, existing root retrieval, denied/unavailable store, missing/deleted/corrupt root, invalid envelope, and supported-device/OS behavior.
2. Test privacy lock/unlock and stale async publication under concurrency, including title/body/attachments, dialogs, UI caches, temporary staging, revisions, SQLite/FTS/WAL and backups.
3. Cover failure, cancellation, interrupted filesystem operations, process-death/restart, recovery and cleanup ownership.
4. Test the **actual deployed** encrypted-backup persistence/restore format and fail-closed behavior **once the planned 2b.43.4 repository-v3 cutover exists**. Staged keyset tests alone are insufficient.
5. Validate `Rustnote.app` on supported macOS versions and relevant Apple Silicon and Intel hardware. Audit signing/notarization separately before a distributed release.
6. Review dependencies and secrets, run targeted QA for each changed surface, and comprehensive source/native/browser/security QA after significant cross-cutting changes.

## QA/document decoupling status

Q2 begins the executable-suite redesign: native pass-history documents are no longer QA inputs, the current native source suite is concern-based, and CI now owns the all-feature native matrix. Historical `QA_NATIVE_COMPILE_*.md` reports remain at root until Q4 by design. Security/Vault and other QA-report prose dependencies remain technical debt for Q3/Q4; pass-numbered validator-chain cleanup remains Q5 scope.

# Quality assurance and release gates

A validator PASS demonstrates only its stated assertions in its stated environment and revision. It does **not** prove cryptographic correctness, implementation completeness or a release-ready application. The [project status](PROJECT_STATUS.md) separates actual source evidence, planned work and outstanding testing.

## Configured CI jobs

The workflow is [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). Triggers: pushes to **`main`**, pull requests, and manual dispatch. A push to this working branch alone does not trigger a workflow run under the current push filter.

| CI job | Environment | Gates |
| --- | --- | --- |
| Source and browser contracts | Ubuntu, Node.js 22, Python 3.12 | Syntax/source checks, dependency-free JS tests; `npm ci` and dependency-backed editor/browser tests |
| Native macOS build/test | Hosted `macos-15`, Node.js 22, Python 3.12, pinned Rust 1.97.1 | Prepare browser runtimes; `cargo fmt`; strict workspace Clippy; workspace Rust tests; release build; deployment-target binary check |

The `scripts/ci-source-checks.sh` runner checks shell and JS syntax and invokes exactly **four** baseline shell validators: version 2b.43, key-authority 2b.43.1, byte-format 2b.43.2 and secure-key-store 2b.43.3. It then invokes the dependency-free browser/contract test runner. It **does not execute all 80+ historical `validate-*.sh` scripts**. Other static/source-contract-style browser checks are run separately by the JS runner; their PASS should not be confused with an end-to-end native behavior test.

See [Development](DEVELOPMENT.md) for exact commands. Record **commit, OS/version, CPU architecture, command, dependencies, exit/result, duration if measured, and untested or skipped surfaces** for every new evidence claim. Historic `QA_*` files are retained as records for their original versions and environments.

## Manual and adversarial gates before distribution

1. Exercise native macOS **Keychain** first creation, existing root retrieval, denied/unavailable store, missing/deleted/corrupt root, invalid envelope, and supported-device/OS behavior.
2. Test privacy lock/unlock and stale async publication under concurrency, including title/body/attachments, dialogs, UI caches, temporary staging, revisions, SQLite/FTS/WAL and backups.
3. Cover failure, cancellation, interrupted filesystem operations, process-death/restart, recovery and cleanup ownership.
4. Test the **actual deployed** encrypted-backup persistence/restore format and fail-closed behavior **once the planned 2b.43.4 repository-v3 cutover exists**. Staged keyset tests alone are insufficient.
5. Validate `Rustnote.app` on supported macOS versions and relevant Apple Silicon and Intel hardware. Audit signing/notarization separately before a distributed release.
6. Review dependencies and secrets, run targeted QA for each changed surface, and comprehensive source/native/browser/security QA after significant cross-cutting changes.

## Boundary of this documentation pass

The historical `QA_*` records may mix contract assertions, behavioral tests and point-in-time pass closure. Preserve the original results, limitations and failures until the later QA-suite audit. No QA scripts or CI configuration are changed as part of documentation consolidation.

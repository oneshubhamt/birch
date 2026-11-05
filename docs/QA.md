# Quality assurance and release gates

## CI checks

| Gate | Environment | Purpose |
| --- | --- | --- |
| Source, format, and contract checks | Ubuntu, Node 22, Python 3.12 | shell/JavaScript syntax, version and backup authority contracts, source-level JS behavioral tests |
| Full browser behavior | Ubuntu with `npm ci` | pinned ProseMirror test dependencies; required editor-specific tests |
| Rust build/test | Hosted macOS with Rust 1.97.1 | prepared browser runtimes; `cargo fmt`, strict Clippy, full locked workspace tests, release build, deployment target check |

CI workflows are defined in `.github/workflows/ci.yml`. They need network access to fetch GitHub actions, npm dependencies, PDF.js and Rust crates. They do not assert verified encryption guarantees by merely passing static source checks.

## Manual security checks required before distribution

1. Execute native macOS **Keychain integration** tests for creation, existing-key retrieval, denied/unavailable store, missing/deleted root, corrupt root/envelope, and platform-version compatibility.
2. Exercise privacy-lock and async-publication concurrency; ensure no plaintext leaks through dialogs, temporary paths, SQLite/FTS/WAL, backups or UI caches.
3. Reproduce failed/aborted operations and process-crash/restart recovery paths.
4. Check restored backup behavior in the format actually deployed; **2b.43.4 is not in the current snapshot**.
5. Verify `.app` runtime behavior on supported macOS versions and relevant Apple Silicon/Intel hardware. Sign/notarize separately before any distributed release.
6. Perform dependency review, repository secret scan and QA of the entire affected codebase after substantial architectural changes.

## Evidence convention

Document each suite's exact command, OS/architecture, exit code, tested source commit, and uncovered limitations. A validator PASS means its particular assertions passed; it does **not** prove implementation completeness. Historical QA files must not be retroactively presented as results for a new commit.

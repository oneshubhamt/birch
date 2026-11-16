# Quality assurance and release gates

A QA PASS demonstrates only the assertions executed for that revision and environment. It is regression evidence, not a proof of cryptographic correctness or a substitute for supported-device testing.

## Current suite architecture

Current source QA is organized by invariant domain:

```text
qa-release
  ├── qa-toolchain
  ├── qa-editor-browser
  ├── qa-security-privacy
  ├── qa-backup-format
  └── qa-architecture
```

The five concern suites are independently runnable and do not call one another. `qa-release.sh` is the only source-suite orchestrator and invokes each concern exactly once. Historical A/B, 2a and 2b pass chains are not part of the current executable QA surface.

| Suite | Primary responsibility |
| --- | --- |
| `qa-toolchain.sh` | package/build identity, pinned Rust/Dioxus/crypto dependencies, macOS deployment baseline |
| `qa-editor-browser.sh` | canonical editor document specification plus dependency-free editor, note-list and PDF behavior |
| `qa-security-privacy.sh` | crypto/profile owners, privacy publication/revocation, plaintext ownership, residue, crash/recovery and forensic JS contracts |
| `qa-backup-format.sh` | managed-backup key authority, exact format constants, secure-store boundary and recovery/fail-closed owners |
| `qa-architecture.sh` | one syntax scan, documentation decoupling, suite topology, layering, modularity, typed boundaries and storage-owned search |
| `qa-release.sh` | composition only; no invariant logic |

The small remaining `validate-*.sh` helpers are artifact validators, not predecessor suites: prepared browser-runtime integrity, packaged macOS-app integrity, and Mach-O deployment-target inspection.

## Configured CI

The workflow is [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). It runs on pushes to `main`, pull requests, and manual dispatch.

| CI job | Gates |
| --- | --- |
| Source and browser contracts | `qa-release.sh` once; pinned JS dependency install; dependency-backed ProseMirror/bridge behavior once |
| Native macOS | browser-runtime preparation; `cargo fmt`; one all-target/all-feature `cargo check`; one strict all-feature Clippy; one all-feature workspace test run; one release build; Mach-O deployment inspection |

The dependency-free JS files are owned by the concern suites. After `npm ci`, CI uses `run-browser-behavior-tests.mjs --dependency-backed-only` for only the two tests that require installed ProseMirror dependencies, avoiding the former second execution of the dependency-free security/editor/PDF contracts. Cargo check/Clippy/test/build remain exclusively in the native job rather than leaf source validators.

## Evidence ownership

- Editor grammar: `editor-spec/document-spec.json`, `validation-cases.json`, JS/Rust adapters and executable corpus validation.
- Security/privacy: all-feature Rust tests plus maintained JS publication, residue and profiling contracts; source assertions are limited to architecture/constants/owner traceability where runtime execution is not available in the Ubuntu source job.
- Backup format/key authority: implementation constants/parsers and Rust behavioral owners; historical Markdown is not an executable oracle.
- Native quality: hosted macOS all-feature compile/lint/test plus release build.
- OS boundaries: Keychain/Keystore and supported-device behavior still require native/manual integration evidence where mocks or source structure cannot establish real OS behavior.

Historical pass reports and removed validators remain available through immutable Git and [the historical evidence ledger](qa/HISTORICAL_EVIDENCE.md); they are provenance, not compatibility fixtures.

## Manual/adversarial gates before distribution

Native distribution still requires supported-device checks for Keychain creation/retrieval/denial/loss/corruption, privacy lock/unlock and stale publication under concurrency, interrupted filesystem operations and restart recovery, actual encrypted-backup persistence/restore once that cutover exists, and app-bundle testing across supported macOS/hardware. Signing/notarization remains a separate release concern.

## Q6 independent closure audit

Q6 treats the streamlined suites themselves as untrusted and reconstructs the current editor, E0–E3 security/privacy and backup requirements from the governing specifications plus production source. The durable audit is [Q6 independent closure](qa/QA_INDEPENDENT_CLOSURE_Q6.md).

The audit found and repaired one substantive current managed-backup defect: a normal attachment removal can leave a valid durably owned quarantine file, and the old snapshot/restore boundary could either reject the resulting safety snapshot or lose that quarantine owner at the whole-library SQLite handoff. Current snapshot copies now omit only positively validated settled quarantine ownership, while Restore strictly retires old-library quarantine before the database switch. Executable regressions prove exact note/attachment restore plus restart and separately prove that active managed plaintext staging still blocks snapshots.

Q6 status is intentionally scoped:

- current editor contract and runtime behavior: **PASS**;
- implemented E0–E3 security/privacy, ownership, publication, concurrency, crash/recovery and residue invariants: **PASS after Q6 remediation**;
- active managed-backup repository v2 snapshot/restore behavior: **PASS after Q6 remediation**;
- staged 2b.43 key/header/keyset/secure-store abstraction authority: **PASS as staged authority**;
- encrypted managed-backup repository v3 persistence/restore: **DEFERRED to 2b.43.4**;
- real macOS Keychain / Android Keystore device behavior and supported-device privacy behavior: **native/manual integration outstanding**.

A Q6 PASS therefore does not claim that the planned encrypted-v3 repository cutover or real-device secure-store integration has already occurred.


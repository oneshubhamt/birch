# Rustnote 2b.20 — Startup Identity, Test Isolation, and Privacy Ordering Closure Plan

## Audit findings

1. Deleting the default Documents library and backup folders does not delete the durable per-user `backup-location-history.json`. A later empty library at the same canonical data path generated a new library ID and was therefore rejected from the historically owned default backup path.
2. A failed bootstrap can leave only lock files, an empty attachments directory, and a newly generated identity before backup ownership validation fails. That shell must be recoverable without weakening cross-library ownership.
3. `cargo test --all-features` can exercise the library from the binary test target through `internal-test-support`; test configuration/output recovery metadata must not use the real per-user configuration directory.
4. Notebook-only portability undo incorrectly attempted an empty multi-note privacy transition.
5. A retained protected mutation could race a Lock that had already been admitted but had not yet published Vault revocation.
6. Note Lock/Unprotect/Delete drained derivative permits before publishing Vault revocation. Compound protected native resources therefore could not observe revocation while their sensitive lifetime was still active, contrary to the E3 contract.
7. One emergency-journal cleanup test created a plaintext record outside the durable managed journal namespace and therefore tested an invalid setup.

## Objectives

- Recover the historical library identity only for the same canonical data-directory path.
- Reconcile a mismatched identity only for a demonstrably pristine failed-bootstrap shell; never rewrite a non-pristine library.
- Preserve fail-closed cross-library backup ownership and pending privacy-reset history.
- Isolate unit/internal-test-support configuration and output recovery metadata from real user configuration.
- Make notebook-only undo valid without creating an empty note transition.
- Linearize retained protected mutations against privacy admission.
- Publish note-level Vault revocation before derivative drain for Lock/Unprotect/Delete, while retaining derivative-first whole-library restore ordering.
- Correct the invalid emergency-journal test fixture.
- Add a 2b.20 static closure guard and rerun inherited E0–E3/Vault validators plus dependency-free behavioral QA.

## Native closure required on macOS

The authoritative native gate remains Cargo format/check, strict Clippy, targeted regression tests, full tests, E0–E3 validators, and the 2b.20 guard. Native Rust execution is unavailable in the packaging sandbox and must not be claimed here.

# QA — Rustnote 2b.20

## Source/static closure

- Historical same-path identity recovery is guarded by canonical path matching.
- Failed-bootstrap identity repair is restricted to a pristine library shell.
- Non-pristine mismatched identities remain untouched and cross-library backup reuse remains rejected.
- Test/internal-support configuration is process-scoped and outside durable user configuration.
- Output recovery metadata uses the test namespace for unit/internal-support builds and the durable namespace for normal builds.
- Retained protected mutation checks privacy admission before storage serialization.
- Lock/Unprotect/Delete publish Vault revocation before derivative drain; whole-library restore remains derivative-first.
- Notebook-only portability rollback no longer asks the coordinator to admit an empty note set.
- Managed emergency-journal cleanup test uses an owned journal namespace.

## Sandbox QA

The 2b.20 targeted static guard, inherited Vault/E0–E3 guards, script syntax checks, source lexical checks, and available dependency-free editor/PDF behavioral tests are run during packaging. Cargo/rustc/rustfmt are unavailable in this sandbox, so native compilation and native unit-test success are explicitly not claimed.

## macOS native regression focus

The native rerun should first target the failures supplied from 2b.19, especially startup identity recovery, retained-mutation admission, protected command/export Lock lifetime, notebook-only undo, emergency-journal cleanup, derivative-vs-Protect blocking, and the remaining protection tests. Then run the complete serial suite to distinguish correctness from high-cost parallel KDF pressure, followed by the normal parallel suite.

## Added regression surface

Static closure now requires live Protect to scrub image-rendition and media-compatibility plaintext derivatives for every protected attachment hash, backed by the existing `protected_note_scrubs_rustnote_managed_plaintext_residue` native test. Native execution remains required on the Mac because this sandbox has no Rust toolchain.

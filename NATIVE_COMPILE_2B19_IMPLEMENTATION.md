# Rustnote 2b.19 — Test-target dead-code closure implementation

## Root cause
`cargo check --all-targets` compiles the library with `cfg(test)`. In that variant, `native::output::journal_root()` intentionally uses a test-only temporary directory, so the production durable-configuration helper and `OUTPUT_JOURNAL_NAMESPACE` are unreachable by design.

## Correction
- `app_paths::durable_configuration_directory` is now compiled only under `#[cfg(not(test))]`.
- Its `std::io` import carries the same production-only gate.
- `OUTPUT_JOURNAL_NAMESPACE` is now compiled only under `#[cfg(not(test))]`.
- Production `journal_root()` remains `#[cfg(not(test))]` and still resolves `durable_configuration_directory()?.join(OUTPUT_JOURNAL_NAMESPACE)`.
- Test `journal_root()` remains `#[cfg(test)]` and continues using a process-scoped temporary directory.

No `#[allow(dead_code)]` or broader warning suppression was introduced.

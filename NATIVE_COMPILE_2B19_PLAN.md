# Rustnote 2b.19 — Test-target dead-code closure plan

## Audit finding
Native `cargo check --workspace --all-targets --all-features --locked` on build 2b.18 completed successfully but emitted two `dead_code` warnings while compiling the `rustnote` library test target:

- `app_paths::durable_configuration_directory`;
- `native::output::OUTPUT_JOURNAL_NAMESPACE`.

Both symbols remain required by the production (`cfg(not(test))`) output recovery ledger. They are unused only in the unit-test variant because `native::output::journal_root()` deliberately substitutes a process-isolated OS-temporary ledger under `cfg(test)`.

## Objectives
1. Preserve the production crash-durable output journal path exactly.
2. Exclude the two production-only symbols from the unit-test compilation variant rather than deleting them or suppressing `dead_code`.
3. Gate the now production-only `std::io` import in `app_paths.rs` so the fix cannot create a replacement unused-import warning.
4. Add a targeted validator proving the production/test split and absence of warning suppression.
5. Preserve all inherited 2b.18 and E0–E3 closure validators.
6. Make no cryptographic, persistence-schema, authorization, output-transaction, or user-visible behavior change.

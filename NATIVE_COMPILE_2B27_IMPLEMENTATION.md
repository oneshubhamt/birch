# Rustnote 2b.27 — Main PDF lifetime closure implementation

## Changes

### `src/storage/mod.rs`

`protected_note_accepts_vault_reads_for_test` is now available under `cfg(any(test, feature = "internal-test-support"))`, is `#[doc(hidden)]`, and is public only so the `rustnote` binary test target can inspect the library's Vault admission state. It remains absent from ordinary production builds without internal-test-support.

### `src/attachment_protocol.rs`

Both protected PDF lifetime tests now observe Lock admission through `rustnote::storage::protected_note_accepts_vault_reads_for_test(...)` instead of starting a second protected attachment read through `transport.attachment_resource(...)`.

This removes the circular test wait while retaining the security contract:

- Lock publishes revocation;
- new protected reads are no longer admitted;
- Lock remains blocked while the already-issued response/OS-file lifetime owns its Vault lease;
- after that owner is dropped, Lock completes.

The post-release completion timeout is aligned with the existing lifetime regressions at 15 seconds.

## Production behavior

No production Lock, attachment transport, PDF I/O, encryption, or storage behavior was changed.

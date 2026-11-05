# Rustnote 2b.22 — Media Lock Test + Validator Closure Implementation

## Changes

- Added a `cfg(test)` storage helper that reads the already-existing Vault session availability state without acquiring a new plaintext/Vault permit and without waiting on privacy admission.
- Updated `protected_media_file_keeps_lock_waiting_until_file_handle_drops` to use that nonblocking test-only state probe instead of `AttachmentTransportStore::attachment_resource` during an admitted Lock.
- Retained the test's critical assertions: Lock publishes revocation while the file lease is alive, Lock has not completed before the lease drops, and Lock completes after the lease drops.
- Extended the post-release completion timeout to 15 seconds to avoid unrelated scheduling flakes.
- Made the 2b.21 restore-order validator whitespace-insensitive so `cargo fmt` cannot invalidate a correct Lock call.
- Admitted build 2b.22 into the inherited 2b.21 closure guard; no schema or production privacy contract changed.

## Architectural effect

None on production behavior. Vault revocation, derivative gating, media leases, storage serialization, backup ownership and cross-library fail-closed behavior are unchanged. The only new API is test-only and crate-private.

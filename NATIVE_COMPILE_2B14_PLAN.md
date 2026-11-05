# Rustnote 2b.14 — Editor Native Capture Closure Plan

## Audit finding
Native `cargo check` on build 2b.13 exposed four remaining Dioxus/Rust ownership errors in `src/ui/editor.rs`:

1. `PrivacyPublicationToken` was moved from reusable `FnMut` editor event handlers into spawned futures.
2. `ApplicationServices` was moved into the attachment-command handler and then needed later in the editor render tree.
3. `ApplicationServices` was moved into the revision-restore handler and then needed later in the editor render tree.
4. The packaged source also retained the `rustfmt` discrepancy reported by the native Mac toolchain in `attachment_protocol.rs`.

## Objectives
- Clone publication authorization per event invocation immediately before `spawn(async move { ... })` while retaining the reusable handler-level token for the synchronous publication gate.
- Give each consuming Dioxus handler its own `ApplicationServices` clone rather than moving the component-level service handle.
- Carry the native `rustfmt` correction into the packaged source.
- Make no crypto, storage-schema, Vault-format, privacy-policy, or feature behavior changes.
- Preserve all inherited 2b/E0–E3 invariants and add a static regression guard for these exact ownership patterns.

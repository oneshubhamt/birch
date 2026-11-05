# Rustnote 2b.14 — Editor Native Capture Closure Implementation

Build 2b.14 is a narrow native compile-closure pass over build 2b.13.

## Changes

### Reusable publication-token handlers
Both live editor `onchange` and `oncommit` handlers continue to capture one reusable `PrivacyPublicationToken` for the synchronous `is_publishable()` check. Each invocation now clones that token before transferring it into its spawned future. This matches Dioxus `EventHandler`'s `FnMut` lifetime and retains the existing publication-lease semantics.

### ApplicationServices ownership
The attachment-command handler and revision-restore handler each capture their own `ApplicationServices` clone. The editor component-level service handle therefore remains available to later panels and note-link picker rendering.

### Formatting
The native rustfmt expansion reported for the image-staging identity error in `src/attachment_protocol.rs` is carried into source.

## Scope
No cryptographic format, key policy, encrypted object schema, database schema, privacy invariant, or user-visible feature behavior was changed.

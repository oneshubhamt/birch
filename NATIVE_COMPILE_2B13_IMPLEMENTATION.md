# Rustnote 2b.13 — Native Compile Closure Implementation

## Changes

### Dioxus RSX correctness

- `NoteDocumentEditor` now supplies `key` through a Dioxus-formatted string backed by a precomputed `editor_key`; raw `format!(...)` is no longer used in the special `key` attribute.
- Backlink count is precomputed before RSX rather than embedding an inline `if` expression in a formatted text segment.

### Storage capability boundary

- `AttachmentFileCommandResource` is re-exported crate-wide from `storage` so the application layer can name the narrow resource without reaching into the private `capabilities` module.
- `StorageError` is imported where capability validation constructs it.
- `list_attachments_with_authorization` now belongs to `AttachmentStore`, the facade actually consumed by `AttachmentService`; the unused duplicate on `EditorStore` was removed.

### Privacy publication / lifetime ownership

- Top-level autosave publication uses the existing `list_generation` signal as its library-generation freshness input.
- `LiveEditor` clones its publication token per `FnMut` effect invocation before moving it into the async task.
- Revision preview captures a dedicated `preview_note_id` clone, preserving the component's original `note_id` for later controls.
- Attachment-panel Open and menu actions receive independent `PrivacyPublicationService` clones.

### Rust ownership and type fixes

- Revision-index contiguity validation explicitly types `previous_no` as `Option<i64>`.
- Restore-blob iteration accepts the `usize` count returned by `for_each_archive_blob` rather than matching `Ok(())`.
- Staged attachment mutability is placed only on the implementation that mutates ownership state.
- Backup-history normalization retains the normalized `data_directory` through the identity check with one deliberate clone.
- Signal generation reads are completed before subsequent mutable signal writes, avoiding overlapping Dioxus signal borrows.

### Security-preserving Debug implementation

`VaultPlaintextPath` implements `Debug` manually as `VaultPlaintextPath([REDACTED])`. The compiler-suggested `#[derive(Debug)]` was intentionally not used because it would expose the sensitive managed plaintext path through debug formatting.

### Dead-code / warning cleanup

- Removed unused attachment-finalization `note_id` and `request_id` state and corresponding dead call parameters.
- Removed unused production imports.
- Test-only plaintext lifecycle re-exports are now `#[cfg(test)]`.
- Test-only `Duration` import moved into the runtime test module.

## Security invariants

No encrypted-object format, key hierarchy, PrivacyCoordinator ordering, OwnedPlaintext lifecycle, publication-token semantics, backup privacy behavior, or AEGIS/Argon2 policy was changed in this pass.

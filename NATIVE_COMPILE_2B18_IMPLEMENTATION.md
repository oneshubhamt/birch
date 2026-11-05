# Rustnote 2b.18 — Strict Clippy compile-closure implementation

## Compile correctness
- The granular protected-revision test now clones only `NoteId` before `save_note_checkpoint_owned` consumes the `Note` and uses that identifier for subsequent graph/SQL checks.
- `Storage::lock_protected_note` remains `pub(crate)`. Binary transport lifetime tests call a `#[cfg(any(test, feature = "internal-test-support"))]`, `#[doc(hidden)]` façade instead of widening the production mutation API.

## Signature consolidation
- Authorized derived-image attachment ingress uses `AuthorizedDerivedImageAttachment` and a private `AuthorizedAttachmentWrite` request with explicit direct/derived origin.
- Protected attachment blob sealing uses `ProtectedBlobInput`.
- Image import and revision restore use `ImageImportFlow` / `RevisionRestoreFlow` contexts.
- Right-panel attachment commands use `AttachmentActionContext`.
- Native close durability uses `CloseDurabilityRequest`.

These are data-boundary consolidations only; the existing operation order and authorization lifetimes are preserved.

## Strict Clippy cleanup
The exact reported warning classes were removed structurally rather than silenced: collapsible conditionals, needless borrows, explicit auto-deref, `question_mark`, `unnecessary_lazy_evaluations`, `needless_return`, and ambiguous `OpenOptions::create(true)` truncation behavior.

No blanket Clippy allowances were added.

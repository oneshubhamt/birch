# Rustnote Vault 2b.10 — Cross-cutting Invariant Closure

## Scope

This pass closes three independent whole-contract findings discovered by the post-2b.9 de-novo audit. It does not redesign the Vault object store, PrivacyCoordinator, encrypted object format, FTS architecture, backup repository, editor engine, or attachment encryption.

## Objective 1 — Fail-closed durable namespace establishment

`DurablePlaintextNamespace::establish()` now distinguishes fresh establishment from post-hoc adoption.

```text
marker absent + root absent/empty
→ publish durable authority

marker absent + root nonempty
→ fail closed

valid marker present
→ reopen normally

pre-existing nonempty root with independent proof
→ adopt_existing_with_proof()
```

The exceptional adoption API is intentionally explicit. There is exactly one production caller: the bounded one-way adoption of a 2b.8 automatic-backup repository after its authenticated inline owner and library identity have been verified. Emergency-journal, media-compatibility, and image-rendition namespaces remain on ordinary fail-closed establishment and cannot use the exceptional path.

Emergency-journal adversarial fixtures were corrected so their setup itself establishes namespace ownership before test plaintext is created.

## Objective 2 — Token-bound global protected status lifetime

`NoteEditor` now classifies status publication once at its outer boundary. Ordinary notes retain the lightweight existing `onstatus` path. An unlocked protected note captures a note-scoped `PrivacyPublicationToken` and publishes through `ProtectedStatusPublisher`.

```text
protected editor status
→ capture fixed UI/library/note scope
→ capture current coordinator authorization
→ begin canonical publication lease
→ publish global toast
→ retain token with toast
```

`ToastMessage` therefore carries an optional publication token. Protected toast rendering and retained state both recheck that token. `PrivacyPublicationToken::observe_publishable()` subscribes Dioxus to UI privacy, library, and note-generation changes while also checking coordinator authorization.

Lock/Protect and other Vault visibility transitions begin the UI privacy revocation epoch before the authoritative operation. In addition, every privacy-generation change clears any existing global toast, including a toast that was originally published while a note was ordinary and was subsequently reclassified by Protect. This closes both the original protected-toast-after-Lock defect and the ordinary-to-protected reclassification case.

No child editor surface needs to know how global privacy generations compose; all status emitted through `NoteEditor` uses the central boundary.

## Objective 3 — Protected sync-queue prohibition at one storage boundary

Under `remote-sync`, both `enqueue_sync_upsert()` and `enqueue_sync_delete()` now call `sync_payload_allowed()` before constructing JSON.

For note entities, the boundary checks the canonical Vault root. For attachment entities it checks both a current attachment row and a durable attachment-operation row, so recovery of an ordinary committed remove can still emit its tombstone after the attachment row has disappeared.

If the entity is protected, the boundary:

```text
classify protected
→ delete any existing queue rows for that entity
→ return without serializing payload
```

There is still exactly one `INSERT INTO sync_queue` implementation, private behind the guarded enqueue functions. Protection's existing scrub of pre-existing note/attachment sync rows remains in place.

All-features Rust tests now cover both sides of the boundary:

- protected note/attachment upsert/delete attempts produce no queue rows;
- ordinary note/attachment upsert/delete remains enabled.

## Objective verification

1. Fresh namespace establishment refuses post-hoc adoption of a nonempty root — complete.
2. Explicit proven adoption is a separate API and has one production caller — complete.
3. All unlocked-protected-editor status uses a note-scoped publication token — complete.
4. Protected global toast lifetime is token-bound — complete.
5. Any privacy transition clears pre-existing global toast state — complete.
6. `is_publishable()` preflight cannot be followed by an unleased protected-editor global status handoff — complete at the central boundary.
7. Protected note/attachment sync payloads are rejected before serialization — complete.
8. Ordinary sync behavior has explicit non-regression coverage — complete.
9. Inherited security/Vault and architecture/static gates remain green in every runnable mode — complete.

## Environment limitation

The available sandbox contains no `cargo`, `rustc`, or `rustfmt`. Native Rust tests and the full all-features compile/clippy/test release gate cannot be executed here. They remain mandatory on the exact packaged source.

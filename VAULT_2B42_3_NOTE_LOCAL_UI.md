# Vault 2b.42.3 — Note-local UI privacy lifetime

## Objective

Make UI surfaces whose sensitive plaintext belongs to one note depend only on that note's privacy generation (plus the library generation), never the aggregate notes-list lifecycle merely because they render inside the notes application.

## Implemented

- `use_note_privacy` is the reactive authority for one-note surfaces.
- Note Properties reloads/clears through the note-local privacy memo and a note-local publication token.
- Current-note Sources/citations clear decrypted summaries and note-derived composer state on same-note revocation; their local load is note-authorized. Cross-note source candidate search remains explicitly multi-note for 2b.42.4.
- Revision preview plaintext cache is erased on same-note/library revocation and preview publication remains note-authorized.
- The live editor has an immediate same-note redaction path: body/title/tags/attachment names are replaced with a non-secret transition shell, attachment/link registries and note-derived popover strings are cleared, and the WebView editor is disposed.
- Attachment panel/action authority is note-local; the unused aggregate lifecycle field was removed. The application-layer attachment resource already retains the compound plaintext/Vault permit across native dialogs/handoff.
- Current-note metadata note/PDF export and note-local tag removal remain note-authorized. Aggregate tag-catalog mutation remains multi-note and is intentionally deferred to 2b.42.4.

## Explicit non-goals

This pass does not migrate backlinks, hierarchy, note picker, link-target registry, global source candidate search, tag catalog, sidebar/list caches, Vault action orchestration, backup/export/Restore, or performance behavior. Those belong to later 2b.42 subpasses.

The outer `selected_note` ownership transition during Lock/Protect/Unlock/Remove Encryption remains the responsibility of 2b.42.5. This pass ensures note-local child caches and the live editor fail closed as soon as that note's UI privacy epoch is revoked.

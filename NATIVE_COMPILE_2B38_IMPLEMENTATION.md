# Rustnote 2b.38 — Vault action lifecycle closure implementation

The note-card action workflow now uses `dioxus::dioxus_core::spawn_forever` instead of component-scoped `spawn`.

This is intentionally one lifecycle change, not a storage/security rewrite. The workflow only captures `ApplicationServices`, `NotesListState` App-owned signals and owned action data. It therefore remains valid for the root scope while allowing the initiating `LockedNotePane`, note editor, or note card to unmount.

The existing Vault sequence remains unchanged:

- synchronize when required;
- install non-secret transition state;
- begin UI privacy revocation;
- remove decrypted UI state for Protect/Lock;
- execute canonical Vault/storage operation;
- end revocation;
- capture fresh post-transition publication authorization;
- reread canonical note;
- republish selected note when appropriate;
- refresh note-list cache;
- clear transition state.

No cryptography, key handling, Argon2 parameters, Vault object model, database schema, autosave security policy, FTS policy, attachment protection or backup privacy behavior changed.

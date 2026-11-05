# Rustnote 2b.41 — UI Task Scope Ownership Implementation

## Implementation

`RunningApp` captures its own Dioxus `ScopeId` and stores it in `NotesListState` as `task_scope`.

`NotesListState::spawn` calls `Runtime::current().spawn(task_scope, future)`. This gives short-lived UI workflows exactly the same lifetime as the Signals they operate on:

- child card/editor remounts do not cancel them;
- replacing/unmounting `RunningApp` does cancel them;
- no component-owned Signal is read or written from Dioxus's root scope.

The five previous `spawn_forever` sites were migrated:

- Vault/note-card actions;
- note selection;
- note creation;
- library-section switching;
- sidebar navigation.

No storage, cryptography, Vault object, autosave, database, attachment, FTS, or PrivacyCoordinator logic changed.

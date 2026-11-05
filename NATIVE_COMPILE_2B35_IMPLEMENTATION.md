# Rustnote 2b.35 — implementation

- Replaced post-action `selected_note = None -> yield -> selected_note = Some(note)` remount choreography with direct canonical republishing under a fresh publication lease.
- Added `VaultTransitionUiState`, containing only a stable note ID, a non-secret label, and progress text.
- Protect/Lock still revoke and remove decrypted content before the secure transition, but the editor now renders a privacy-safe transition pane instead of `Select a note`.
- The note list keeps the transitioning note visually selected by falling back to the transition note ID when decrypted `selected_note` is intentionally absent.
- Encrypt uses the user-approved locked-note public label in the transition pane. Lock deliberately uses the generic `Encrypted note` label because the selected unlocked note carries the private decrypted title, not the public label.
- Transition state is cleared on successful canonical republish and on error/authorization exit paths.
- `note_selection_generation` is part of both unlocked-editor and locked-pane component keys, providing an explicit clean Dioxus remount without a post-operation blank frame.
- Added explicit Vault progress messages: Encrypting, Unlocking, Locking, and Removing encryption.
- Kept the existing Argon2id profile, forensic SQLite/FTS/WAL cleanup, backup privacy cleanup, Vault transition authority, autosave barriers, and encrypted object model unchanged.
- Build incremented to 2b.35.

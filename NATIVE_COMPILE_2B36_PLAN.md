# Rustnote 2b.36 plan

- Separate selection-request generation from editor-view remount generation.
- Remove Vault self-invalidating publication/remount sequence and nested publication lease.
- Preserve Protect/Lock plaintext revocation while keeping logical selection through non-secret transition UI.
- Replace the Unlock password pane with transition UI during authentication.
- Stabilize editor session capture before synchronization after a remount.
- Preserve the user-approved public locked-note label in note-list summaries while still scrubbing private title/body/tags/attachment names.
- Add targeted regression guards and run inherited Vault/E0–E3 closure.

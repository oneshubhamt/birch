# Native Compile 2b.40 Implementation

## Selected-note Vault routing
`apply_note_card_action` now routes Encrypt, Unlock, Lock, and Remove Encryption through the existing `select_note` workflow whenever the target is not already selected. This gives every Vault operation one UI ownership model: checkpoint the current editor, load the target canonically, then perform the security mutation on the selected target.

The callback re-enters the action workflow with `target_checkpointed_by_selection = true`. That immediate routed action skips a second editor checkpoint because `EditorService::switch_note` already flushed pending drafts and loaded canonical target state. Existing already-selected actions still synchronize the live editor normally, preserving last-second edits.

The root-scoped action rechecks that the target is still selected before admitting a Vault mutation. If a user selection wins before the task is polled, the action cancels rather than falling back to the removed cross-note path.

## Dead-code cleanup
The unused `UiPrivacyEpoch::capture_selection_publication` method was removed. Semantic library/note publication composition remains owned by `capture_library_publication`, `note_publication_scope`, and `capture_note_publication`.

## Security scope
No changes were made to encryption, password derivation, Vault keys, PrivacyCoordinator admission/revocation/drain ordering, autosave barriers, protected storage, FTS/WAL cleanup, attachments, backups, or database schema.

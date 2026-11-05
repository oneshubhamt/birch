# Rustnote 2b.38 — Vault action lifecycle closure plan

## Audit finding

`apply_note_card_action` used Dioxus component-scoped `spawn`. Vault actions deliberately replace or remove the UI component that initiated them: Unlock swaps `LockedNotePane` for the secure transition shell, Protect/Lock remove the decrypted editor surface, and Delete can remove its own note card. Dioxus cancels component-scoped tasks when their owner unmounts, while `ApplicationExecutor::read` uses detached Tokio work that can continue. The result is a split-brain completion: canonical storage mutation succeeds, but UI reconciliation (canonical reread, selected-note republish, note-list refresh and transition cleanup) can be cancelled.

## Objectives

1. Make the note-action workflow survive intentional initiator unmounts.
2. Preserve all existing privacy revocation and plaintext-removal boundaries.
3. Guarantee post-Vault canonical reread and selected-note republish can complete after Unlock/Lock/Protect/Unprotect.
4. Guarantee note-list cache refresh and transition cleanup run after the canonical mutation.
5. Keep all state used by the detached UI workflow rooted in `App`-owned signals.
6. Add regression guards for lifecycle ownership and completion order.
7. Re-run targeted UI/browser and inherited Vault/E0–E3 closure checks.

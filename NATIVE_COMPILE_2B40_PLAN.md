# Native Compile 2b.40 Plan

## Audit finding
Cross-note Vault actions used a second durability path: with note A selected and note B targeted, Rustnote synchronized A and then separately checkpointed B before beginning the Vault transition. That target checkpoint can drain the autosave queue again while `vault_action_busy` defers navigation. The selected-note path does not need this cross-note choreography.

The same audit found `UiPrivacyEpoch::capture_selection_publication` is no longer referenced after publication-scope consolidation and therefore fails strict Clippy as dead code.

## Objectives
1. Make Encrypt, Unlock, Lock, and Remove Encryption selected-note-only UI actions.
2. When a Vault action is invoked from another card, first use the canonical note-selection/checkpoint workflow to select the target.
3. Do not immediately checkpoint the newly selected target again; the selection boundary already flushed pending work and loaded canonical state, and waiting for a just-mounted editor would be redundant.
4. Preserve the normal live-editor checkpoint when the target was already selected before the action.
5. Cancel rather than resurrect a cross-note Vault mutation if selection changes before the root-scoped action is polled.
6. Remove the obsolete `capture_selection_publication` helper instead of suppressing dead-code warnings.
7. Keep Vault cryptography, PrivacyCoordinator transition semantics, storage formats, and plaintext-revocation behavior unchanged.

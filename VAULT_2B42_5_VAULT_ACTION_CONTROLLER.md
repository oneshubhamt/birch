# Vault 2b.42.5 — Vault-action controller

## Objective

Make Protect, Lock, Unlock and Remove Encryption operate on the target note's privacy scope without globally blocking navigation or forcing an unrelated selected note through the target's transition. The Vault controller must preserve the target note's durability boundary, fail closed for target plaintext UI, publish canonical post-transition state only under fresh target authority, and leave aggregate recovery to the multi-note privacy lifetime established in 2b.42.4.

## Implemented architecture

### Selected versus unselected target

The controller no longer routes an action on unselected B through ordinary A -> B selection.

- If B is selected, `synchronize_for_note_mutation` checkpoints the live B editor with `CheckpointSilent` under B's note-local publication token.
- If B is not selected, it silently checkpoints B directly under B authority. Unrelated selected A is neither synchronized nor replaced.
- After an unselected-B checkpoint completes, the synchronization barrier resamples selection. If B acquired a live editor while that checkpoint was in flight, the barrier loops through the live-editor `CheckpointSilent` path before it may return. This closes the durability half of the unselected -> selected race; fail-closed redaction alone is not considered sufficient.
- The barrier returns `NoteMutationSynchronization { selection_generation, target_selected }`, captured at the same no-await boundary as the completed checkpoint. The Vault controller consumes that exact snapshot rather than resampling independently.
- Unlock skips the plaintext editor checkpoint because a locked note has no editable decrypted draft. Delete Permanently retains its pre-existing no-editor-checkpoint behavior.

`vault_action_busy` remains `Signal<Option<NoteId>>`. It serializes Vault visibility operations, but ordinary navigation and note creation do not poll it as a global busy flag.

### Post-audit durability repair

A de-novo audit after 2b.42.6 found one race in the original 2b.42.5 checkpoint: an action could start on unselected B, B could become selected and edited while its silent checkpoint was running, and the controller would redact B without first checkpointing the newer live editor.

The repair moves ownership of the selection decision into `synchronize_for_note_mutation`. The function now loops until the target is stable at a checkpoint boundary:

```text
unselected B -> silent B checkpoint -> B still unselected -> return boundary snapshot
unselected B -> silent B checkpoint -> B became selected -> live B checkpoint -> return boundary snapshot
selected B   -> live B checkpoint -> B still selected -> return boundary snapshot
selected B   -> live B checkpoint -> selection changed -> re-evaluate target
```

There is no await between the returned boundary snapshot and B UI revocation. Therefore a newly live B editor cannot be skipped merely because the action began while B was unselected.

### Transition ordering

For a Vault visibility action on B the controller follows:

```text
capture B authority
-> checkpoint/synchronize B when required
-> install a non-secret selected-B transition shell when B is selected
-> begin B UI revocation
-> remove selected B plaintext from UI when applicable
-> execute the storage/Vault operation
-> close B UI revocation
-> acquire fresh B authority
-> canonical reread of B
-> acquire B publication lease
-> republish B only if the original selected-B context is still current
```

Storage remains the low-level transition admission authority through `NotePrivacyTransition::{Protect, Unlock, Lock, Unprotect}`.

### Fail-closed selected-B UI

For selected Protect/Lock/Unlock/Remove Encryption, the controller installs `VaultTransitionUiState` before plaintext revocation and removes the selected `Note` object for the transition lifetime.

Protect may display the explicitly user-approved public label. Lock, Unlock and Remove Encryption use the non-secret fallback `Encrypted note`; they never copy the currently decrypted private title into transition UI.

The 2b.42.3 note-local privacy lifetime independently clears editor/panel plaintext on B revocation, while the 2b.42.4 multi-note lifetime clears aggregate B-derived state.

### Fresh canonical B publication

Post-transition publication does not reuse the pre-action token and does not reread first and authorize later. `read_canonical_note_for_vault_publication` loops through:

```text
fresh B publication token
-> canonical B reread
-> token freshness recheck
-> B publication lease
```

The returned lease stays alive across the selected-editor handoff.

The same canonical reconciliation is attempted on the error path because a Vault transition may have committed its canonical state before a later cleanup operation reports an error.

### Anti-resurrection selection rule

Completion may republish B into the selected editor only when all of the following remain true:

- B is selected when the UI revocation boundary opens;
- the note-selection generation captured at that revocation boundary is unchanged;
- the active transition shell still belongs to B.

Therefore an action started on unselected B never injects B into editor A, and a selected-B action cannot resurrect B after the user navigates to C.

### Navigation/create during a selected-B transition

A selected-B Vault action checkpoints B before opening its fail-closed UI revocation. During that window `selected_note` is intentionally empty and the aggregate privacy generation is revoked. Falling back to the ordinary "no selected note" multi-note flush would therefore wait on the transition being escaped.

`selected_source_checkpointed_by_active_vault_transition` records the narrow condition that the selected Vault source has already crossed its durability boundary. `select_note` and `create_contextual_note` may skip only that redundant source synchronization, then acquire independent target-note authority as established by 2b.42.2.

For Create Note, new C is selected and its title field is focused before the later aggregate list refresh wait, so C remains usable while B's aggregate revocation is still active.

### Aggregate recovery

The Vault controller does not perform its own list/tag refresh. Closing B's UI revocation wakes the single aggregate privacy-lifetime effect introduced in 2b.42.4. That effect waits for fresh multi-note authority, acquires a publication lease, refreshes the tag catalog and reloads the note list.

Keeping aggregate recovery centralized avoids duplicate requests and keeps B-local canonical publication independent from sidebar/list availability.

### Authentication and failure reconciliation

Wrong-password messages remain action-specific:

- Unlock: `Incorrect password. The note remains locked.`
- Remove Encryption: `Incorrect password. Encryption was not removed.`

Regardless of failure type, the controller clears the transition shell and, for Vault actions, attempts a canonical B reread under fresh B authority so post-commit cleanup failures cannot leave stale plaintext visible.

## Explicit non-goals

This pass does not redesign automatic backup, complete archive export, shutdown `checkpoint_all`, library Restore, or other whole-library operations. Those remain 2b.42.6.

It also does not add profiling or alter SQLite plaintext scrubbing/VACUUM behavior; those remain 2b.42.7 and 2b.42.8.

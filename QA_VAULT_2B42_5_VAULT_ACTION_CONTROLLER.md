# QA — Vault 2b.42.5 Vault-action controller

## Required invariants

1. Protect/Lock/Unlock/Remove Encryption act directly on target B; an action on unselected B does not force A -> B selection.
2. Selected B is live-editor checkpointed before revocation; unselected B is silently checkpointed without pulling unrelated A into B's privacy scope. If B becomes selected while the silent checkpoint is in flight, the durability barrier must loop through a live-editor checkpoint before returning the exact revocation-boundary selection snapshot.
3. `vault_action_busy` identifies the active target note and is not a navigation/create blockade.
4. B UI revocation begins before the storage transition and invalidates aggregate B-derived state while leaving unrelated A note-local authority valid.
5. Selected B plaintext is absent during the transition; transition UI contains only a public/non-secret label.
6. Post-transition canonical B is authorized before reread and is published under a B lease.
7. Completion cannot resurrect B after navigation to C and cannot inject unselected B into editor A.
8. Selected-B navigation/create can proceed after B's checkpoint without waiting for B's aggregate revocation to close.
9. Aggregate list/tag recovery is owned exactly once by the centralized 2b.42.4 multi-note lifetime.
10. Wrong-password and post-commit error paths reconcile canonical B without stale plaintext publication.

## Dedicated executable contract

`tests/js/vault-action-privacy-contract.test.mjs` checks the invariants above, including ordering and absence of the superseded forced-selection helpers.

`editor-ui-contract.test.mjs` was updated to assert the scoped controller instead of the historical global publication/forced-selection architecture.


## Post-audit repair coverage

The repaired contract specifically requires the previously missing durability behavior:

```text
unselected B silent checkpoint
-> B becomes selected
-> retry through live-editor checkpoint
-> return exact selection-generation/selected snapshot
-> no-await B UI revocation
```

The inherited `editor-ui-contract.test.mjs` was strengthened to require this upgrade loop and consumption of the returned `NoteMutationSynchronization` boundary rather than merely checking that selection was resampled after a silent checkpoint.

## Observed assistant-side QA

- `validate-vault-2b42-1-scoped-publication-core.sh`: PASS.
- `validate-vault-2b42-2-navigation-source-target.sh`: PASS.
- `validate-vault-2b42-3-note-local-ui.sh`: PASS.
- `validate-vault-2b42-4-multi-note-ui.sh`: PASS.
- `validate-vault-2b42-5-vault-action-controller.sh`: PASS.
- Dedicated Vault-action JS contract: 11/11 PASS.
- Dependency-free browser/PDF runner after the post-audit repair: 85/85 PASS.
- `validate-vault-2b1-foundation.sh`: PASS.
- `validate-vault-2b4-granular-cutover.sh`: PASS.
- `validate-vault-2b5-consolidation.sh`: PASS.
- `validate-vault-2b6-independent-closure.sh`: PASS.
- `validate-vault-2b8-plaintext-authority-closure.sh`: PASS.
- `validate-vault-2b9-backup-ownership-closure.sh`: PASS.
- `validate-vault-2b11-atomic-privacy-authority-closure.sh`: PASS.
- `validate-vault-2b12-aegis-privacy-disclosure-closure.sh`: PASS when invoked through `bash` in this environment.
- `validate-security-e0-e3-ownership-2a16.sh`: PASS.
- `validate-security-e0-e3-residue-2a17.sh`: PASS.
- Shell syntax: 81/81 scripts PASS under `bash -n`.
- JS/MJS syntax: 49/49 PASS under `node --check`.
- Changed Rust delimiter/lexical structural audit: 4/4 PASS (`ui/mod.rs`, `ui/note_card.rs`, `ui/note_list_data.rs`, `ui/notes_header.rs`).

Native Rust formatting/compile/Clippy/tests are not executable in the assistant environment because the Rust toolchain is unavailable. They remain mandatory before native acceptance.

## Historical validator interpretation

Some later pre-2b.42 validators are exact-text guards for the architecture intentionally removed here and must not be made green by restoring it:

- 2b.35/2b.36 expect old selected-target/global redaction shapes such as `if locking_note || protecting_note` and controller-local `redact_protected_note`.
- 2b.38 expects controller-local synchronous list refresh between canonical reread and transition clear; aggregate recovery is now centrally owned by 2b.42.4.
- 2b.40/2b.41 require `vault_action_requires_selected_target`, `apply_note_card_action_with_selection`, and `target_checkpointed_by_selection`, all deliberately removed so unselected B no longer forces A -> B navigation.

The 2b.42.5 validator is the successor guard for these controller invariants.

## Targeted manual matrix

- Selected B: Protect, Lock, Unlock and Remove Encryption; verify plaintext disappears for the full transition and canonical B returns after success/failure as appropriate.
- A selected -> start Vault action on B; verify A remains selected/usable and B is never injected into A's editor.
- Selected B -> start action -> navigate to C; verify C opens and B does not reappear when the action completes.
- Selected B -> start action -> create C; verify C becomes selected/focusable before aggregate list recovery and B does not resurrect.
- Rapid repeated Lock/Unlock attempts; verify only one Vault action owns the busy guard and UI settles to canonical storage state.
- Wrong Unlock password; verify locked state and exact non-sensitive error message.
- Wrong Remove Encryption password; verify encryption remains and exact non-sensitive error message.
- Exercise a failure after the storage transition has committed (where injectable test support permits); verify canonical storage wins over stale pre-transition UI.

## Native gate

On macOS after installing this checkpoint:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1

./scripts/validate-vault-2b42-1-scoped-publication-core.sh
./scripts/validate-vault-2b42-2-navigation-source-target.sh
./scripts/validate-vault-2b42-3-note-local-ui.sh
./scripts/validate-vault-2b42-4-multi-note-ui.sh
./scripts/validate-vault-2b42-5-vault-action-controller.sh

node --test tests/js/vault-action-privacy-contract.test.mjs
```

After the prepared editor/PDF runtime is available, also run:

```bash
./scripts/prepare-rustnote-mac.sh
./scripts/validate-prepared-runtime.sh
node scripts/run-browser-behavior-tests.mjs --require-editor-deps
```

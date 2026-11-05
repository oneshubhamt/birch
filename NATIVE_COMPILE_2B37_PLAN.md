# Rustnote 2b.37 plan

- Audit both native warnings introduced by the 2b.36 session/selection refactor.
- Remove only references proven obsolete: the unused `PrivacyPublicationToken` import in `ui/note_card.rs` and the unused local `note_selection_generation` destructuring in `ui/editor.rs`.
- Preserve the authoritative `library_state.note_selection_generation` flow and all 2b.36 Vault/session behavior unchanged.
- Do not use warning suppressions or dummy `_` bindings.
- Extend inherited validators so 2b.37 is evaluated under the same architecture/schema/security expectations as 2b.36.
- Add a 2b.37 closure guard preventing either dead reference from returning.
- Run targeted UI/browser QA, inherited Vault/E0–E3 checks, shell syntax checks, and final diff review.

# Rustnote 2b.32 — Vault Session Query Compile Closure Implementation

## Implemented

- `VaultRuntime::note_session_is_active` is now the always-compiled semantic session query. It returns true only when the note session exists and is not revoking.
- The existing cfg-gated `contains_note_key` helper remains available for tests/internal test support and delegates to the semantic query.
- `PrivacyCoordinator::note_session_is_active` exposes the same production semantic through the established privacy authority. Its cfg-gated `contains_note_key` compatibility helper delegates to it.
- `Storage::note_autosave_mutation_is_blocked` now uses `note_session_is_active` rather than the test-only helper.
- `build-id.txt` is incremented from 2b.31 to 2b.32.
- Inherited validator allowlists are extended through 2b.32; their substantive assertions are unchanged.
- Added `validate-vault-2b32-native-full-qa-closure.sh`, which checks this production/test boundary and then runs the inherited 2b.31 closure chain.

No database migration, encryption-format change, or product-behavior expansion is part of this pass.

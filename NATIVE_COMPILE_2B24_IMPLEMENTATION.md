# Rustnote 2b.24 — Tag Mutation + Transport Lifetime Closure Implementation

## Production correction

- Added `TagMutationGuard`, which owns either the ordinary storage mutex or the complete protected-note mutation authorization set.
- `begin_all_protected_notes_tag_mutation` now takes the ordinary storage mutex when there are no protected notes instead of making an invalid empty multi-note Vault authorization request.
- After acquiring storage authority, the helper re-reads the protected-note set. If it changed, it drops the guard and retries. This preserves linearization against a concurrent Protect/Unprotect transition.
- When protected notes exist, the existing complete-set Vault authorization path remains unchanged and still fails closed if any protected note is locked.

## Test-lifetime correction

- `protected_pdf_transport_resource_keeps_lock_waiting_until_request_releases_it` now observes Lock revocation using `protected_note_accepts_vault_reads_for_test`, the same nonblocking test-only Vault-state inspection already used by the media-server lifetime regression.
- The live protected transport resource is still retained until after the test proves Lock has not completed.
- Completion after resource release uses the same 15-second debug-build allowance as the other Argon2-backed lifetime regressions.

## Diagnostic refinement

- Added descriptive assertion messages to `protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only` so a targeted native failure identifies the precise graph/locality invariant.
- No production source/citation/PDF-annotation behavior was changed without native failure detail.

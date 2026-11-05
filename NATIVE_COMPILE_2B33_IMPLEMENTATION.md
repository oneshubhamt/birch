# Rustnote 2b.33 — implementation

- Added `#[cfg(test)]` to `Storage::protect_note_with_password`.
- Updated `protected_transport_test_fixture` to call `protect_note_with_password_and_public_label(..., "Locked note")` directly, so `internal-test-support` remains valid under `--all-features`.
- Added no `allow(dead_code)` attributes.
- No cryptography, Vault format, schema, search, attachment, editor, or production protection-flow behavior changed.
- Updated build-id allowlists for 2b.33 and added a dedicated dead-code closure guard inheriting 2b.32.

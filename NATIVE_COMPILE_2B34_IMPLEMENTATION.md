# Rustnote 2b.34 — implementation

- Changed `VaultRuntime::contains_note_key` from `#[cfg(any(test, feature = "internal-test-support"))]` to `#[cfg(test)]`.
- Kept `VaultRuntime::note_session_is_active` as the production semantic query.
- Kept `PrivacyCoordinator::contains_note_key` available to `internal-test-support`; it delegates to `PrivacyCoordinator::note_session_is_active` and does not require the raw runtime helper.
- Updated inherited static validators so build 2b.34 is recognized and the runtime helper's narrower configuration is explicitly enforced.
- Added no dead-code suppression attributes.
- No cryptography, Vault format, database schema, search, attachment, editor, autosave policy, navigation behavior, or public locked-note label behavior changed.

# Rustnote 2b.17 — Native test-compile closure implementation

- Restart/ownership tests now clone their reusable `PathBuf`s when calling `Storage::at_with_backup`, matching its owning API without changing production signatures.
- `Storage` exposes a `#[cfg(test)] pub(super)` sibling-test façade for the private revision-status save helper. The production helper itself remains private.
- The stale protection test now uses `PrivacyCoordinator::allocate_vault_plaintext_path`, the current coordinator API.
- Added the zero-dependency `internal-test-support` Cargo feature. The protected transport fixture is exported only for library tests or that feature; binary PDF lifetime tests are feature-gated and call it through `rustnote::storage`, the actual library crate boundary.
- No cryptographic format, Vault object schema, persistence schema, privacy ordering, or product behavior changed.

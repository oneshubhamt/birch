# Rustnote 2b.20 — Implementation Record

## Startup identity continuity

`ensure_library_identity` now performs identity selection while holding the durable backup-history lock. If backup history contains exactly one non-empty library identity for the same canonical data-directory path, a recreated empty library reuses that historical identity instead of manufacturing a conflicting new one.

A prior failed bootstrap may already have written a fresh identity. Rustnote rewrites that identity only when the directory is a narrow bootstrap shell: identity/lock files plus an optional empty attachments directory, with no database or other library state. A non-pristine library retains its own identity and therefore still fails cross-library backup ownership validation.

No backup-history entry is deleted, transferred, or forgotten. Pending privacy-reset obligations remain associated with the historical identity.

## Test isolation

`configuration_directory` uses a process-scoped OS-temporary namespace under `cfg(test)` and the existing `internal-test-support` feature. The crash-output journal follows the same rule. Normal builds retain the durable platform configuration path.

This prevents unit tests and binary/internal-support tests from writing `storage-locations.json`, `backup-location-history.json`, or output transaction ledgers into the user's real Rustnote configuration directory.

## Privacy authority

Note Lock/Unprotect/Delete now publish/drain Vault revocation before draining plaintext derivatives. Protect still drains derivatives without Vault revocation. Whole-library restore remains derivative-first, preserving the E3 database-replacement invariant.

Retained protected mutation now holds admission while checking the retained permit and acquiring storage serialization. If Lock/library transition admitted first, the mutation fails closed immediately. If the mutation linearizes first, the later transition waits normally.

## Additional regression repairs

- Notebook-only import undo skips the multi-note privacy transition when there are no imported notes.
- The successful emergency-journal protected cleanup test now establishes the durable journal plaintext namespace before creating its managed record.
- 2b.20 validator/version admission was added without changing the Vault schema or encryption format.

## Live protection residue closure

The native failure `protected_note_scrubs_rustnote_managed_plaintext_residue` exposed a real asymmetry: restart recovery cleared persistent image-rendition and media-compatibility namespaces, while the live Protect scrub only handled quarantine, staging, handoffs, backups and SQLite. `scrub_managed_plaintext_residue_after_hashes` now performs strict hash-targeted cleanup of both persistent derivative caches before the remaining residue scrub. Unrelated cache identities are preserved and any cleanup failure keeps protection fail-closed.

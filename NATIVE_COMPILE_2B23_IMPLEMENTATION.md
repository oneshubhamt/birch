# Rustnote 2b.23 — Protected Attachment Roundtrip Lifetime Closure Implementation

## Changes

- Updated `protected_note_encrypts_attachment_blob_and_restores_it_losslessly` so the protected attachment resource is explicitly dropped after its materialized bytes are verified and before `remove_note_protection_with_password` begins.
- Added an explanatory comment documenting that the resource intentionally owns a `VaultReadPermit` for the lifetime of materialized plaintext.
- Added a 2b.23 validator that requires the resource drop to occur before Unprotect and rejects any protected-resource rematerialization between those operations.
- Admitted build 2b.23 into inherited validators. No storage schema, encryption format, Vault transition ordering, transport behavior, backup ownership, or production authorization behavior changed.

## Architectural effect

None on production behavior. The test now respects the production lifetime contract it is intended to verify: a protected plaintext resource must be released before an operation that drains Vault readers can complete.

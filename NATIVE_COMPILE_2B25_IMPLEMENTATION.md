# Rustnote 2b.25 — Transport Test Helper Scope Closure Implementation

## Implemented change

The protected PDF transport lifetime test now calls:

`crate::storage::protected_note_accepts_vault_reads_for_test(&storage, &note.id)`

instead of attempting to resolve the helper through `super::` from `storage::transport::tests`.

## Architectural effect

None in production. `protected_note_accepts_vault_reads_for_test` remains compiled only under `cfg(test)` and remains `pub(crate)`. Production transport reads, Vault read permits, Lock admission/revocation ordering, attachment materialization, tag mutation authorization, encryption, persistence, and schemas are unchanged.

## Validator compatibility

Build 2b.25 is admitted as the same current-schema/privacy architecture branch as 2b.24. No inherited invariant was weakened; only build-version admission was extended.

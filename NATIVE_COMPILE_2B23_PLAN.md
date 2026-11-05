# Rustnote 2b.23 — Protected Attachment Roundtrip Lifetime Closure Plan

## Audit finding

The serial native suite now advances through the 2b.22 media-lock regression and stalls in `protected_note_encrypts_attachment_blob_and_restores_it_losslessly`.

The test materializes a protected attachment through `protected_attachment_resource`, which intentionally returns a resource owning a `VaultReadPermit`. It then calls `remove_note_protection_with_password` while that resource is still alive. Unprotect correctly admits a privacy transition and waits for all existing Vault readers to drain. The test therefore retains the exact permit that Unprotect must wait for, creating a test-only circular wait.

## Objectives

1. Preserve production Vault reader and Unprotect lifetime semantics unchanged.
2. Release the materialized protected attachment resource after byte verification and before Unprotect.
3. Add a static regression guard proving the resource drop remains ordered before Unprotect.
4. Admit 2b.23 through inherited 2b/E0-E3 validators without changing schema or privacy contracts.
5. Re-run all available static/security and dependency-free behavioral QA, then package with exact archive comparison.

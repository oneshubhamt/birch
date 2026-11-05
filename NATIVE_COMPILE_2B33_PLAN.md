# Rustnote 2b.33 — dead-code closure plan

## Audit finding

`Storage::protect_note_with_password` became a legacy convenience wrapper after the locked-note public-label cutover. Production protection now enters through `protect_note_with_password_and_public_label`, but the wrapper remained compiled in the normal library and therefore emitted `dead_code`. An `internal-test-support` transport fixture still called the wrapper, so simply gating it to unit tests would break `--all-features` library compilation.

## Objectives

1. Compile the legacy password-only wrapper only for unit tests.
2. Move the internal-test-support transport fixture to the explicit label-aware protection entry point.
3. Do not suppress the warning with `#[allow(dead_code)]`.
4. Preserve all Vault/privacy semantics and the public locked-note label behavior.
5. Increment the source build to 2b.33 and retain inherited 2b.32 closure.

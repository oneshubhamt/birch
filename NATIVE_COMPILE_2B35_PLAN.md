# Rustnote 2b.35 — Vault selection continuity and perceived-responsiveness plan

## Audit findings

1. Vault actions intentionally revoke protected UI publication and, for Protect/Lock, remove decrypted note content before the canonical transition. That fail-closed privacy boundary must remain.
2. Post-transition remounting also cleared `selected_note` and yielded before restoring it. That second blank frame was unnecessary and caused the editor to land on `Select a note`.
3. While Protect/Lock is still running, the privacy-safe redaction can itself last long enough to look like lost selection. A dedicated transition shell can retain only the stable note ID plus non-secret transition metadata while private content stays revoked.
4. The editor component key used only `note.id`; `note_selection_generation` is the correct explicit remount identity.
5. Development testing uses `[profile.dev]` with default `opt-level = 0`. Argon2id and encryption are therefore materially slower under `cargo run --locked`; release-mode testing is required before changing security parameters.
6. Protect is intrinsically heavier than Lock/Unlock because it performs Argon2id wrapping, encrypted graph/blob conversion, plaintext-derivative cleanup, backup reset, FTS rebuild, WAL truncation, and `VACUUM`. Those privacy steps must not be weakened merely to improve perceived speed.

## Objectives

1. Preserve the selected note after successful Encrypt/Unlock/Lock/Remove Encryption without an intermediate post-action `None` publication.
2. During Protect/Lock, replace the privacy-required blank editor with a non-secret transition pane keyed to the same note ID.
3. Keep the same note card visually active during the transition using the stable note ID.
4. Never copy the unlocked/private note title into the transition shell; Protect may use the user-approved public label and Lock uses a generic non-secret label until the canonical locked stub is available.
5. Force clean editor remounts through `note_selection_generation`, not blank-frame choreography.
6. Show explicit progress text for all Vault operations.
7. Make no cryptographic/KDF, Vault-format, database-schema, E0–E3, attachment, autosave-policy, or transition-authority changes.
8. Require release-mode manual performance testing before deeper performance work.

# Rustnote 2b.38 QA — Vault action lifecycle closure

Targeted invariants:

- `apply_note_card_action` is root-scoped with `spawn_forever`.
- The self-unmounting action workflow contains no component-scoped `spawn(async move ...)`.
- Unlock still uses the secure transition shell and canonical post-transition reread.
- Canonical reread precedes selected-note republish, list refresh and transition cleanup.
- 2b.36 generation/session separation remains intact.
- public locked-note labels remain preserved by protected-summary hydration.
- inherited E0–E3/Vault privacy guards remain unchanged.

Mac-native final gates remain `cargo check`, strict Clippy, full serial Rust tests, full dependency-backed browser tests, and manual Unlock/Lock/Protect/Unprotect lifecycle stress testing.

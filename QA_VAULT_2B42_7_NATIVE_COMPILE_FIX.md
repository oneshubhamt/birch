# Vault 2b.42.7 native compile fix

This narrow closure fixes the first native Rust compile layer discovered on macOS after the profiling-only 2b.42.7 checkpoint.

## Fixed

- `src/ui/app.rs` now imports the existing `request_note_list_preferences_save` helper.
- Removed stale `synchronize_current_editor` imports from `src/ui/app.rs` and `src/ui/note_card.rs`.
- `note_link_targets` now has the explicit `Vec<NoteLinkTarget>` signal type required by Rust inference.
- `BacklinksPanelView` now receives its required `library_generation` field.
- Removed the unused `editor_session` destructuring binding from `apply_note_card_tag_action`.
- Updated the 2b.42.4 static/browser contracts to require the compile-safe typed note-link target signal.

No Vault behavior, privacy authority, persistence ordering, profiling vocabulary, or forensic cleanup ordering changed.

## Local assistant-side QA

- `validate-vault-2b42-7-native-compile-fix.sh`: 14/14 PASS.
- 2b.42.1 through 2b.42.7 validators: PASS.
- dependency-free browser/PDF behavior suite: 91/91 PASS.
- Native `cargo check`/Clippy/tests remain a macOS gate because this environment has no Rust toolchain.


## Strict Clippy closure discovered on macOS

The next native gate compiled successfully but strict `cargo clippy -- -D warnings` exposed six additional issues. This checkpoint closes them without lint suppressions:

- removed unnecessary `mut` from the aggregate status signal binding;
- removed the unused `Storage::export_portable_bundle` wrapper while preserving the locked implementation used by portability;
- removed obsolete `NoteListCache::redact_protected_note` left behind by scoped aggregate invalidation;
- removed obsolete `NoteSwitchLoadResult` left behind by source/target navigation separation;
- replaced the 8-argument `synchronize_current_editor_scoped` interface with `EditorSynchronizationContext` + mode;
- replaced the 10-argument whole-library synchronization interface with executor turn + shared context + operation privacy + mode.

The context is a packaging of existing dependencies only. It changes no synchronization, privacy, storage, or profiling semantics.

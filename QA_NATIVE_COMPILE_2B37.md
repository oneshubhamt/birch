# Rustnote 2b.37 QA

Targeted invariants: `ui/note_card.rs` has no unused `PrivacyPublicationToken` import; `EditorPane` has no unused local `note_selection_generation` binding; `library_state.note_selection_generation` remains the authoritative status/publication generation; the 2b.36 session/selection closure still passes; inherited Vault/E0–E3 expectations remain unchanged. Native `cargo check`, strict Clippy, and full Rust tests remain Mac-side gates.

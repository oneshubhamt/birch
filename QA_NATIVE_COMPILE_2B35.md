# Rustnote 2b.35 — QA

## Packaging-environment checks

- UI contract verifies post-Vault republishing does not publish an intermediate `None` selection or call `yield_now`.
- UI contract verifies both locked and unlocked editor keys include `note_selection_generation`.
- UI contract verifies the privacy-safe Vault transition pane exists, carries only public/generic metadata, and preserves note-list selection through the transition note ID.
- UI contract verifies explicit Vault progress messages are present.
- Dependency-free browser/PDF behavior suite remains green.
- The 2b.35 static guard verifies fail-closed Protect/Lock decrypted-content redaction remains present.
- The guard also verifies Lock never copies the decrypted private title into transition metadata.
- Inherited 2b.34/Vault/E0–E3 closure is retained.
- Rust compilation, strict Clippy, full Rust tests, dependency-backed browser QA, and release-mode manual performance observation must run on macOS.

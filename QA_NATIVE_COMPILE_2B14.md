# Rustnote 2b.14 — Editor Native Capture Closure QA

## Targeted static checks
- build identity is 2b.14;
- editor `onchange` clones its publication token before the spawned future;
- editor `oncommit` clones its publication token before the spawned future;
- attachment-command handler captures a local `ApplicationServices` clone;
- revision-restore handler captures a local `ApplicationServices` clone;
- the component-level `services` handle remains available for later settings/panel/note-link rendering;
- the native rustfmt expansion for image-staging invalid note identity is present.

## Regression chain
The 2b.14 validator chains:
- 2b.13 native compile closure;
- 2b.12 AEGIS/privacy-disclosure closure;
- 2b.11 atomic privacy-authority closure;
- E0–E3 ownership, residue, UI-lifetime, and adversarial closure validators.

## Native limitation
This environment still has no Rust compiler/toolchain. Native `cargo fmt`, `cargo check`, Clippy, and Rust tests must therefore be rerun on macOS. Build 2b.14 is the next native compile candidate, not a claim that native compilation has already succeeded here.

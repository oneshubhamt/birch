# Paragraph UI refinement — v0.12.5 build 1a.17

Date: 2026-08-28

## Objectives

1. Keep the compact `¶` toolbar trigger while making the surface a coherent Paragraph popover.
2. Keep Left/Right indent and Drop cap; clarify First-line and default first-line terminology.
3. Keep auto first-line indent and its amount together; move Spellcheck out of the popover to Settings → Editor.
4. Move Reset paragraph formatting to the bottom.
5. Present line spacing as Auto, 1.0, 1.15, 1.5, 1.75, and 2.0, with a persisted default for newly created normal paragraphs.
6. Add Left/Center/Right/Justify alignment without creating a separate editor subsystem.
7. Preserve canonical document validation, portability, and prior architecture boundaries.

## Implemented

- Paragraph popover is named `Paragraph`; the toolbar trigger remains `¶`.
- Alignment controls: Left, Center, Right, Justify.
- Indentation labels: Left indent, Right indent, First-line indent.
- Drop cap retained.
- Line spacing: Auto / 1.0 / 1.15 / 1.5 / 1.75 / 2.0.
- New paragraphs: Auto first-line indent + Default first-line indent remain together.
- Spellcheck remains in Settings → Editor and is removed from the Paragraph popover.
- Reset paragraph formatting moved to the bottom.
- Settings → Editor now has `Default paragraph line spacing`; the preference affects newly created normal paragraphs and does not silently restyle existing notes.
- Explicit new-paragraph spacing is stored in the canonical document, avoiding device-local rendering divergence.
- `textAlignment` is a canonical paragraph/heading attribute (`left`, `center`, `right`, `justify`) in document schema v6.
- Alignment is supported by the ProseMirror schema/rendering, Rust and JS validation, toolbar state/commands, HTML portability, block-style conversion, and foreign-clipboard layout sanitization.
- Atomic boundary paragraphs remain layout-neutral.
- Paragraph layout state correctly reports line spacing inside list items even though list indentation remains owned by the list system.
- Defaults also apply when Enter at the end of a heading creates a normal paragraph.

## QA executed in this environment

- Canonical editor specification validation: PASS — 21 nodes / 14 marks.
- Canonical JS document contract corpus: PASS — 36/36 cases.
- Paragraph/editor UI contract suite: PASS — 9/9 tests.
- Dependency-free editor/PDF browser suite: PASS — 20/20 tests.
- PDF behavior suite: PASS — 8/8 tests.
- Cumulative static architecture validation through B6: PASS.
- First-party JavaScript and shell syntax through B6: PASS.
- Architecture guards discover 108 Rust regression tests and 53 JS ProseMirror behavior tests.

## Environment-limited checks

- Rust compiler/Cargo are not installed in this sandbox, so `cargo fmt`, `cargo check`, `cargo clippy`, and `cargo test` could not execute here.
- The source archive intentionally does not ship `editor-build/node_modules`. Registry access is unavailable/unstable in this sandbox, so the dependency-backed ProseMirror behavior suite could not execute here. `prepare-rustnote-mac.sh` installs the pinned dependencies on the target Mac, after which the normal behavior runner can execute those tests.

No temporary `node_modules` directory is included in the source package.

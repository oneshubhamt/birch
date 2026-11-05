# Architecture A3 — ProseMirror source modularization QA

A3 is a structural pass. It must not intentionally alter editor behavior, document format, bridge protocol, persistence semantics, attachment behavior, or browser delivery. The modular source still bundles into one local `assets/prosemirror-editor.js` file.

## Automated acceptance

Run:

```bash
./scripts/validate-architecture-a3.sh
```

This first validates the modular source graph, then runs the complete A2 → A1 → A0 acceptance chain, rebuilds the prepared editor runtime, and validates the prepared browser runtimes.

## Manual regression corpus

Use a normal working library and verify the following surfaces after the automated suite passes.

### Editor lifecycle and persistence

- Open an existing note and a newly created note.
- Type continuously for several seconds; verify autosave and no visible input stalls.
- Switch rapidly between two notes after editing each; verify content never crosses note identities.
- Close/reopen Rustnote and confirm the final canonical editor state is restored.
- Undo and redo text plus structural changes.
- Toggle editable/read-only states where exposed and verify editor interaction follows the state.

### Text, structure and navigation

- Paragraphs, H1/H2/H3, quote and code block formatting.
- Bold, italic, underline/strike where supported, superscript/subscript, font size/family, text color/highlight and line spacing.
- Bullet, ordered and task lists; indent/outdent, split/merge, restart/continue numbering and checkbox interaction.
- Insert/edit/delete tables; add/remove rows/columns, header row and column sizing.
- Insert/edit ordinary links and Rustnote note links; exercise keyboard/context-menu invocation.
- Editor Find: next/previous matching and no-match behavior.
- Focus mode, Escape behavior and fullscreen command path.

### Scientific editor

- Inline and display equation insertion.
- Guided fraction/root/scripts/integral/summation/product forms.
- Matrix, determinant, cases and aligned-equation builders.
- Chemistry formula/reaction entry.
- Unicode/LaTeX smart paste and ordinary currency text that should remain prose.
- Copy/paste scientific nodes and convert inline ↔ display where supported.
- Arrow-key, Backspace/Delete and Escape behavior around scientific nodes.

### Attachments

For image, PDF, audio, video and generic file nodes:

- Insert from picker and verify placeholder → finalized node transition.
- Save, close and reopen the note; verify attachment references remain intact.
- Open externally / Save As / Reveal where applicable.
- Context-menu, long-press and keyboard contextual invocation.
- Remove the document node and verify the editor remains stable.

For images additionally verify:

- Paste and drag/drop import.
- Replace Image.
- Copy Image.
- Image edit, save edited result, cancel edit and reopen edited image.
- No unexpected blur/enlargement regression.

For media additionally verify:

- Play/pause/seek, resume position and compatibility fallback.
- Point/range annotations and deep-link seek/pause behavior.

For PDFs additionally verify:

- Embedded viewer opens and scrolls normally.
- Search/annotations still function.
- PDF export still reaches the system print/save flow.

## Performance sanity

A3 must not add network-loaded editor modules or multiple browser script loads. Confirm editor startup, note switching, typing, attachment interaction and scientific editing feel no slower than A2. The production browser surface should still execute the single prepared ProseMirror bundle.

# Architecture A2 — Typed editor bridge QA

A2 is an architectural bridge migration. It should not change editor behavior.

## Automated acceptance

Run:

```bash
./scripts/validate-architecture-a2.sh
```

The pass is accepted only when A1/A0 tests, the rebuilt editor runtime, and prepared-runtime validation all pass.

## Targeted manual regression

1. Open an existing note and create a new note. Confirm the editor initializes without an `ACTION FAILED` bridge error.
2. Type continuously, switch notes quickly, return, and confirm autosave/reopen preserves the correct note content.
3. Exercise bold/italic/underline, typography/font size, lists/checklists, tables, undo/redo, find, links, insert menu, editor focus mode, and window fullscreen.
4. Import an image by picker and by paste/drop; edit/replace it; verify completion and failure paths do not leave a stuck pending node.
5. Import generic file, PDF, audio, and video attachments. Exercise Open, Save As, Reveal, pending-ingest recovery, and attachment removal from the document.
6. Exercise media compatibility probing/preparation with a compatibility candidate and verify playback state still updates.
7. Insert a note link, open it, cancel the picker once, then insert another successfully.
8. Export an embedded PDF and verify the completion/failure status returns to the editor.
9. Restore a revision and confirm the editor accepts/replaces state without remount unless genuinely necessary.
10. Close/reopen Rustnote and repeat one edit + one attachment action to catch lifecycle/dispose regressions.

## A2 architecture invariants

- Rust sends versioned `EditorCommand` envelopes rather than naming individual JavaScript functions.
- JavaScript emits versioned `EditorEvent` envelopes through one Dioxus bridge element.
- The editor runtime exposes one public namespace: `window.__rustnoteEditorBridge`.
- Legacy per-command editor globals and the three legacy hidden bridge elements are absent.
- Controller/error state is runtime-local rather than published on `window`.
- Boolean, string, and count command results retain their expected host-side types.
- Wrong protocol versions, unknown commands, malformed command payloads, unknown events, and malformed event payloads are rejected rather than guessed.

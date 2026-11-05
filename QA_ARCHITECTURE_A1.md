# Rustnote Architecture A1 — Targeted QA

A1 introduces an application-service boundary without intentionally changing editor behavior.
Run this regression set after `./scripts/validate-architecture-a1.sh` passes.

## Editor persistence

- Open a note, edit text, switch to another note, then reopen the first note. Confirm the edit persists.
- Make rapid edits and switch notes repeatedly. Confirm content never crosses between notes.
- Edit a title and body, open version history, and restore a revision. Confirm the restored document, attachment registry, and note-link state refresh together.
- Quit after editing and reopen Rustnote. Confirm the latest committed/autosaved state is recovered as before A1.

## Attachments

- Insert one image, one PDF, one audio/video attachment, and one generic file.
- Insert multiple attachments in one picker operation. Confirm staged placeholders resolve at their original insertion positions.
- Cancel an attachment picker. Confirm no attachment or placeholder remains.
- Open, Save As, and Reveal an attachment from both the editor context surface and Attachments panel.
- Exercise a pending attachment recovery path if available; confirm finalized attachments refresh and unresolved items remain recoverable.

## Images

- Paste/drop an image and use Replace Image where available.
- Apply an image edit and confirm the derived rendition replaces the editor node correctly.
- Trigger/cancel a read-only image import/edit attempt and confirm staged files are discarded without changing the note.

## Media compatibility

- Open an attachment that requires compatibility probing/preparation.
- Confirm probe/prepare/invalidate responses still reach the editor and playback remains unchanged.

## PDF export

- Export an annotated PDF from the embedded PDF viewer and save it successfully.
- Cancel the save dialog and confirm the editor returns to its normal state without an error toast.
- Confirm the resulting PDF opens normally and annotations are present.

## General editor regression

- Formatting, lists/checklists, tables, links, scientific nodes, undo/redo, find, note links, right panel, fullscreen/focus mode.
- Confirm no new visible delay when opening a note or inserting an attachment.

## Automated acceptance

Run after the normal macOS preparation step:

```bash
./scripts/validate-architecture-a1.sh
```

This validates the A1 dependency boundary and then runs the full A0 behavioral Rust test suite.
`RUSTNOTE_A1_STATIC_ONLY=1` is reserved for source-only environments that do not have the Rust toolchain; it is not a substitute for the full validation on a development Mac.

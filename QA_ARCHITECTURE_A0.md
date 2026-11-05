# Architecture A0 regression corpus

This is the minimal manual regression corpus protecting the user-visible editor surface before architectural refactoring. It is intentionally small; automated unit/contract coverage lives beside the affected Rust modules.

Run the automated safety net with `./scripts/validate-architecture-a0.sh`, then use this corpus after architecture passes that touch editor orchestration, the Rust↔JavaScript bridge, canonical document handling, autosave, or attachments.

- Create a note, type plain text, apply bold/italic/underline, undo and redo, then reopen the note and verify the canonical content persists.
- Create bullet, numbered and checklist items; toggle a checklist item; reopen the note.
- Insert and edit a table; verify navigation, editing and persistence.
- Insert a web link and a note link; open both through their normal interaction paths.
- Attach an image; verify rendering, resize/selection behavior, Replace/Copy Image where applicable, save/reopen, and attachment-panel actions.
- Attach a PDF; verify embedded loading, scrolling, search/annotations if present, reopen, and PDF export/print flow.
- Attach representative audio and video; verify playback, compatibility preparation when required, seek/pause/resume, and reopen.
- Insert representative scientific content (inline/display math and other currently supported scientific nodes); edit and reopen it.
- Switch rapidly between two edited notes, return to each, and verify neither note receives the other's content or title.
- Make an edit, trigger an explicit save/checkpoint, close/reopen the note, and verify the saved state.
- Start an attachment import and verify normal completion; separately exercise startup/recovery with a durable pending ingest when practical.

Expected invariant: these checks should exhibit the same user-visible behavior before and after an architecture-only pass.

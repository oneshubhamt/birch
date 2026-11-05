# A5 Application Execution + Structured Error QA

A5 moves blocking-job ownership into the application layer and introduces structured errors at the three application-service boundaries. It is an architectural pass; user-visible behavior should remain unchanged.

## Automated acceptance

From the Rustnote source directory on macOS:

```bash
./scripts/prepare-rustnote-mac.sh
./scripts/validate-architecture-a5.sh
```

The A5 validator protects the complete A0–A4 chain, rebuilds/validates the prepared editor runtime through that chain, runs the workspace tests, and performs an additional locked workspace check.

## A5 invariants

1. **One blocking execution authority** — `tokio::task::spawn_blocking` exists only inside `application::executor`.
2. **Application-owned queue** — bounded FIFO status, editing locks, worker-panic handling, and operation lifecycle no longer live in Dioxus UI code.
3. **Rendering-only background UI** — `ui/background.rs` only renders `ApplicationExecutionStatus`.
4. **No duplicate busy-operation state** — settings and attachment import use executor status rather than maintaining a second `ApplicationOperation` signal.
5. **Structured service errors** — `EditorService`, `AttachmentService`, and `ExportService` expose `EditorError`, `AttachmentError`, and `ExportError`; presentation code turns errors into messages.
6. **Autosave remains specialized** — its dedicated worker and emergency-journal worker are unchanged and are not routed through the generic executor.

## Targeted manual regression

- Edit a note, switch rapidly between notes, and verify the note being left is saved and the newest selection wins.
- Trigger a manual save/checkpoint and close Rustnote immediately afterward; reopen and verify the latest edit is durable.
- Import several attachments, including a large file. Verify staging/finalization progress, editor responsiveness, and final embedded attachments.
- While an attachment import is active, attempt another attachment import. Confirm Rustnote rejects/queues work consistently rather than starting duplicate finalization.
- Open, Save As, and Reveal an attachment; cancel Save As once and confirm cancellation is quiet and leaves no stuck busy state.
- Exercise image import/edit and media compatibility preparation; verify failures, if induced with an invalid/missing staged resource, surface as a normal status message and do not strand the editor.
- Export Markdown, plain text, a complete archive, and an annotated PDF. Cancel at least one file dialog and confirm the application immediately becomes usable again.
- Start a long export/maintenance operation and then start one additional tracked operation. Confirm the second can wait; a third competing tracked operation should be rejected cleanly instead of freezing the UI.
- Restore a note revision. Confirm editing is temporarily unavailable during the critical restore and automatically re-enables on success or failure.
- Verify/restore a complete archive using a disposable test library. Confirm editing is paused across preparation/restore and always re-enables if preparation fails or the restore is cancelled/fails.
- Open Settings during background maintenance and confirm the status surface reflects executor state without duplicate or permanently stuck busy indicators.
- Leave Rustnote open long enough for automatic-backup/trash-maintenance scheduling to execute at least once in a test environment; normal editing should remain responsive.

## Performance/architecture sanity

- Normal typing, scrolling, note switching, and editor commands should feel identical to the A1–A4 closure build.
- Short reads/preferences should not display unnecessary global progress UI.
- Tracked storage-heavy work remains bounded to one active operation plus one waiter.
- No new browser runtime or network dependency is introduced by A5.

## Closure criterion

A5 is complete when the automated acceptance command passes and the targeted regression above shows no stuck operation state, lost editing lock, persistence regression, or noticeable UI-performance regression.

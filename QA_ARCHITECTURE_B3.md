# Architecture B3 — editor workflow orchestration boundary

## Scope

B3 finishes the high-value editor workflow boundary without changing editor features or the
underlying storage/autosave/media systems.

The pass moves multi-step attachment, media, PDF-export, image, and revision-restore sequencing
out of `src/ui/editor.rs` into two explicit layers:

```text
Dioxus editor component
        │ guards / signals / presentation
        ▼
ui/editor_workflows.rs
        │ browser bridge choreography
        ▼
application/editor_workflows.rs
        │ execution + cross-service sequencing
        ▼
EditorService / AttachmentService / ExportService
```

The application workflow service is Dioxus/browser independent. The UI workflow adapter is the
only new layer allowed to combine Dioxus signals with editor bridge calls.

## Objectives

1. Remove attachment staging/finalization/recovery sequencing from `ui/editor.rs`.
2. Remove media compatibility execution sequencing from `ui/editor.rs`.
3. Remove PDF export completion sequencing from `ui/editor.rs`.
4. Remove image import/edit execution sequencing from `ui/editor.rs`.
5. Remove revision restore/replacement/normalization flow implementation from `ui/editor.rs`.
6. Preserve the existing durability invariant: attachment finalization starts before the next
   browser await once a managed stage is durable.
7. Preserve recovery behavior: finalized attachment state refreshes even if embedding-intent lookup
   subsequently fails.
8. Keep Dioxus/browser transport out of the application layer.
9. Add structured workflow errors only for the touched boundary; do not launch a project-wide
   `Result<_, String>` rewrite.
10. Remove the old sequencing paths before closure.

## Baseline → B3

| Metric | B2 | B3 |
| --- | ---: | ---: |
| `src/ui/editor.rs` lines | 2,072 | ~1,500 |
| `document::eval` occurrences in `ui/editor.rs` | ~39 | 15 |
| workflow `ApplicationExecutor::read/start_read/execute` calls in `ui/editor.rs` | many | 1 unrelated note-link read |
| attachment/media/export/revision implementation calls in `ui/editor.rs` | many | 0 |
| typed editor workflow error | none | `EditorWorkflowError` |
| typed bridge-facing coordinator error | none | `EditorCoordinatorError` |

Line count is evidence of responsibility movement, not a target by itself.

## Behavioral invariants

- Attachment insertion anchor is synchronized/projected and saved before file selection continues.
- Managed staging is created before finalization.
- Finalization is started before the placeholder-update browser await.
- Durable pending ingest state remains recoverable after editor unmount/note switch/process failure.
- Finalized attachment lists update before an embedding-intent lookup warning can abort insertion.
- Generation checks still suppress stale recovery results.
- Image staging tokens are discarded on read-only/busy/error paths exactly as before.
- Media compatibility status strings sent to the editor remain `ready`, `invalidated`, `failed`,
  `error`, or the probe result.
- PDF export retains saved/cancelled/error completion semantics.
- Revision restore still synchronizes the current runtime state, validates the selected state in the
  runtime, commits through `EditorService`, reloads/remounts when required, and persists any runtime
  normalization as maintenance.
- Autosave remains specialized; B3 does not move it into the generic executor.

## Targeted manual QA on macOS

1. Insert image/audio/video/PDF/general files; verify placeholders, completion, ordering and reopen.
2. Cancel the picker and verify no broken pending node remains.
3. Start an attachment import, switch notes, return, and verify recovery/finalization.
4. Exercise media probe/preparation/invalidation with compatible and incompatible media.
5. Import pasted/dropped images and perform an image edit.
6. Export an annotated PDF: save, cancel, and destination/error paths.
7. Restore a revision normally and verify history retains the replaced state.
8. Restore a revision that forces runtime reload/remount and verify the final canonical document.
9. Reopen the app and verify autosave/recovery behavior is unchanged.

## Acceptance

Run:

```bash
./scripts/validate-architecture-b3.sh
```

The full path executes B3 static invariants, the complete B2→B1→A0–A7 chain, prepared editor/PDF
runtime validation, `cargo fmt --check`, strict Clippy, and Rust tests where the toolchain/dependencies
are installed.

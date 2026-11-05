# Rustnote Architecture B1 — Application/Storage Boundary Closure

B1 finishes the application/storage boundary left open after A0–A7. It is an architecture-only pass: normal note, editor, attachment, media, backup, settings, and export behavior should remain unchanged.

## What changed

- Dioxus UI modules no longer import or own `Storage`, `AutosaveController`, media-compatibility implementation modules, native staging/handoff modules, or platform primitives.
- A concrete `ApplicationServices` composition root owns the opened-library service set.
- Existing `EditorService`, `AttachmentService`, and `ExportService` are joined by broad `LibraryService`, `NoteService`, `SettingsService`, and `RuntimeService` capabilities.
- The storage boundary now mirrors those responsibilities with concrete `LibraryStore`, `NoteStore`, `SettingsStore`, `RuntimeStore`, `EditorStore`, `AttachmentStore`, and `ExportStore` facades over one shared `Storage` handle.
- The obsolete source-only capability was folded into `NoteStore`/`NoteService`; there is one source/citation workflow path rather than a parallel repository surface.
- UI-owned autosave access was removed. Editor persistence, checkpoint, flush-ticket, restore, event-subscription, stale-version, discard, and library-reset operations now flow through `EditorService`; attachment insertion/import durability flows through `AttachmentService`.
- Runtime attachment descriptor/media policy moved out of `ui/live_editor.rs` and behind `AttachmentService`.
- Prepared attachment imports are application-owned opaque values; the UI no longer receives a storage ingest DTO through a re-export.
- UI persistence intent uses `NoteChangeScope`; `NoteSaveScope`/`RevisionMode` remain storage/application implementation details rather than UI API.
- Configured storage locations are exposed through an application-owned DTO rather than re-exporting the storage type.
- Library-scoped transient cleanup is owned by `RuntimeService` during application open.

## Automated acceptance

After the normal macOS preparation step, run:

```bash
./scripts/validate-architecture-b1.sh
```

For a source-only environment without Rust/Cargo:

```bash
RUSTNOTE_B1_STATIC_ONLY=1 ./scripts/validate-architecture-b1.sh
```

The full mode runs B1 boundary checks and then the complete A7 acceptance chain, which includes formatting, strict all-target/all-feature Clippy, and workspace tests.

## B1 boundary invariants

- `src/ui/` has no dependency on `crate::storage`, `Storage`, `NoteSaveScope`, `RevisionMode`, `AutosaveController`, `crate::media_compat`, `crate::media_server`, `crate::image_rendition`, `crate::native`, or `crate::platform`.
- Application services do not hold unrestricted `Storage`; only the application composition root creates services from an opened library.
- Storage capabilities are concrete wrappers, not traits or separate connections.
- Autosave remains specialized internally, but no Dioxus component owns it directly.
- `LiveEditor` receives runtime attachment facts through `AttachmentService` rather than storage/media implementation modules.
- No duplicate legacy UI→Storage or UI→media/native pathway remains.
- No release-profile change is made.


## Boundary delta from A7

Static source comparison against the final A7 baseline:

- UI modules importing `crate::storage`: **17 → 0**.
- UI modules with direct `storage.<operation>()` calls: **24 → 0**; distinct direct operations: **52 → 0**.
- UI modules owning/importing autosave machinery: **13 → 0**.
- UI modules importing media implementation modules: **3 → 0**.
- UI modules importing native/platform implementation modules: **1 → 0**.
- UI use of storage persistence policy types (`NoteSaveScope`/`RevisionMode`): **eliminated**.
- Storage-side application capabilities: **7 broad concrete facades** over the existing shared `Storage` handle; no table-per-repository proliferation.

The underlying `Storage` implementation remains the single persistence authority. B1 narrows who can reach it rather than optimizing for an arbitrary method-count reduction.

## Targeted manual QA

1. **Startup / shutdown**
   - Launch with the default library and a configured custom library.
   - Edit a note and close immediately; reopen and confirm the latest state is present.
   - Confirm startup maintenance, expired-trash cleanup, and automatic backup status remain normal.

2. **Note navigation and persistence**
   - Create notes from the sidebar/header and inside a notebook.
   - Edit text/title/tags, rapidly switch notes, then return to each note.
   - Pin, archive, trash, restore, duplicate, move, and permanently delete a trashed note.
   - Confirm filters, sorting, search, notebooks, hierarchy, backlinks, properties, and sources/citations refresh correctly.

3. **Autosave / history**
   - Type continuously, pause, switch notes, and reopen the app.
   - Restore a revision and confirm editor content, attachment registry, and note links refresh together.
   - Change history retention/storage settings and prune history.

4. **Attachments / images**
   - Import image, PDF, audio/video, and generic files.
   - Exercise multi-file import and interrupted-import recovery if available.
   - Open, Save As, and Reveal from editor and right-panel surfaces.
   - Paste/drop/replace/edit an image and confirm staged files are cleaned on success/cancel/failure.

5. **Media / PDF**
   - Play native-compatible and compatibility-prepared audio/video; seek, pause, resume, reopen.
   - Open small and large PDFs; scroll, search, annotate, export, close/reopen.

6. **Settings / backup / restore**
   - Change appearance/layout preferences and restart.
   - Inspect backup diagnostics, export a complete archive, verify a restore candidate, and perform a restore.
   - Confirm the editor is disposed/reset and the restored library reloads normally.

## Performance sanity

B1 adds no new database connection, dynamic dispatch, DI framework, or production runtime layer. The capability wrappers are ordinary concrete Rust structs over the existing shared `Storage` handle. Note switching, typing, list scrolling, attachment insertion, media playback, and PDF interaction should therefore show no measurable regression from A7.

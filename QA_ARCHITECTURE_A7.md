# Rustnote Architecture A7 — Final Closure QA

A7 is a closure/cleanup pass. It should not introduce user-facing features or change normal editor, storage, attachment, media, or PDF behavior.

## Automated acceptance

Run after `./scripts/prepare-rustnote-mac.sh`:

```bash
./scripts/validate-architecture-a7.sh
```

This runs the cumulative A0–A6 acceptance chain, prepared browser-runtime checks, `cargo fmt --check`, strict all-target/all-feature Clippy, and all-feature workspace tests.

## Manual full-regression checklist

1. Launch Rustnote with the default library and with a custom library path. Confirm only one active library is used and startup maintenance completes normally.
2. Create, edit, rapidly switch between, archive, restore, trash, and permanently delete notes. Confirm autosave, revision history, search, filters, tags, notebooks, hierarchy, links, properties, and sources/citations behave normally.
3. Exercise rich editor behavior: formatting, lists/checklists, tables, links, find, undo/redo, slash/insert commands, scientific/math/chemistry nodes, keyboard/context-menu/long-press interactions, and editor remount/reopen.
4. Attach and operate on images, PDFs, audio, video, and generic files. Test Open, Reveal, Save As, replace/edit image flows, pending ingest recovery, large-file import, and missing-file reporting.
5. Confirm attachment handoffs are created under the active library's canonical cache/handoff root and stale library-scoped handoffs are cleaned without using the old process-global temp fallback.
6. Open ordinary and large PDFs. Test scrolling/virtualization, thumbnails, zoom, search, annotations, annotation index, text/ink/shapes, save/export/print, close/reopen, and repeated navigation. PDF search must not throw a JavaScript `ReferenceError` while indexing pages.
7. Test audio/video compatibility preparation, playback, seek/pause/resume, annotations/bookmarks, cache pruning, and reopening notes.
8. Export note text/Markdown/PDF and complete archive outputs. Confirm names, overwrite behavior, and file contents are correct.
9. Exercise Settings operations that use background/exclusive execution, including restore flows and maintenance. Confirm editing locks/status clear after success and failure.
10. Confirm startup/shutdown durability: edit several notes, close immediately, reopen, and verify the latest state; then test recovery after an interrupted attachment import if practical.

## Performance sanity

- Note switching and typing should feel unchanged from A6.
- No new UI blocking should appear during attachment/media/PDF background work.
- PDF rendering/search should have no regression from A6.
- Startup should not scan the old global temporary handoff directory.

## Closure invariants

- Remote-sync scaffolding remains present and compile-checked under `--all-features`; A7 does not remove or redesign it.
- No direct SQLite access exists outside `src/storage/`.
- No process-global active-library lookup exists.
- The editor uses one typed/versioned bridge namespace.
- ProseMirror and PDF source stay modular while production runtimes remain single bundles.
- Autosave/emergency-journal specialized workers remain outside `ApplicationExecutor`.

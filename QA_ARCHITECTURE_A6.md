# Rustnote Architecture A6 QA

A6 is a structural decomposition pass. It must not change note semantics, attachment/file behavior, or PDF viewer behavior.

## Automated acceptance

Run after `./scripts/prepare-rustnote-mac.sh`:

```bash
./scripts/validate-architecture-a6.sh
```

The script validates A1–A5 first, then the A6 domain/native/PDF decomposition and prepared runtime.

## Targeted manual regression

1. **Core notes/domain**
   - Launch Rustnote and open an existing note.
   - Create, rename, pin, archive, trash, restore, and permanently delete disposable notes.
   - Change notebook, tags, filters, sort order, hierarchy, links, and sources/citations.
   - Confirm note list pagination/search and note switching remain normal.

2. **Attachments/native operations**
   - Add an image, PDF, audio/video file, and a generic file.
   - Open, Reveal, and Save As an attachment.
   - Paste/drop/replace an image and commit one image edit.
   - Confirm staged imports/edits disappear after completion/cancellation as before.

3. **PDF viewer — ordinary and large files**
   - Open a small text PDF and a larger PDF that uses range transport.
   - Verify first render, scrolling, zoom, fit width/page, page navigation, and thumbnails.
   - Verify outline navigation where an outline exists.
   - Search forward/back through several matches and confirm highlighting.
   - Exercise markup/annotations, undo/redo, annotation sidebar, and annotated PDF export.
   - Reopen the PDF and verify saved annotations remain.
   - Check a scanned/image-heavy PDF for the same smoothness/memory behavior as A5.

4. **Lifecycle/performance**
   - Switch repeatedly between notes containing PDFs/images and ordinary text notes.
   - Close/reopen notes and the app; confirm no stale viewer/attachment state.
   - Confirm there is no new visible delay from the source modularization (production remains one PDF bundle).

## Pass criteria

A6 closes when automated validation passes and the targeted regression above shows no behavioral or performance regression.

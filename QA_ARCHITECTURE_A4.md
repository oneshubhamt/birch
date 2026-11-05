# Architecture A4 QA — Explicit library and storage boundaries

A4 is an architecture-only pass. It should not change visible editor, attachment, media, PDF, storage-location, or export behavior.

## Automated acceptance

After `./scripts/prepare-rustnote-mac.sh`, run:

```bash
./scripts/validate-architecture-a4.sh
```

This runs the A4 storage/library boundary checks and then the full A3 → A2 → A1 → A0 acceptance chain, including the Rust workspace tests.

## Targeted manual regression

Use an ordinary library containing text notes plus image, PDF, audio/video and generic-file attachments.

1. **Startup / library binding**
   - Launch Rustnote normally and confirm the library opens without a startup recovery error.
   - If you use a custom data directory, confirm that exact library opens and its existing notes/attachments appear.
   - Open Settings → Storage and confirm the displayed database, attachment and backup locations still match the configured library.

2. **Notes / persistence**
   - Edit an existing note, switch to another note, switch back, and confirm the edit persisted.
   - Create a new note, type content, close/reopen Rustnote, and confirm it persisted.
   - Exercise undo/redo once to ensure A3 editor behavior remains intact.

3. **Images and generic attachments**
   - Open an existing image inline; confirm original/screen rendition loading remains normal.
   - Import an image and a generic file.
   - Use Open, Save As and Reveal in Finder on an attachment.
   - Confirm image editing/replacement still completes and refreshes the attachment.

4. **Audio/video compatibility**
   - Play a native-compatible audio/video attachment and seek within it.
   - For a compatibility-candidate file (for example MKV/MKA used in prior QA), trigger the compatibility path and confirm playback still starts after preparation.
   - Restart Rustnote and confirm compatible-cache reuse still works.

5. **PDF transport**
   - Open a small PDF and a larger PDF.
   - Scroll quickly through several pages and confirm rendering/range transport remains responsive.
   - Add/edit/remove a PDF annotation and reopen the PDF to confirm persistence.
   - Confirm annotation search/index/sidebar data still appears where applicable.
   - Export an annotated PDF and confirm the saved file opens externally.

6. **Storage-location semantics**
   - In Settings, choose different storage locations but do not restart; confirm Rustnote still reports that a restart is required and continues using the current library during that process.
   - Cancel/revert unless you intentionally want to test a new library after restart.

## A4 acceptance criteria

A4 is closed when:

- `./scripts/validate-architecture-a4.sh` passes;
- the manual checks above show no behavior regression;
- there is no `ACTIVE_DATA_DIRECTORY`, `Storage::active_data_directory`, or `active_attachment_resource` path left;
- `attachment_protocol.rs` contains no `rusqlite`/SQLite query implementation;
- protocol/media/native long-lived services receive the active library or its paths explicitly;
- application services depend on narrow concrete storage capabilities rather than unrestricted `Storage`;
- no duplicate legacy storage/transport pathway remains.

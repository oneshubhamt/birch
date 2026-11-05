# Image Paste/Drop Staging Fix — QA Record

## Audit finding

Rustnote already detected image `File` objects from paste/drop and deliberately kept image bytes out of the JSON/Dioxus control bridge. The failure occurred one layer later: the browser posted those bytes to the Dioxus/Wry custom asset protocol, while the native `image-import-stage` handler could receive an empty request body. Native staging correctly rejected that zero-byte body as `Image import staging is empty`.

## Objectives

1. Preserve binary staging: do not reintroduce Base64 or full-image JSON payloads.
2. Use one reliable browser-to-Rust binary transport rather than creating a second attachment engine.
3. Retain the existing 64 MB transient import bound, opaque single-use staging tokens, cleanup, and canonical attachment finalization.
4. Cover clipboard implementations that expose copied images through `DataTransfer.items` instead of `DataTransfer.files`.
5. Reject truncated/aborted uploads instead of accepting a partial stage.
6. Leave ordinary text paste and internal ProseMirror drag behavior unchanged.

## Implementation

- The existing authenticated `127.0.0.1` Rustnote media transport now exposes a token-protected `/stage/<server-token>/image-import` endpoint.
- The editor receives that endpoint through the typed install payload (`image_import_stage_url`).
- Clipboard/drop image `File` bodies are POSTed directly to the loopback endpoint; only file metadata plus the resulting 32-hex stage token use the editor command bridge.
- DELETE on `/stage/<server-token>/image-import/<stage-token>` performs best-effort cleanup for failed/cancelled imports.
- CORS/preflight handling is limited to the staging endpoint; the unguessable 256-bit server token remains required in the URL.
- Rust streams the declared body directly to the temporary stage file and verifies that the exact declared byte count arrived before returning a token.
- The former custom-protocol endpoint remains as a compatibility fallback when no valid loopback endpoint is installed.
- Paste/drop extraction now checks `DataTransfer.files` first and then `DataTransfer.items`.

## Automated/static checks in this pass

- `node --check` across modular editor JS/MJS source: PASS.
- Dependency-free browser/editor regression suite: 20/20 tests PASS.
- Canonical editor-document specification validation: PASS (21 nodes, 14 marks).
- JS editor-document contract corpus: 36/36 cases PASS.
- `scripts/validate-architecture-a2.sh` when invoked with Bash: typed editor bridge checks PASS (the script later attempts a `/bin/zsh` nested script, unavailable in this sandbox).
- `scripts/validate-architecture-a3.sh` when invoked with Bash: modular ProseMirror source checks PASS (same nested `/bin/zsh` environment limitation afterward).
- Targeted static wiring assertions: paste/drop → staging URL → controller → typed install payload → loopback stage reader: PASS.
- Rust unit tests added for exact-length stream staging, truncated-body rejection/partial-file removal, and authenticated staging-route shape.

## Environment limitation

This sandbox does not contain the Rust/Cargo toolchain or macOS WKWebView runtime, so the Rust unit tests and a real macOS clipboard integration run could not be executed here. They must run during the normal Rustnote build/test on macOS.

## Targeted manual acceptance QA on macOS

1. Copy a normal PNG/JPEG/WebP image from a browser and paste into a note: one embedded image should appear and persist after reopening the note.
2. Copy an image from Evernote and paste: the image should embed without `Image import staging is empty`.
3. Drag an image file from Finder into the note: it should use the same staging/finalization path and persist.
4. Paste ordinary text/HTML from a browser: no image interception should occur when there is no image `File` item.
5. Paste several images together: all should stage/finalize, or already-staged temporary files should be discarded if a later image fails.
6. Try an empty image-like file and an image above the 64 MB transient limit: both should fail explicitly without creating a usable stage token.
7. Abort/interrupt an upload if practical: the native side should reject a byte-count mismatch and remove the partial stage file.

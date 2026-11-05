# PDF Viewer UI Restoration — v0.12.5 build 1a.23

## Objective

Restore the PDF reader chrome and sidebar without changing the PDF transport, raster scheduler, preview cache, annotation persistence, or runtime-integrity architecture repaired in 1a.22.

## Changes

- Primary PDF toolbar remains sticky at all embedded widths.
- Markup toolbar remains sticky below the measured primary toolbar height.
- The <=760 px rule now compresses controls horizontally; it no longer switches the iframe into a phone-style scrolling/overlay layout.
- PDF sidebar remains a sticky in-layout column at normal embedded widths.
- Multi-page PDFs open with the thumbnail sidebar visible; one-page PDFs remain compact.
- Viewer chrome geometry is synchronized through `--viewer-toolbar-height` and `--viewer-chrome-height`, measured from the live DOM.
- Search, Markup, Focus and iframe resize transitions resynchronize chrome geometry.
- A ResizeObserver guards toolbar-height changes without introducing polling.

## Regression coverage

`tests/js/pdf-layout-integrity.test.mjs` covers eight layout contracts:

1. persistent primary toolbar;
2. persistent markup toolbar;
3. non-overlay sidebar;
4. multi-page initial sidebar policy;
5. one measured chrome-geometry authority;
6. Search/Markup/resize/Focus synchronization;
7. narrow-width horizontal compression rather than chrome dismantling;
8. Thumbnails/Contents/Annotations sidebar surfaces remain present.

The test is part of `scripts/run-browser-behavior-tests.mjs` and the cumulative B5 PDF architecture guard.

## Verification

Source-level verification in the available sandbox:

- PDF layout-integrity tests: 8/8 passed.
- Existing PDF behavior tests: 8/8 passed.
- Existing PDF runtime-integrity tests: 8/8 passed.
- Cumulative dependency-free browser/editor/PDF suite: 36/36 passed.
- Canonical editor document specification: 21 nodes / 14 marks passed.
- JavaScript/MJS syntax checks across `pdf-src`, `editor-src`, `tests/js`, and affected build scripts: passed.
- B5 substantive architecture guard: passed (13 PDF modules; viewer controller remains below the 4,250-line ceiling; 112 Rust tests discovered statically).
- 1a.22 transport/cache/scheduler/error/annotation/virtualization/native protocol sources were byte-for-byte unchanged.

Environment limitations:

- This sandbox has no Rust toolchain, so `cargo check`, Clippy, and Rust tests cannot execute here.
- This sandbox has no `/bin/zsh`; zsh wrapper self-checks cannot execute here, although their substantive Node guards and Bash-compatible syntax paths were checked directly.
- Pinned editor/PDF build dependencies are not installed here, so the prepared WKWebView runtime and a live macOS visual test must be performed on the Mac build.

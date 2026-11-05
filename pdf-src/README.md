# Rustnote PDF viewer source

This directory contains Rustnote's maintainable PDF viewer source. The browser does not load these modules individually in production.

`viewer.mjs` is the single build entry. `scripts/build-pdfjs-runtime.mjs` bundles this source graph together with the pinned PDF.js legacy display/viewer modules into one minified `target/rustnote-pdfjs/runtime/viewer.mjs` file for WKWebView.

The source graph is intentionally coarse and acyclic:

- `config.mjs` — immutable limits, runtime URLs, and markup defaults.
- `transport_policy.mjs` — pure whole-file/range selection and initial-block policy.
- `transport.mjs` — bounded whole-file/range transport ownership, initial acquisition, and read-ahead cache.
- `rendering.mjs` — PDF.js page-view integration and safe layer properties.
- `preview_cache.mjs` — byte-accounted LRU preview-raster cache and adaptive eviction.
- `render_scheduler.mjs` — reading-window planning, render priorities, memory-aware targets, and raster-lane policy.
- `sidebar.mjs` — thumbnail/annotation virtualization geometry and sidebar presentation helpers.
- `virtualization.mjs` — page-number clamping and the Fenwick-tree geometry primitive.
- `ui.mjs` — static DOM references and status presentation.
- `errors.mjs` — pure PDF preview failure classification and user-facing categories.
- `annotations.mjs` — pure annotation conversion/geometry helpers.
- `search.mjs` — pure text-layer search mapping helpers.
- `viewer.mjs` — PDF/session lifecycle, live page-view orchestration, interactions, and browser event wiring.

Viewer layout follows one additional contract: iframe width may compress controls, but it must not change reader chrome into a phone-style overlay mode. `viewer.mjs` measures the live toolbar/chrome geometry into `--viewer-toolbar-height` and `--viewer-chrome-height`; the stylesheet consumes those values for sticky markup/sidebar offsets. Multi-page documents open with the in-layout thumbnail sidebar visible by default.

Do not add runtime network module loading. New source modules must be imported from `viewer.mjs` (directly or through an acyclic dependency), bundled by esbuild, and included in the prepared-runtime source checksum map.

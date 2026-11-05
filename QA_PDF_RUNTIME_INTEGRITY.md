# PDF Runtime Integrity pass

## Audit findings closed

1. Large-PDF startup called `fetchPdfBlock` from `viewer.mjs` even though the function was private to `transport.mjs`.
2. `updateMemoryAccounting()` still called the removed `updateAdaptivePreviewBudget()` wrapper after adaptive preview budgeting moved into `PreviewRasterCache`.
3. `goToPage()` calculated `next` but used the unresolved `pageNumber` identifier for geometry.
4. Startup flattened ordinary runtime/program failures into the misleading message “This PDF could not be opened safely.”
5. Existing PDF tests covered extracted algorithms but not the reassembled viewer/transport lifecycle.

## Objectives and implementation

- **Range transport ownership:** `transport.mjs` now owns whole-file/range selection, initial block acquisition, and the PDF.js document-source options through `preparePdfDocumentTransport()`. `viewer.mjs` no longer calls block/whole-file transport internals.
- **Preview budget ownership:** removed the stale call; `evictPreviewCacheToBudget()` delegates adaptive calculation to `PreviewRasterCache` as designed.
- **Page navigation:** page input is canonicalized through `clampPageNumber()` and that single resolved value is used for geometry and navigation.
- **Failure classification:** `errors.mjs` preserves password/document/unavailable/transport/viewer categories. Generic program failures are reported as Rustnote preview failures, not document-safety failures.
- **Runtime regression coverage:** added `tests/js/pdf-runtime-integrity.test.mjs` and wired it into cumulative browser/B4/B5/B6 QA.
- **Rust protocol coverage:** added a protocol test that checks the viewer's initial-block contract and bounded tail reads against the native PDF read policy.

## Targeted QA in this environment

- PDF algorithm + runtime-integrity tests: **16/16 passed**.
- Dependency-free browser/editor/PDF suite: **28/28 passed**.
- B4/B5/B6 Node architecture guards: **passed**.
- A6 static modular-PDF architecture guard: **passed** (13 PDF modules; viewer 4166 lines).
- A2/A3 static editor architecture guards: **passed**.
- Editor document specification: **21 nodes / 14 marks passed**.
- Editor document contract corpus: **36/36 passed**.
- Maintained JS/MJS syntax checks: **passed**.

## Environment limitations

This container does not provide the Rust toolchain or macOS WKWebView runtime, so the new Rust test could not be executed here and a live macOS PDF preview could not be exercised. The prepared PDF.js bundle also cannot be rebuilt here without the pinned npm/PDF.js build inputs. Those are the remaining machine-level acceptance checks.

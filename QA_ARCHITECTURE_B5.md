# Architecture B5 — cohesive PDF decomposition + scoped typed export errors

## Scope

B5 closes the remaining A6 PDF decomposition item without turning the viewer into a graph of tiny
callback wrappers. The pass extracts the three responsibilities that had independent state/policy and
were suitable for direct behavioral testing:

1. preview raster cache ownership and adaptive eviction;
2. sidebar virtualization/presentation helpers;
3. render scheduling policy, target planning, and raster-lane priority.

The live PDF.js page-view lifecycle remains in `viewer.mjs`: page proxy acquisition/release, native
page-view creation, render cancellation, detail render execution, viewport/session state, and browser
event wiring all share live objects and are intentionally kept together.

B5 also performs one opportunistic typed-error cleanup on the PDF export path already touched by the
pass. It does not start a project-wide `Result<_, String>` migration.

## PDF source boundary

```text
viewer.mjs
  PDF/session lifecycle + live page-view orchestration + interactions
       │
       ├── preview_cache.mjs
       │     byte-accounted LRU cache + adaptive budget/eviction
       │
       ├── render_scheduler.mjs
       │     reading window + target planning + priorities + lane policy
       │
       └── sidebar.mjs
             thumbnail/annotation windows + sidebar presentation helpers
```

Existing modules remain responsible for transport, PDF.js rendering integration, annotations,
search, UI references, configuration, and Fenwick geometry.

`viewer.mjs` changes from 4,456 lines in B4 to 4,178 lines in B5. More importantly, it no longer owns:

- preview-cache map/LRU byte accounting;
- adaptive preview-cache budget calculation;
- preview-cache eviction/disposal policy;
- thumbnail and annotation sidebar window geometry;
- thumbnail sizing/priority presentation logic;
- annotation label/anchor/summary/counter/navigation helpers;
- thumbnail/annotation row construction and outline tree construction;
- reading-window computation;
- memory-aware render-target planning;
- page/preview render priority calculation;
- raster-lane selection order.

## Behavioral preservation

B5 keeps the prior scheduling order exactly:

```text
visible blank page
  ↓
critical visible detail
  ↓
next page in reading direction
  ↓
bounded warm/read-ahead page
  ↓
settled optional detail
  ↓
visible thumbnail preview
```

The render-policy helpers are dependency-free and tested directly. Under a constrained memory budget,
the preserved warm-page ordering is next page, one page behind, then further forward read-ahead.

The preview cache remains optional under pressure: protected visible/current previews are preserved
under ordinary pressure where possible, but may be evicted when the absolute raster hard ceiling is
exceeded. Evicted canvases have their backing stores collapsed.

## Scoped typed error cleanup

The PDF staging functions in `src/native/staging.rs` now return:

```text
PdfExportStagingError
  InvalidToken
  InvalidPdf
  Prepare(io::Error)
  Create(io::Error)
  Stage(io::Error)
  AllocationFailed
  Discard(io::Error)
  Unavailable
  Save(String)
```

`ExportError` now carries `PdfStaging(PdfExportStagingError)` instead of flattening this boundary into
`Native(String)`. Existing user-facing error text is preserved through `Display`, while underlying I/O
errors remain available through `Error::source` where applicable.

The shared atomic-copy helper still returns `String`; B5 wraps that existing boundary as `Save(String)`
rather than expanding this pass into unrelated native/platform error refactoring.

## Behavioral coverage

B5 extends the B4 dependency-free PDF behavioral suite from 4 to 8 tests. New coverage verifies:

- LRU touch order, byte accounting, protected eviction, hard-pressure eviction, and canvas disposal;
- sidebar row windows and thumbnail window translation;
- annotation summary clamping/counter/navigation semantics;
- memory-aware target selection and visible-page priority;
- strict lazy raster-lane ordering so lower-priority work is not evaluated early.

A Rust unit test also verifies PDF staging error categories and preserved messages. The workspace Rust
test inventory therefore increases from 64 to 65 tests.

## Acceptance

Full acceptance on macOS:

```bash
cd /Users/shubhamtiwari/Developer/rustnote
./scripts/validate-architecture-b5.sh
```

If Rustnote dependencies/toolchains have not been prepared on the machine, run
`./scripts/prepare-rustnote-mac.sh` first.

Source-only validation:

```bash
RUSTNOTE_B5_STATIC_ONLY=1 ./scripts/validate-architecture-b5.sh
```

The B5 validator runs targeted PDF behavioral tests, B5 source-boundary checks, cumulative B4 → B1 →
A0–A7 static guards, and—during full acceptance—the complete B4 prepared-runtime/Rust/JS suite before
rechecking B5.

# Architecture B4 — behavioral test architecture

## Scope

B4 turns the remaining editor/PDF quality gap into executable behavioral coverage before any
further ProseMirror or PDF structural movement. Static architecture guards remain useful, but are
no longer treated as evidence that important runtime algorithms behave correctly.

B4 also closes the two small B3 follow-ups found during de novo audit:

1. image edit/import rejection cleanup no longer calls `AttachmentService` directly from
   `src/ui/editor.rs`; and
2. `EditorWorkflowService` now has direct behavioral tests for detached finalization, lookup-failure
   state preservation, and media-compatibility status semantics.

## Test layers

```text
Rust workflow behavior
    src/application/editor_workflows.rs tests

ProseMirror behavior
    tests/js/editor-prosemirror-behavior.test.mjs

Editor bridge behavior
    tests/js/editor-bridge-behavior.test.mjs

PDF algorithm behavior
    tests/js/pdf-behavior.test.mjs

Shared document contract
    existing 29-case B2 corpus

Architecture/static guards
    A0–A7 + B1–B4 validators
```

Node's built-in `node:test` runner is used rather than adding another JavaScript test framework.
The ProseMirror tests use the already-pinned dependencies in `editor-build/node_modules`; the B4
runner creates only a temporary root `node_modules` symlink when necessary and removes it after the
run.

## Behavioral coverage added

### Editor / ProseMirror

- atomic boundary normalization and idempotence;
- real block transformation through ProseMirror transactions;
- real list wrapping through ProseMirror transactions;
- horizontal-rule insertion boundary behavior;
- table insertion, header-row creation and trailing editable boundary;
- find indexing and source-position mapping;
- scientific-node find decoration behavior;
- ordered-list attribute normalization;
- scientific template caret placement.

### Editor bridge

- versioned event envelope;
- bubbling input dispatch;
- bounded attachment-status payloads;
- fail-closed behavior when the event bridge is absent.

### PDF

- Fenwick page-height prefix sums, updates and lower-bound lookup;
- text-layer search concatenation and DOM range mapping;
- annotation geometry clamping and PDF coordinate conversion;
- portable text annotation creation and empty-value rejection;
- vector annotation path generation;
- annotation deep-copy behavior;
- normalized pointer geometry;
- PDF scripting security invariant.

### Rust editor workflow coordinator

- detached attachment finalization continues before the browser-facing completion step is awaited;
- refreshed durable attachment state survives embedding-intent lookup failure;
- media compatibility retains the existing action-specific `error` / `failed` status semantics.

## B3 close-out

`ui/editor.rs` no longer directly performs image-stage cleanup or image rejection bridge scripts.
It delegates those guard-failure paths to `ui/editor_workflows.rs`, while native staging cleanup is
reached through `EditorWorkflowService` / `AttachmentService`.

This keeps the intended boundary:

```text
ui/editor.rs
  guards + intent
       ↓
ui/editor_workflows.rs
  bridge/presentation choreography
       ↓
EditorWorkflowService
  application cleanup / sequencing
```

## Acceptance

Full acceptance on macOS:

```bash
cd /Users/shubhamtiwari/Developer/rustnote
./scripts/validate-architecture-b4.sh
```

If the machine has not yet prepared Rustnote's pinned JavaScript dependencies and Rust toolchain,
run `./scripts/prepare-rustnote-mac.sh` first.

The B4 full path runs:

- B4 behavioral/static guards;
- dependency-free PDF behavior tests;
- cumulative B3 → B2 → B1 → A0–A7 acceptance;
- prepared editor/PDF runtime validation;
- Rust formatting, strict Clippy, and all-feature workspace tests through the cumulative chain;
- the complete ProseMirror/editor-bridge/PDF `node:test` suite using the pinned editor dependencies;
- a final B4 static recheck.

For source-only validation where Rust/editor dependencies are unavailable:

```bash
RUSTNOTE_B4_STATIC_ONLY=1 ./scripts/validate-architecture-b4.sh
```

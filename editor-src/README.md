# Rustnote editor source architecture

`prosemirror-editor.js` is intentionally a tiny build entry. The browser does not load these modules individually: `scripts/build-prosemirror-editor.mjs` bundles them into the single local `assets/prosemirror-editor.js` runtime.

## Ownership

- `config.js` — immutable editor constants, command catalogs, ProseMirror plugin keys, scientific registries.
- `ui/menu.js` — generic floating/menu keyboard and accessibility helpers.
- `bridge/events.js` — the single typed JavaScript → Rust event emitter.
- `runtime/state.js` — minimal mutable runtime controller/error state shared by node views and the host bridge.
- `attachments/import.js` — browser-side image staging, attachment import placeholders, and import recovery plugin.
- `document/schema.js` — ProseMirror schema plus canonical Rustnote envelope validation and media-link persistence helpers.
- `editor/core.js` — scientific notation, document normalization, editor plugins, selection/toolbar derivation, and scientific node views.
- `commands/index.js` — formatting/list/table/find/clipboard command implementations and editor-state serialization helpers.
- `attachments/view.js` — attachment/image/PDF/audio/video node view implementation and image-edit rendering helpers.
- `runtime/controller.js` — editor session/controller orchestration and transient editor surfaces.
- `bridge/host.js` — A2 versioned command dispatcher and the sole public `window.__rustnoteEditorBridge` namespace.

## Dependency rule

Local editor modules must remain acyclic. Lower-level modules must not import the controller or host bridge. Runtime/browser behavior should be changed in the owning module rather than reintroduced in the build entry.

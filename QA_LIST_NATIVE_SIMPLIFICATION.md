# Native List Simplification — 1a.16

> **Historical note:** This 1a.16 record describes the simplification baseline. Builds 1a.19-1a.20 subsequently add two deliberately narrow native-command policies for list-item Backspace exit and paragraph-after-list textblock joining; manual list reconstruction remains removed.

## Objective

Remove Rustnote's custom list-boundary editing engine and let ProseMirror own list structure and ordinary Backspace/Delete behavior. Retain only product behavior ProseMirror does not provide directly, including task-list conversion, list-style metadata, and Rustnote's em-dash sequence.

## Audit result

The 1a.14/1a.15 consolidation had accumulated a second list interaction authority around ProseMirror. The most harmful elements were custom Backspace/Delete boundary handlers, manual join/reconstruction logic, paragraph-layout normalization on list exit, ordered-list continuation normalization after lifting, and CSS compensation for the resulting list/paragraph/list transitions.

A comparison with 1a.13 also showed that the older editor already carried custom list-exit reconstruction in `editor-src/editor/interactions.js`. This pass therefore does not restore that code. It removes it.

## Implementation

- `editor-src/editor/list_system.js` is reduced to a thin list integration layer.
- Structural commands are ProseMirror's `wrapInList`, `splitListItem`, `sinkListItem`, and `liftListItem`.
- `listKeymap()` binds only:
  - Enter → native `splitListItem` chain for normal/task items;
  - Tab → native `sinkListItem` chain;
  - Shift-Tab → native `liftListItem` chain.
- Backspace and Delete have no Rustnote list bindings and fall through to ProseMirror's `baseKeymap`.
- The generic paragraph first-line-indent Backspace rule explicitly declines when the caret is inside a list, preventing it from becoming a competing list authority.
- 1a.14/1a.15 list/paragraph boundary CSS compensation was removed.
- The em-dash input/continuation/exit behavior remains unchanged.
- Toolbar Normal text and same-type list toggles use native `liftListItem`.
- Cross-type Bullet/Numbered/Checklist conversion remains explicit Rustnote product policy because the schema uses both `list_item` and `task_item`, and ordered lists carry Rustnote style/restart attributes.

## Guards

Static contracts now fail if custom list Backspace/Delete handling, manual list reconstruction/join logic, or list-boundary CSS compensation is reintroduced. The A3 ownership guard also requires the native list command chain rather than the removed custom Backspace function.

## QA executed in this environment

- Dependency-free browser/editor/PDF contract suite: 19/19 passed.
- A3 modular-source static validation: passed.
- Changed JavaScript syntax checks: passed.
- Shell syntax for the changed architecture guard: passed under Bash; the shipped scripts retain their macOS zsh shebangs.

The real ProseMirror behavior suite is updated to test native ownership rather than Rustnote-specific Backspace/Delete tree shapes. It cannot execute in this sandbox because `editor-build/node_modules` is intentionally not shipped in the source archive and the sandbox cannot resolve the npm registry. `prepare-rustnote-mac.sh` installs the pinned dependencies on the target Mac before the normal validation sequence.

## Acceptance principle

List editing must no longer depend on a Rustnote Backspace/Delete state machine. If ProseMirror changes the document for an ordinary list-boundary Backspace/Delete, Rustnote allows that transformation. Rustnote intervenes only for independent product features, not to reinterpret native list boundaries.

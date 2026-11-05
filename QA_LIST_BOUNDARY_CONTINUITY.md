# List Boundary Continuity — v0.12.5 build 1a.20

## Objective

Close the remaining list-boundary oscillation without restoring a second list engine:

1. Backspace at the start of a list item uses native `liftListItem`.
2. A second Backspace at the start of the resulting paragraph merges text with the preceding list item's textblock without recreating an independent list-owned paragraph.
3. Explicitly reapplying Bullet/Numbered/Checklist beside compatible list fragments rejoins them instead of creating list islands.
4. Ordered-list fragments preserve visible numbering while an item is temporarily outside the list, and re-listing restores one continuous sequence.

## Implementation

- `backspaceListItemAtStart` remains the only list-item Backspace exit rule and delegates structure to ProseMirror `liftListItem`.
- `backspaceParagraphAfterList` handles only an ordinary paragraph at offset 0 immediately following a list. It delegates to ProseMirror `joinTextblockBackward`, wrapped in `autoJoin` so compatible list fragments reunite after the textblocks merge.
- All other Backspace behavior falls through to ProseMirror's base keymap.
- `normalizeOrderedTrailingFragment` is metadata-only policy applied after an outer ordered-list lift. The trailing fragment receives the number it had before the split and is marked as continuation rather than a new explicit restart.
- Explicit list application uses native `wrapInList` wrapped in `autoJoin`. The new list adopts compatible adjacent bullet marker or ordered style/order metadata so adjacent fragments can become one list.
- Explicit `restart=true` on a following ordered list remains a hard boundary.
- No manual list tree reconstruction, `ReplaceAroundStep`, direct `joinBackward`, or custom Delete behavior is introduced.
- Em-dash behavior is unchanged.

## Behavioral invariants

- Backspace may delete text, merge textblocks, remove list membership, or outdent a nested item; it never creates a new independent bullet/numbered/checklist item.
- Reapplying the same list type to a paragraph between compatible fragments extends/rejoins that list rather than creating an isolated list node.
- Ordered-list split/rejoin preserves style, starting order, restart intent on the surviving left fragment, and visible continuation numbering on the trailing fragment.

## QA

- Dependency-free browser/editor/PDF suite: 20/20 passed.
- Canonical editor-document contract: 36/36 cases passed during architecture validation.
- B4 behavior guard reports 58 JS behavior tests and 108 Rust regression tests discovered.
- A1-A7/B1-B4 static architecture chain passed before the cumulative wrapper exceeded the sandbox execution window.
- B5 static-specific guards and PDF behavior suite passed; B6 typed-error guard passed.
- First-party JavaScript syntax checks passed.
- Dependency-backed ProseMirror tests were expanded to assert exact tree shapes for `Backspace -> Backspace`, explicit re-listing, ordered continuation, and explicit restart boundaries across bullet/numbered/checklist variants.

The dependency-backed ProseMirror suite cannot execute in this sandbox because the source archive does not ship `editor-build/node_modules` and outbound npm resolution times out. It is expected to run after `prepare-rustnote-mac.sh` on the target Mac.

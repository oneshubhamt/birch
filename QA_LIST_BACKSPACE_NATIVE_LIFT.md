# List Backspace Native-Lift Correction — v0.12.5 build 1a.19

> **Superseded by 1a.20:** 1a.19 fixed the first Backspace. 1a.20 additionally handles the second Backspace with native `joinTextblockBackward` and adjacency-aware native list wrapping.

## Objective

Correct the middle-list-item Backspace path without restoring Rustnote's former list tree-surgery engine.

## Root cause

Since the native-list simplification, Backspace was not bound by Rustnote's list keymap. At the start of a middle list item, ProseMirror's generic `baseKeymap` could therefore run `joinBackward`. Given Rustnote's valid `list_item` shape (`paragraph table_cell_block*`), that generic join could move the paragraph into the preceding list item as an additional block. The bullet disappeared visually, but the paragraph remained structurally inside the list and therefore retained list-container indentation.

The 1a.18 post-lift indent reset was correct for explicit `liftListItem` paths, but Backspace never reached that path.

## Correction

`list_system.js` now adds exactly one guarded Backspace policy:

- selection must be an empty `TextSelection`;
- caret must be at textblock offset 0;
- caret must be in the first child of the nearest `list_item` or `task_item`;
- only then does Backspace call the existing `liftSelectedListItems` authority.

`liftSelectedListItems` still delegates structure to ProseMirror's native `liftListItem` command. Rustnote only applies the existing post-lift policy that resets `leftIndent`, `rightIndent`, and `firstLineIndent` when a textblock actually leaves every list.

Backspace elsewhere is not intercepted and falls through to ordinary editor/browser handling. Delete remains entirely uncustomized. No manual list reconstruction, joins, `ReplaceAroundStep`, ordered-list continuation repair, or list-boundary CSS rules were added.

## Behavioral contract

For bullet, numbered, and checklist lists:

```text
• First
• |Middle
• Last
```

Backspace must produce a real outer-list exit:

```text
• First
Middle
• Last
```

The resulting middle paragraph must have zero left/right/first-line paragraph indent while preserving unrelated paragraph formatting such as line spacing and text alignment.

Nested Backspace uses the same native lift and therefore outdents by one list level; paragraph indent attributes are only reset if the textblock actually leaves all lists.

## QA

- JavaScript syntax checks pass for the changed editor and behavior tests.
- Dependency-free browser/editor/PDF suite: 20/20 passed.
- Static cumulative architecture chain through B6 passed.
- Canonical editor-document contract: 36/36 cases passed during the B6 chain.
- Rust regression inventory discovered by the architecture guards: 108 tests.
- Real dependency-backed ProseMirror tests are present for bullet/numbered/checklist middle-item Backspace, but cannot execute in this sandbox because the source archive intentionally does not ship `editor-build/node_modules` and `npm ci` cannot complete here. `prepare-rustnote-mac.sh` will install the pinned dependencies on the target Mac.

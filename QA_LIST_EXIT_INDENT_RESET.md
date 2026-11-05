# List exit indentation reset — v0.12.5 build 1a.18

## Audit finding

The native-list simplification in 1a.16 correctly removed Rustnote's competing list structure engine, but it also removed one legitimate product policy: when an outer list item is converted back to normal text, paragraph indentation must return to normal. ProseMirror's native `liftListItem` preserves the paragraph node and therefore correctly preserves its attributes; Rustnote's `leftIndent`, `rightIndent`, and `firstLineIndent` attributes consequently survived the lift and became visible once the paragraph was no longer rendered inside a list.

The dependency-backed behavior tests had encoded that preservation as expected behavior, so the regression was specification/test-driven rather than an upstream list-structure failure.

## Correction

- ProseMirror remains the sole owner of list structure through native `liftListItem`.
- `liftSelectedListItems` captures the native transaction and performs only a post-lift paragraph-layout policy.
- Textblocks that actually end up outside every list have `leftIndent`, `rightIndent`, and `firstLineIndent` reset to zero.
- Nested outdent does not reset indentation because the textblock remains inside a list.
- Explicit line spacing, text alignment, marks, drop-cap state, and other paragraph attributes are not rewritten.
- Toolbar list toggle-off, Normal text, and Shift-Tab share this same lift authority.
- No custom list Backspace/Delete handling, join logic, tree reconstruction, or list-boundary CSS compensation was reintroduced.

## Behavioral coverage

The dependency-backed ProseMirror matrix now requires outer-list exit indentation reset for bullet, numbered, and checklist lists, verifies the middle-item split case, verifies non-indent paragraph layout survives the lift, and retains the nested-outdent preservation test.

The dependency-free UI contract requires Shift-Tab to use the same lift authority and requires the post-lift policy to reset only the three paragraph indent attributes.

## QA in this environment

- Dependency-free editor/note-list/PDF contract suite: 20/20 passed.
- A3 modular-source static validation: passed.
- Cumulative architecture static chain through B6: all substantive guards passed; the first wrapper invocation returned non-zero only because this Linux sandbox lacks a normal TERM for the final terminal-clear operation after the success messages.
- JavaScript syntax for the changed source/tests: passed.
- Rust regression tests discovered by architecture guards: 108.

The source archive intentionally does not ship `editor-build/node_modules`, and registry DNS is unavailable in this sandbox. Therefore the dependency-backed ProseMirror behavior matrix cannot execute here. `scripts/prepare-rustnote-mac.sh` installs the pinned dependencies on the target Mac, after which the normal validation sequence executes it.

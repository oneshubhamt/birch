# Rustnote 2b.41 — UI Task Scope Ownership Plan

## Audit finding

The 2b.40 environment run reports Dioxus `__copy_value_hoisted` warnings. Interactive UI futures were started with `dioxus_core::spawn_forever`, which always attaches the task to Dioxus's root scope. Those futures capture `Signal` values created by `RunningApp`, so reads/writes occur from a scope that can outlive the signal owner.

This is a real lifetime defect, not a cosmetic warning.

## Objectives

1. Keep UI workflow tasks alive across NoteCard/EditorPane remounts.
2. Do not hoist `RunningApp`-owned Signals into the Dioxus root scope.
3. Cancel those workflows when `RunningApp` itself is replaced.
4. Remove every `spawn_forever` from `src/ui`.
5. Preserve 2b.40 selected-note-only Vault actions and all privacy behavior.
6. Add static/browser regression guards for the ownership contract.
7. Re-run inherited Vault/E0–E3 closure and targeted browser QA.

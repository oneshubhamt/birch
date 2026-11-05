# Vault 2b.1–2b.4 Final Objective Closure — Audit & Plan

Baseline: Rustnote v0.12.5 build 2b.5.1.

This pass closes only defects found by a de-novo audit against the original 2b contract.

## Audit findings

1. Protected mutation callers still manually compose Vault access before the storage operation lock.
2. Protected async publication gaps remain in tag mutation, revision-limit refresh, attachment removal/status, and export completion/status paths.
3. Tag command APIs (`remove_tag_from_note`, global rename/delete) still operate only on ordinary `note_tags`, bypassing encrypted `MetadataState` for protected notes.
4. Existing static validators do not reproduce these failures.

## Objectives

- Introduce one coordinator-owned protected mutation guard that owns Vault authorization + storage serialization ordering.
- Migrate all production protected mutation callers that currently know that ordering.
- Route protected tag command mutations through encrypted `MetadataState`; never recreate protected tag relationships in plaintext `note_tags`.
- Define fail-closed semantics for global tag rename/delete when affected protected notes are locked.
- Lease-protect all identified async protected-derived publications and perform a whole-UI follow-up sweep.
- Add direct Rust/static regressions for each defect class.
- Re-audit source after implementation, run all available security/architecture/behavior QA, then package only the exact validated tree.

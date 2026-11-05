# Rustnote Vault 2b.42.1 — Scoped Publication Core

## Scope

This pass closes only the privacy/publication authority primitives. It does not attempt the later 2b.42 navigation, note-local UI, multi-note UI, Vault-controller, library-operation, profiling, VACUUM, or final-closure passes.

## Coordinator model

`PrivacyCoordinator` now has three publication dimensions:

- one whole-library publication generation;
- one multi-note publication generation;
- one publication generation per `NoteId`.

Publication leases are tracked separately for multi-note surfaces and per note.

A note transition for B advances B's note generation and the multi-note generation, then drains only:

- already-started B publication leases; and
- already-started multi-note publication leases.

It does not advance A's generation and does not drain A's note-local leases.

A whole-library Restore advances the library and multi-note generations and drains every note-local and multi-note publication lease.

The exceptional protected revision-Restore admission path follows the same affected-note + multi-note publication semantics while retaining its required reader-before-transition ordering.

## Application/storage API

The publication API is explicit:

- `capture_note(note_id)` for one note;
- `capture_multi_note()` for a surface or operation that may contain several notes.

No generic/global capture API is retained in these authority layers.

## UI immediate revocation model

`UiPrivacyState` mirrors the coordinator scopes:

- library generation/revocation count;
- multi-note generation/revocation count;
- per-note generation/revocation counts.

State changes are applied through one Dioxus `Signal::write()` guard rather than clone-modify-set, avoiding lost updates if privacy actions overlap.

Generations advance at revocation admission. Guard release only decrements the corresponding active-revocation count; it does not advance the generation again. This keeps pre-transition work stale without causing a redundant second invalidation when the fail-closed window closes.

## 2b.11 invariant

The pass deliberately preserves the 2b.11 rule: a compound plaintext operation admitted before Lock/Protect remains authorized to finish while the later transition waits. The mutation step does not re-enter admission or perform a post-admission authorization recheck.

## Explicit non-goals

Deferred to later passes:

- 2b.42.2 source/target navigation authorization;
- 2b.42.3 note-local surface migration;
- 2b.42.4 multi-note surface migration;
- 2b.42.5 Vault-action controller;
- 2b.42.6 library-wide workflows;
- profiling/performance and no-VACUUM forensic work;
- broad dead-code/final closure work.

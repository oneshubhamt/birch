# Vault 2b.42.9 — Dead code + architecture cleanup

## Scope

Behavior-preserving cleanup only. This pass removes obsolete/development-era compatibility and duplicated architecture after the scoped privacy model, profiling work, and no-VACUUM forensic closure. It does not redesign Vault behavior, Restore ordering, publication scope, backup privacy semantics, attachment lifetime, encryption, or storage mutation ordering.

## Cleanup completed

- Removed production `dead_code` lint suppressions rather than replacing them with `allow`/`expect` exceptions.
- Kept parse-only editor specification fields under strict serde validation by renaming them as explicitly ignored Rust fields.
- Removed the exceptional `DurablePlaintextNamespace::adopt_existing_with_proof` API and nonempty-root adoption mode. Fresh establishment remains fail-closed when durable authority is missing.
- Removed the superseded inline backup-owner convention and one-way repository adoption path. The external durable namespace marker is now the only repository ownership authority.
- No backward-compatibility migration paths retained for backup-location history: only the current identity-bearing format is accepted; old v1/v2 and empty-library-id fallback/adoption are gone.
- Tightened current backup/history JSON parsing with `deny_unknown_fields` and removed compatibility defaults that silently accepted incomplete older shapes.
- Current crash recovery preserved: current-format backup history can still recover the same library identity for the same data path when the library bootstrap is pristine; mutable repository index recovery remains derived from verified current manifests under the external owner.
- Folded the one-use current-editor publication capture helper into `EditorSynchronizationContext`.
- Replaced stale inherited publication/privacy validators with compact current scoped-authority checks while preserving their historical entry-point names for QA orchestration.
- Removed stale pass-number commentary from production source.
- Preserved the 2b.42.8 production residue sequence: `secure_delete + FTS rebuild + verified WAL TRUNCATE`, with no per-Protect VACUUM.

## Architectural result

Current Rustnote has one canonical model for each affected authority:

- durable managed-plaintext namespace: marker must pre-exist any nonempty managed root;
- automatic backup repository ownership: external durable marker only;
- backup-location history: current version + nonempty library identity only;
- UI privacy/publication: library / multi-note / note-local scoped authority only;
- current-editor publication capture: owned by `EditorSynchronizationContext`.

No migration/adoption branch remains merely to support earlier development builds.

## Explicitly preserved

- fail-closed ownership checks;
- offline/pending backup privacy obligations;
- interrupted repository purge recovery;
- missing mutable repository-index reconstruction under verified durable ownership;
- library identity recovery for a pristine current-format bootstrap;
- note-scoped and multi-note publication semantics from 2b.42.1–2b.42.6;
- 2b.42.8 no-VACUUM forensic security decision.

## Native closure gate

Native Rust compilation, strict Clippy, and the full Rust test suite must pass on macOS before 2b.42.9 is considered closed and before 2b.42.10 begins.

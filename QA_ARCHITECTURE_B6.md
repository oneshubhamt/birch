# Architecture B6 — typed-error closure + storage-owned search translation

## Scope

B6 closes the broad string-error architectural gap as far as it is useful without replacing
human-readable diagnostics with ceremonial wrapper types. It also closes the residual search/SQLite
ownership leak identified after B5.

The pass has two rules:

1. subsystem/application control flow must carry typed errors rather than `Result<_, String>`;
2. final presentation text may remain `String`, and typed variants may contain message detail where
   the message itself is the domain diagnostic.

B6 does not attempt to turn every validation sentence or external-tool stderr value into a separate
enum variant. The architecture boundary, not a zero-`String` source metric, is the objective.

## Typed-error boundary

The B6 validator performs a generic-aware scan over first-party Rust source and rejects any direct
`Result<_, String>` error type. This distinguishes a string *error* from valid success values such as
`Result<Vec<String>, DataAccessError>`.

The B5 audit's grep-based inventory was roughly 157 explicit string-result signatures; a stricter
balanced-generic scan of the B5 source finds additional local/multiline forms. B6's closure invariant
is simpler and stronger: the direct string-error count is now **zero**.

The application executor also no longer implements `ExecutionFailure for String`, so worker code
cannot erase a domain error merely to cross the background-execution boundary.

Typed errors now include or preserve these stable categories:

- `DocumentValidationError` for canonical editor-document validation;
- `AutosaveError` and `EmergencyJournalError` for autosave/journal control flow;
- `PlatformError` and `NativeError` for OS/native operations;
- `MediaCompatibilityError` for compatibility planning/preparation;
- `LibraryRuntimeError` and `ApplicationOpenError` for library lifecycle/startup;
- `EditorBridgeDecodeError` for versioned bridge decoding;
- `SynchronizationError` for live-editor save/switch choreography;
- `NoteIdError` and `SourceIdError` for typed domain identifier validation;
- existing `EditorError`, `AttachmentError`, `ExportError`, and `EditorWorkflowError` application
  boundaries;
- `DataAccessError` for thin application data capabilities.

Underlying source errors remain attached with `Error::source` where the underlying subsystem exposes
one. Message-only states remain message-bearing variants inside a typed error instead of being
promoted into dozens of one-use enum cases.

## Application data boundary

B6 initially exposed an important regression during cumulative QA: typing UI worker failures by
importing `StorageError` directly would have reopened the B1/A4 storage boundary.

The final architecture therefore uses one application-owned wrapper:

```text
StorageError
    ↓
DataAccessError
    ↓
LibraryService / NoteService / SettingsService / RuntimeService
    ↓
DataTaskError (+ ApplicationExecutionError)
    ↓
UI presentation
```

Those four services no longer return `StorageResult`, and `src/ui/` contains no `crate::storage`,
`StorageError`, or `StorageResult` dependency.

This deliberately avoids four near-identical service error enums while keeping storage implementation
errors behind the application boundary.

A UI-only persistence-listener restart condition was also moved into `EditorError` as
`PersistenceListenerRestarted`, removing the temporary direct UI dependency on `AutosaveError`.

## Search/SQL ownership

`src/search.rs` now owns only storage-neutral search semantics:

- tokenization;
- parsed groups/terms;
- field semantics;
- highlighting/matching behavior.

SQLite FTS5 translation lives in `src/storage/search_query.rs`:

- FTS5 column mapping;
- term escaping/phrasing;
- group expressions;
- `MATCH` expressions;
- candidate SQL predicates and bound parameter numbering.

Storage consumers call this canonical translation module from `storage/notes.rs` and
`storage/note_candidates.rs`. `search.rs` contains no `notes_fts`, `MATCH`, SQL candidate predicate,
or FTS column-expression knowledge.

Two storage-local tests cover mixed positive/negative translation and the all-negative indexed
`NOT IN` branch.

## Typed identifier coverage

`NoteId::parse` and `SourceId::parse` now return `NoteIdError` / `SourceIdError` rather than strings.
Their `TryFrom` implementations and serde validation preserve the typed category while retaining the
same displayed validation message.

Two regression tests verify empty/control-character rejection and valid identifier acceptance.

These two new public domain error declarations increase the A6 domain declaration count from 41 to
43. The A6 guard was corrected so it no longer freezes an unrelated exact declaration count: it
requires the original baseline to remain present and explicitly requires `NoteIdError` and
`SourceIdError`.

## Behavioral preservation

B6 changes error representation and ownership, not normal workflow order. Existing display text is
kept at the final user-facing boundaries. Particular care was taken around:

- autosave pending/target combined failures;
- emergency-journal worker/scheduler failures;
- editor synchronization generation/session failures;
- startup/library-open failures;
- native/platform/media failures;
- note-switch target-load failure presentation;
- settings/revision/background task presentation.

Cumulative B1 validation is part of B6 acceptance specifically to prevent typed-error work from
reintroducing UI → storage/native/autosave implementation dependencies.

## Regression coverage

Rust test inventory in the source tree is now **69**, up from 65 at B5. B6 adds:

- two storage-owned FTS translation tests;
- typed `NoteId` validation coverage;
- typed `SourceId` validation coverage.

Existing B2/B4/B5 suites remain cumulative:

- 29 shared editor-document contract cases;
- 16 first-party JS behavioral tests in total;
- 8 dependency-free PDF behavioral tests;
- B3 workflow architecture/ordering guards;
- B1/A0–A7 architecture guards.

## Acceptance

Full acceptance on macOS:

```bash
cd /Users/shubhamtiwari/Developer/rustnote
./scripts/validate-architecture-b6.sh
```

If dependencies/toolchains are not prepared:

```bash
./scripts/prepare-rustnote-mac.sh
./scripts/validate-architecture-b6.sh
```

Source/static acceptance:

```bash
RUSTNOTE_B6_STATIC_ONLY=1 ./scripts/validate-architecture-b6.sh
```

The B6 validator checks the typed-error closure, application/storage boundary, storage-owned FTS
translation, Rust regression inventory, JavaScript/shell syntax, dependency-free PDF behavior, and
the cumulative B5 → B1 → A0–A7 static chain. Full mode then runs the complete B5 prepared-runtime,
Rust, and dependency-backed editor acceptance suite before rechecking B6.

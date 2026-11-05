# Architecture B2 QA — canonical editor-document specification

## Objective

B2 removes independently maintained Rust/JavaScript definitions of the canonical editor format while preserving independent runtime validation on both sides of the trust boundary.

The required architecture is:

```text
        editor-spec/document-spec.json
                    │
          ┌─────────┴─────────┐
          ▼                   ▼
   Rust spec adapter      JS spec adapter
          │                   │
   Rust validator        JS validator
    independent           independent
          │                   │
          └──── shared contract corpus ────┘
```

## Canonical authority

`editor-spec/document-spec.json` owns:

- envelope identifiers and versions;
- 21 node types and 14 mark types;
- contexts and child-context relationships;
- permitted/required attributes and `strict` / `known` / `passthrough` policy;
- defaults used by ProseMirror schema construction;
- enum domains and numeric/UTF-8-byte/item limits;
- ProseMirror content/group/inline/marks/excludes metadata;
- validator-class assignment;
- structural invariant assignment;
- list/table/scientific/attachment rule parameters;
- pending-attachment uniqueness key;
- time-based attachment-kind domain;
- media-annotation canonical sort order.

Rust embeds this exact file with `include_str!`. JavaScript imports this exact file. The production editor build includes it in the runtime source hash and bundles its data locally. The editor-specific `note_id_max_bytes` projection is explicitly contract-checked against the independent Rust `NoteId::MAX_LEN` domain constraint so the shared JS format contract cannot silently drift from Rustnote's domain model.

## Independent validation

B2 does **not** make Rust trust JavaScript.

`src/editor_document.rs` and `editor-src/document/validation.js` retain separate validation algorithms. The specification determines the vocabulary, contexts, attributes, bounds, validator classes, invariant assignment, and rule parameters those algorithms must implement.

Both sides reject unsupported validator classes or invariant-contract mismatches. Rust's specification parser also uses `serde(deny_unknown_fields)` so a misspelled canonical field cannot be silently ignored by the trust-boundary implementation.

## Shared behavioral contract

`editor-spec/validation-cases.json` contains 29 cases and is consumed independently by:

- `scripts/validate-editor-document-contract.mjs`; and
- the Rust `shared_editor_document_contract_corpus_matches_rust_validator` test.

The valid corpus exercises all 21 node types and all 14 mark types. Cases cover envelope drift, vocabulary, contexts, strict/known/passthrough attributes, bounds, UTF-8 byte limits, C1 controls, mark exclusivity, scientific restrictions, pending IDs, attachment/media constraints, annotation ordering/uniqueness, and table structure/geometry.

## Intentional cross-runtime harmonization

The de novo B2 comparison found three pre-existing JavaScript/Rust disagreements. B2 resolves them in favor of the Rust persistence/trust-boundary semantics and records the result in the shared authority/corpus:

1. attachment `mediaType` is optional/nullable;
2. explicit `sizeMode: null` is invalid (omission uses the canonical default);
3. canonical string limits are UTF-8 byte limits rather than JavaScript UTF-16 code-unit limits.

These are contract synchronization fixes, not new editor features.

## Static acceptance

`./scripts/validate-architecture-b2.sh` checks:

- canonical-spec integrity and references;
- exact Rust/JS node and mark validator-class coverage;
- explicit invariant-class coverage;
- ProseMirror vocabulary equality with the specification;
- Rust embedding and strict spec parsing;
- absence of legacy Rust schema constants and name-based JS/Rust schema switches;
- build-manifest/spec hashing ownership;
- the editor `note_id_max_bytes` projection against Rust `NoteId::MAX_LEN`;
- bundled blank/welcome envelope agreement;
- the 29-case independent JavaScript contract corpus;
- editor/PDF/build-script JavaScript syntax;
- shell-script syntax;
- cumulative A1–A7 and B1 architecture guards.

With `RUSTNOTE_B2_STATIC_ONLY=1`, Rust/build acceptance is skipped for environments without the pinned toolchain/dependencies.

## Full acceptance

Without `RUSTNOTE_B2_STATIC_ONLY`, the B2 validator runs the cumulative B1/A0–A7 acceptance chain. That chain rebuilds and smoke-tests the prepared ProseMirror runtime and performs final `cargo fmt`, strict all-target/all-feature Clippy, and all-feature workspace tests.

B2 is accepted only when the canonical authority, both independent validators, the shared corpus, the prepared editor runtime, and cumulative Rust QA agree.

# Rustnote editor-document specification

`document-spec.json` is the single authoritative definition of Rustnote's canonical editor-document format.

It owns the stable envelope identifiers and versions, node and mark vocabulary, document contexts, attribute policies and defaults, value domains and limits, ProseMirror grammar metadata, validator-class assignment, and the structural-invariant contract attached to each validator class.

## Runtime model

The authority is shared; validation execution is not.

- Rust embeds the exact JSON file in `src/editor_document_spec.rs` and validates untrusted/persisted editor state independently in `src/editor_document.rs`.
- JavaScript imports the exact JSON file through `editor-src/document/spec.js` and validates independently in `editor-src/document/validation.js`.
- ProseMirror schema construction derives vocabulary, attributes/defaults, content expressions, groups, inline status, and mark exclusions from the same specification.
- Neither runtime calls or trusts the other runtime's validator.
- `note_id_max_bytes` is an intentional projection of Rustnote's general `NoteId` domain rule; B2 QA and Rust tests require it to equal `NoteId::MAX_LEN`.

A `validator` name selects an independently implemented Rust/JavaScript semantic validator. Each rule's `invariants` array states the structural behavior that validator class is required to implement. Both runtimes reject a specification/implementation mismatch rather than silently applying a different contract.

## Change discipline

A document-format change must update the canonical JSON first. Do not add parallel node/mark/domain/version tables to Rust or JavaScript.

Any intentional contract change should also update `validation-cases.json`. That corpus is executed independently by JavaScript and Rust and must retain valid coverage of every canonical node and mark type.

Run:

```bash
./scripts/validate-architecture-b2.sh
```

before accepting a specification change. The production editor remains a single optimized local bundle; the JSON authority is bundled rather than fetched at runtime.

## Paragraph-layout contract (document schema v6)

Document schema version 6 extends the canonical paragraph-layout attributes on `paragraph` and
`heading` with `textAlignment` (`left`, `center`, `right`, or `justify`) alongside `leftIndent`,
`rightIndent`, `firstLineIndent`, and `dropCap`.

- Indents are persisted as integer typographic points, never as CSS strings. Side indents are
  bounded to 0–144 pt; first-line indent is bounded to 0–72 pt.
- `dropCap` is a boolean presentation semantic. It is mutually exclusive with a non-zero
  `firstLineIndent`, so the renderer never has to reconcile two competing first-line layouts.
- Atomic boundary paragraphs must retain default layout values. List nesting, code blocks, and
  table structure remain separate semantics rather than overloading paragraph indentation.
- Indent and drop-cap DOM import recognizes only Rustnote's `data-*` layout attributes. Native
  `text-align` is also accepted as the standard semantic representation of paragraph alignment;
  arbitrary margin/padding indentation is still not promoted into canonical document state.
- The schema-version bump is intentional. Rustnote is still in production development, so this
  pass does not add a legacy migration layer; all first-party seed/test documents move to v6.

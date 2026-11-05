# Rustnote v0.12.5 build 2b.26 — Autosave Baseline + Validator Closure Plan

## Audit findings

1. `protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only` asserted that an Autosave document-link mutation could not append a revision, but the test had no pre-existing protected revision. Rustnote intentionally records the first Autosave revision when no head exists, matching ordinary-note revision policy.
2. The inherited 2b.23 static guard matched an exact single-line `assert_eq!` expression. `cargo fmt` may wrap that expression, causing a false validator failure on a correctly formatted source tree.
3. The E2 validator admitted builds through 2b.25 at its shell gate, but its schema-v21/current-sync Python branches stopped at 2b.21. Builds 2b.22+ did not change that schema architecture, so this was stale validator bookkeeping.

## Objectives

- Preserve the production first-Autosave revision policy.
- Make the metadata-locality test establish a real recent revision baseline before asserting Autosave interval throttling.
- Make the 2b.23 lifetime validator insensitive to legitimate Rust formatting without weakening ordering checks.
- Admit 2b.26 through inherited validators and keep E2 on the schema-v21/current-sync branch.
- Run inherited Vault/E0–E3 static validation, syntax QA, dependency-free behavioral QA, and exact archive integrity comparison.

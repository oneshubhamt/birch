# Rustnote v0.12.5 build 2b.26 — Autosave Baseline + Validator Closure Implementation

## Metadata-locality regression

The test now retains the `Note` returned by Protect and immediately records a protected checkpoint before source/citation/PDF-annotation/link locality assertions. It also asserts that the resulting graph has a revision head. The later link save still uses `RevisionMode::Autosave`, so its no-new-revision assertion now genuinely executes inside the five-minute revision interval.

No production revision logic was changed. In particular, protected Autosave still treats `revision_head == None` as due, matching ordinary-note `record_revision` behavior where the first Autosave establishes revision history.

## Format-robust inherited guard

The 2b.23 attachment roundtrip validator now uses multiline-aware regular expressions to locate attachment-byte verification, `drop(resource)`, and Unprotect. It continues to prove their ordering and still rejects rematerialization between resource release and Unprotect, but no longer depends on one exact `cargo fmt` layout.

## E2 validator bookkeeping

The schema-v21 and protected-sync branches now include builds 2b.22 through 2b.26. There is no schema migration or production storage change.

## Build identity

`build-id.txt` is `2b.26`. Inherited validator version gates were extended to admit the successor build.

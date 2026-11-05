# QA — Rustnote 2b.24 Tag Mutation + Transport Lifetime Closure

## Targeted QA

- Ordinary global tag rename/delete must not request an empty protected-note mutation guard.
- Ordinary tag mutations remain serialized on the authoritative storage mutex.
- The protected-note set is revalidated after storage authority is acquired.
- Protected global tag operations still require complete unlocked authorization.
- The protected PDF transport lifetime test must not call the blocking attachment read path while Lock is admitted and its original resource is still alive.
- The test must prove revocation, prove Lock is still waiting, release the resource, then require Lock completion.
- Metadata-locality diagnostics must remain assertion-only changes.

## Native macOS gate

Run the three ordinary tag regressions, then the protected transport lifetime regression. Run `protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only` separately with `--nocapture` to obtain the exact invariant if it remains failing. Only after those are clean should the serial full workspace suite resume.

## Sandbox limitation

Cargo/rustc are unavailable in this environment. Static/security validators, dependency-free behavioral tests, shell syntax, source-integrity checks, and archive comparison are available here; native Rust compilation and execution remain authoritative on macOS.

## Sandbox verification completed

- Targeted 2b.24 tag/transport guard: PASS.
- Inherited 2b.23 through 2b.11 and E0-E3 2a.16-2a.19 closure chain: PASS.
- Vault 2b.1 through 2b.10 validators: PASS individually.
- Dependency-free browser/editor/PDF behavioral suite: 36/36 PASS.
- Shell validation scripts: syntax PASS.
- JavaScript/MJS scripts: syntax PASS.
- Changed Rust files (`tags.rs`, `transport.rs`, `protection.rs`): lexical delimiter integrity PASS.
- Core validators that invoke Cargo cannot execute in the sandbox because Cargo is unavailable; this is an environment limitation, not counted as a pass.

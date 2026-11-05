# Rustnote Vault 2b.1 — QA and closure record

Build: `0.12.5 / 2b.1`
Baseline: `0.12.5 / 2a.19`

## Objective verification

1. **Canonical `SensitiveNoteData` authority** — PASS. Domain fields are exhaustively classified and the logical aggregate is deliberately non-serializable.
2. **Explicit public protected-note allowlist** — PASS. Public, sanitized, forbidden and special relational surfaces are declared centrally.
3. **Final granular object vocabulary** — PASS. Manifest, note state, metadata, attachment index, immutable revision object and opaque object ID are defined.
4. **Authenticated identity/context rules** — PASS. Note identity + object kind + object ID + payload format version are bound into AAD, with a kind-specific Vault domain.
5. **SQLite encrypted small-object foundation** — PASS. Schema 19 adds only ciphertext object storage and the minimal root locator/envelope.
6. **Manifest/root primitive** — PASS. Exact manifest ciphertext is authenticated before root installation; SQLite enforces same-note manifest existence.
7. **Independent revision format** — PASS. Immutable encrypted predecessor chain uses head/floor/count so normal root commits are O(1) in history length.
8. **Independent attachment-index format** — PASS. Sensitive current attachment metadata and plaintext↔ciphertext blob mappings live in one encrypted object vocabulary.
9. **No plaintext shadow representation** — PASS for the new foundation. New SQLite tables persist only routing identifiers, root format/envelope and ciphertext.
10. **No production dual writer** — PASS. 2a.19 remains the sole production persistence path until 2b.4.

## Targeted foundation/adversarial validation

`bash scripts/validate-vault-2b1-foundation.sh` — PASS.

The validator checks, among other things:

- fixed build/schema/object vocabulary;
- non-serializable logical sensitive aggregate;
- exhaustive `Note`/`NoteDocument` classification;
- exact public/sanitized SQLite column classification;
- direct and transitively note-owned SQLite tables are explicitly classified;
- opaque object IDs;
- object AAD kind/note/object/version binding;
- schema-18 → schema-19 structural equivalence with fresh schema 19;
- no plaintext kind/version/timestamp metadata in `vault_objects`;
- same-note foreign-key root integrity;
- executable SQLite rejection of a root whose manifest is absent;
- O(1)-in-history manifest/root structure;
- revision-chain head/floor/count semantics;
- temporary decrypted/serialized plaintext wiping markers;
- no new object-store writer/root usage in production callers before 2b.4.

The required post-implementation audit found one design defect in the first draft: the manifest contained a full revision-object vector, making root installation O(revision history). It was replaced with the immutable predecessor-chain/head-floor-count design before closure.

## Inherited security regression validation

The final tree passes:

- E0 static contract;
- E1 static contract;
- E2 static contract;
- E3 static contract;
- 2a.16 canonical plaintext ownership guard;
- 2a.17 persistent residue/backup guard;
- 2a.18 UI privacy lifetime/revision-restore guard;
- 2a.19 independent adversarial closure guard;
- cumulative E0–E3 closure guard.

The inherited validators were changed only to accept build `2b.1` and schema version 19 where applicable; their substantive security checks were not weakened.

## Non-Rust regression validation available in this environment

- all shell scripts: `bash -n` PASS;
- first-party JS/MJS syntax: PASS (45 files);
- canonical editor document specification: PASS (21 nodes / 14 marks);
- editor document contract corpus: PASS (36 cases);
- dependency-free browser/editor/PDF behavior suite: PASS (36/36).

The two dependency-bound editor tests are unavailable because the editor-build npm dependencies (including `prosemirror-state`) are not installed in this sandbox; they fail at dependency resolution before application assertions execute.

## Native Rust validation limitation

This environment has no `cargo`, `rustc` or `rustfmt`. The new Rust unit tests are present in source but could not be compiled or executed here. No claim of native Rust-toolchain validation is made by this record.

## Pre-existing architecture-validator debt

Three broader architecture checks reproduce the same failure on the untouched 2a.19 baseline and are not caused by 2b.1:

- A6 expects 17 A5 native operations while the baseline already has 18;
- B1 reports the existing `src/ui/note_metadata.rs` direct storage reference;
- B6 reports the same pre-existing `note_metadata.rs` boundary reference.

Several chained architecture scripts also assume `zsh`, which is unavailable in this environment. 2b.1 did not expand scope to repair unrelated baseline validator debt.

## Closure verdict

Within the available source/static/non-Rust execution environment, the 2b.1 objectives are met and the production 2a.19 security path remains unchanged except for deduplicating shared sensitive DTO definitions and advancing the schema foundation. Native Rust compilation/tests remain an external release gate.

# QA — Rustnote Vault 2b.5 Final Consolidation & Performance Closure

## Environment boundary

This execution environment has no `cargo`, `rustc`, or `rustfmt`. Native Rust compilation, formatting, Clippy and Rust test execution were therefore **not executed** and are not represented as passed.

## Exact final-tree Vault guards

```text
validate-vault-2b1-foundation.sh          PASS
validate-vault-2b2-privacy-authority.sh   PASS
validate-vault-2b3-publication-token.sh   PASS
validate-vault-2b4-granular-cutover.sh    PASS
validate-vault-2b5-consolidation.sh       PASS
```

The 2b.5 guard was strengthened during this pass to require:

- removal of newly identified dead APIs;
- absence of old temp/PID compatibility layouts;
- current `session-v2` owner-lock cleanup;
- `attachment_ingest_jobs` through `OwnedPlaintext`;
- failed ingest cleanup preserving the durable job owner;
- centralized cleanup of untracked `.incoming` files;
- current startup recovery remaining present;
- no low-level privacy choreography outside the coordinator;
- FTS5 ordinary search;
- single-pass protected Unlock;
- mutation-local protected autosave;
- O(1)-with-history revision append;
- locality/streaming of protected attachment opening;
- targeted single-note Markdown export;
- one granular protected representation.

## Inherited E0–E3 closure

`./scripts/validate-security-e0-e3-closure.sh` on the final source reported:

```text
Security E0 static contract                    PASS
Security E1 static contract                    PASS
Security E2 static contract                    PASS
Security E3 static contract                    PASS
2a.16 canonical plaintext ownership            PASS
2a.17 persistent residue / backup closure      PASS
2a.18 UI privacy lifetime / revision restore   PASS
2a.19 independent adversarial closure          PASS
2a.19 cumulative closure                       PASS
```

One inherited E3 assertion still named the deleted PID-layout test. It was updated to require the stronger current invariant: v2 owner-lock crash cleanup is present and obsolete PID/temp-layout compatibility markers are absent. Production security behavior was not weakened or reintroduced.

## Architecture regression closure

`RUSTNOTE_B6_STATIC_ONLY=1 ./scripts/validate-architecture-b6.sh` passed the cumulative static chain:

```text
A1–A7 static architecture closure   PASS
B1–B6 static architecture closure   PASS
canonical editor document corpus    PASS
first-party JS syntax               PASS
shell syntax                        PASS
Rust tests discovered               254
```

The Rust tests are only discovered statically here; they are not executable without the Rust toolchain.

## Dependency-free behavior

`node scripts/run-browser-behavior-tests.mjs` on the final source:

```text
36 tests
36 passed
0 failed
```

This includes editor focus/boundary/list behavior, note-list cache safety, PDF search/annotation/cache/scheduling/layout, transport, navigation and preview lifecycle.

## Targeted 2b.5 adversarial source/test coverage

Current Rust source contains direct regressions for, among others:

```text
admitted_lock_blocks_new_reader_before_vault_revocation_begins
admitted_protect_blocks_new_plaintext_producer_before_derivative_drain
restore_is_a_same_note_conflicting_transition
failed_attachment_operation_rename_retains_predecessor_ownership
failed_pending_ingest_cleanup_keeps_durable_job_owner
protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only
large_protected_revision_history_does_not_expand_autosave_mutation_scope
unlocked protected note accepts encrypted attachment ingestion
pre-commit restart retains previous committed manifest root
post-commit restart exposes new committed manifest root
```

These tests are present and statically guarded; native execution remains part of the macOS gate.

## Final de-novo sweeps

### Protected representation

Production `src/` contains no active:

```text
ProtectedNotePayload
protected_notes
protected_note_blobs
vault.e3_privacy_scrub
pre-release schema migration ladder
```

### Temp compatibility

Production no longer contains:

```text
legacy_rustnote_temp_directory
rustnote-vault-* ownership parsing
PID-only session cleanup
old session-* cleanup
native process-liveness compatibility module
```

Current `session-v2-*` owner-lock/claim cleanup remains present and tested.

### Transition callers

Low-level Vault revocation/derivative transition assembly is confined to security internals and `PrivacyCoordinator`. Storage wrappers delegate to the coordinator; feature code does not reconstruct the old sequence.

### SQLite writer sweep

Encrypted object-table writers are confined to the Vault object/protection implementation. Plaintext `note_revisions` writes remain only for ordinary history/import/explicit Unprotect reconstruction; protected restore does not insert ordinary revisions.

### Sensitive filesystem sweep

No second current managed-plaintext ownership convention was found. `OwnedPlaintext` now covers durable staging, Vault operations, attachment operations, attachment ingest, Vault session and process-private lifetimes. Remaining raw storage/native filesystem operations were classified as ciphertext, backup/archive ownership, external export, directory creation, centralized privacy authority, or tests.

### Compatibility classification

Historical backup-repository ownership proof/removal remains intentionally present because it discharges recorded E3 plaintext-backup privacy obligations. It neither reads nor writes the obsolete protected-note representation.

## Performance source audit

| Surface | QA conclusion |
|---|---|
| Startup | no protected graph/history traversal |
| Ordinary note open | ordinary SQLite + cheap protection decision |
| Ordinary typing/autosave | ordinary SQLite/FTS after cheap decision |
| Ordinary search | FTS5; no Vault object read |
| Protected unlock | current graph exactly once; no history walk |
| Protected autosave | manifest + mutation-relevant object classes only |
| Revision creation | head-based append; no chain traversal |
| Revision restore | selected encrypted revision/current state only as required; no plaintext revision repopulation |
| Attachment ingestion | durably owned staging; specific blob encryption/index update |
| Attachment opening | manifest/index + one streamed encrypted blob |
| Lock | no encrypted graph/history read |
| Protect | one-time conversion work only; no dual steady-state representation |
| Unprotect | explicit one-time reconstruction; allowed to traverse protected history/attachments |

No runtime benchmark timings are claimed. Two algorithmically obvious regressions were corrected: duplicate current-graph decryption on Unlock and whole-library scanning for single-note Markdown export.

## Changed-Rust syntax sanity

Because `rustc` is unavailable, every changed Rust source file was also passed through an independent delimiter/string/comment-aware balance check. All changed Rust files were structurally balanced. This is a sanity check only and is not a substitute for compilation.

## Native macOS release gate — NOT EXECUTED HERE

```bash
cd ~/Developer/rustnote
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
./scripts/validate-security-e0-e3-closure.sh
./scripts/validate-vault-2b1-foundation.sh
./scripts/validate-vault-2b2-privacy-authority.sh
./scripts/validate-vault-2b3-publication-token.sh
./scripts/validate-vault-2b4-granular-cutover.sh
./scripts/validate-vault-2b5-consolidation.sh
```

Any native failure reopens 2b.5 until corrected and the complete gate is rerun.

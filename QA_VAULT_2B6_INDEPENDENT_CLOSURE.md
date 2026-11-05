# QA — Rustnote Vault 2b.6 Independent De-Novo Architectural & Security Closure

## Environment boundary

This execution environment provides Python and Node but no `cargo`, `rustc`, or `rustfmt`. Native Rust compilation, formatting, Clippy and Rust test execution were therefore **not executed** and are not represented as passed.

The environment also lacks a native `zsh`. The cumulative architecture scripts use a `#!/bin/zsh` runner but are Bash-compatible in the exercised static path; for this sandbox only, `/bin/zsh` was temporarily mapped to Bash so the existing validator chain could execute. No source behavior was changed for this workaround.

## 1. Independent 2b.6 guard

New guard:

```text
scripts/validate-vault-2b6-independent-closure.sh   PASS
```

It does not invoke the older Vault guards. It independently reconstructs and checks:

- I1/I2 typed ownership, pre-materialization reservation, `NotFound`-only absence proof and transactional durable retirement;
- I3/I4 coordinator authority, escaped low-level transition primitives, hard delete, import rollback and library restore;
- I5 publication-token authority and real publication leases, including fresh post-Restore authorization;
- I6 sensitive-data classification and protected SQLite/FTS scrub machinery;
- I7 mutation-local protected save, head-based revision append and object-store-owned mutable retirement;
- authenticated granular object vocabulary and absence of the old protected representation;
- snapshot/Vault-operation barrier;
- startup ownership/Vault recovery order;
- protected emergency-journal denial;
- executable canonical SQLite schema at version 21;
- high-risk adversarial test corpus for transition races, manifest crash/corruption, revision locality/restore, FTS/WAL residue, attachment crypto/authentication, recovery and protected↔unprotected cycles.

## 2. Exact final-tree Vault regression guards

Fail-fast execution on the final source:

```text
validate-vault-2b1-foundation.sh          PASS
validate-vault-2b2-privacy-authority.sh   PASS
validate-vault-2b3-publication-token.sh   PASS
validate-vault-2b4-granular-cutover.sh    PASS
validate-vault-2b5-consolidation.sh       PASS
validate-vault-2b6-independent-closure.sh PASS
```

The inherited 2b.2 guard was not weakened for 2b.6. Its 2b.6 branch now requires the stronger `ensure_path_absent()` / `path_entry_exists()` semantics and transactional owner-row retirement, and explicitly rejects a regression to `Path::exists()`.

## 3. Inherited E0–E3 cumulative security closure

`bash scripts/validate-security-e0-e3-closure.sh` on the final source:

```text
Security E0 static contract                    PASS
Security E1 static contract                    PASS
Security E2 static contract                    PASS
Security E3 static contract                    PASS
2a.16 canonical plaintext ownership            PASS
2a.17 persistent residue / backup closure      PASS
2a.18 UI privacy lifetime / revision restore   PASS
2a.19 independent adversarial closure          PASS
2a.19 cumulative independent closure           PASS
```

The inherited 2a.16 ownership guard was strengthened for 2b.6 for the same reason as the 2b.2 guard: the historical assertion expected `Path::exists()`, which is precisely the ambiguity the independent pass removed.

## 4. Cumulative architecture regression closure

Static architecture validation on the final source:

```text
A1–A7 static architecture closure   PASS
B1–B6 static architecture closure   PASS
canonical editor document corpus    PASS
first-party JavaScript syntax       PASS
shell syntax                        PASS
PDF dependency-free tests           PASS
Rust tests discovered statically    255
```

The discovered Rust tests are source coverage only. They were not executed without a Rust toolchain.

## 5. Dependency-free application behavior

`node scripts/run-browser-behavior-tests.mjs`:

```text
36 tests
36 passed
0 failed
```

Coverage includes editor focus/boundary/list behavior, note-list cache safety, PDF search/annotation/cache/scheduling/layout, PDF transport/navigation and preview lifecycle.

## 6. Targeted 2b.6 defect QA

### I2 metadata/absence failure

Production `src/security/plaintext.rs` before its test module contains no `.exists()` call.

Production `src/storage/plaintext_staging.rs` before its test module contains no `.exists()` call.

The new source regression:

```text
metadata_failure_is_not_treated_as_plaintext_absence
```

forces a metadata failure and requires the durable typed owner to remain unretired.

The independent validator additionally requires:

```text
symlink_metadata
NotFound-only absence
Immediate SQLite retirement transaction
absence proof before DELETE
owner retirement before transaction commit
```

### Encrypted-object retirement authority

The final `commit_vault_manifest_root` signature accepts no:

```text
superseded_mutable_objects
old_manifest: &VaultManifest
```

It re-reads the old manifest from the authenticated `root.manifest_object_id` and derives retirement from only:

```text
old/new NotePrivateState
old/new MetadataState
old/new AttachmentIndex
```

The independent validator fails if revision head/floor enters this normal mutable-object deletion authority.

## 7. Mandatory de-novo sweeps

### Direct SQLite writers

Search found no sensitive SQL writer outside `src/storage/`.

Vault object/root/ref writers remain storage-owned. Ordinary note/revision/tag/link/source writers remain valid ordinary/import/Unprotect code paths; protected steady-state persistence is still granular encrypted-object based.

### Direct sensitive filesystem writers

All reviewed production raw filesystem mutations were classified as:

- the canonical `OwnedPlaintext` / native privacy implementation;
- ciphertext/blob operations;
- backup/archive ownership machinery;
- explicit external export destinations;
- directory creation;
- non-production tests.

No second managed-sensitive-plaintext ownership convention was found.

### Transition callers

Search found no low-level Vault revocation or derivative-transition primitive outside `src/security/`.

### Raw `PathBuf` lifecycle

No production `.exists()` remains in the canonical plaintext owner or durable staging lifecycle. Transfer retirement uses typed owner identity plus conclusive absence proof.

### UI publication

Protected publication sites use `PrivacyPublicationToken`; actual decrypted-state handoffs acquire `begin_publication()`/`publish()` leases. Revision restore captures fresh post-transition authorization before editor publication.

### Old representation/dead code

No production marker remains for:

```text
ProtectedNotePayload
protected_notes
protected_note_blobs
```

No permanent dual protected-note model was found.

## 8. Crash/recovery and high-risk source coverage rechecked

The final source retains direct adversarial coverage for:

```text
same_note_transition_conflicts_fail_closed
library_and_note_transitions_are_mutually_exclusive
lock_waits_for_existing_reader_before_storage_mutation
protect_waits_for_plaintext_producer_and_blocks_new_producer
restore_invalidates_pre_restore_publication_and_allows_fresh_post_restore_authorization
vault_manifest_swap_is_rollback_safe_and_missing_component_fails_closed
large_protected_revision_history_does_not_expand_autosave_mutation_scope
protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables
protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot
persistent_snapshots_fail_closed_while_vault_operation_is_pending
protected_note_encrypts_attachment_blob_and_restores_it_losslessly
failed_protected_attachment_authentication_leaves_no_untracked_plaintext
vault_recovery_finishes_plaintext_after_unprotect_commit
vault_recovery_finishes_protection_after_database_commit
vault_recovery_rolls_back_prepared_unprotect_without_publishing_plaintext
protected_unprotected_protected_cycle_preserves_granular_state_and_residue_rules
```

These Rust tests are present and independently guarded but cannot be executed in this environment.

## 9. Final source-diff audit

Against the supplied 2b.5.3 baseline, substantive production changes are limited to:

```text
src/security/plaintext.rs
src/storage/plaintext_staging.rs
src/storage/protection.rs
src/storage/vault_objects.rs
```

No unrelated feature work was introduced. Validator changes are build-ID admission plus stronger 2b.6-specific assertions where the old assertion encoded the superseded `Path::exists()` behavior.

## 10. Native macOS release gate — NOT EXECUTED HERE

Run on the final source before release qualification:

```bash
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
./scripts/validate-vault-2b6-independent-closure.sh
```

Any native failure reopens 2b.6 until corrected and the complete gate is rerun.

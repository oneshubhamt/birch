# Rustnote Vault 2b.5 — Final Consolidation / Performance Audit Plan

## Baseline

`rustnote_v0.12.5_build_2b.5.2_2b1_2b4_objective_closure_source`

Governing scope: Pass 2b.5 of the original **Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model** contract. Earlier implementation reports and validators are evidence only, not proof.

## De-novo audit findings before edits

1. The steady-state protected representation is already granular (`vault_note_roots` + encrypted manifest/object graph); no production `ProtectedNotePayload`, `protected_notes`, or `protected_note_blobs` path remains.
2. `PrivacyCoordinator` now owns protected mutation ordering, revocation/drain, retained Restore mutation serialization, and publication authorization. Ordinary storage still correctly uses the same coordinator-owned storage mutex without Vault work.
3. Durable plaintext staging, Vault-operation ownership, attachment-operation ownership, Vault-session materialization, and process-private ownership are current. Several old public/private helper APIs are nevertheless unreferenced and should be deleted.
4. `scrub_protected_attachment_ingests_for_vault_unlocked` is a compatibility-only repair for a pre-barrier state that the current architecture cannot create: Protect checks pending ingest/journal/ledger state twice and its derivative transition prevents a new ingest entering during Protect. Retaining that repair contradicts the no-obsolete-compatibility objective.
5. Current Protect crash recovery still requires a conservative namespace scrub after a database-committed crash because startup has no NoteKey/plaintext attachment identities. That recovery must remain, but its obsolete pre-2a/E3-marker commentary should be removed.
6. `attachment_ingest_jobs` is a durable cleanup owner after ownership transfers out of `managed_plaintext_staging`, but its cleanup still performs raw file deletion. Fold that cleanup into `OwnedPlaintext` rather than retain a second filesystem convention.
7. Performance hot paths are algorithmically granular: ordinary note open/save/search make only ordinary SQLite/FTS queries plus a cheap protected-root decision; protected autosave reads the root and mutation-relevant objects only; revision append reads only the head; startup does not decrypt protected graphs; large attachment bytes remain streamed. These claims need a strengthened dedicated 2b.5 guard covering every contract-listed performance surface.

## Finite objectives

### O1 — Delete obsolete/dead architecture
- Remove unreferenced APIs/helpers found by the de-novo symbol sweep.
- Remove the compatibility-only protected-ingest scrub path and stale migration-era commentary.
- Keep current crash recovery and backup privacy-obligation proof code where deletion would weaken E0–E3 residue guarantees.

### O2 — Complete typed plaintext cleanup authority
- Add `attachment_ingest_jobs` as a durable `OwnedPlaintext` backend.
- Route pending-ingest plaintext deletion through `OwnedPlaintext` before retiring the job row.
- Add adversarial coverage that a failed ingest cleanup leaves its durable job owner intact.

### O3 — Re-audit duplicate authorities
- Prove low-level revocation/derivative transition composition remains confined to `PrivacyCoordinator`.
- Prove feature code cannot manually compose publication generations.
- Prove protected mutations do not independently acquire fresh Vault access and the storage mutex.
- Classify `UiPrivacyEpoch` as UI-local pre-admission revocation state inside `PrivacyPublicationToken`, not an alternate authoritative transition epoch.

### O4 — Performance closure
Verify source-level algorithmic behavior for:
- startup;
- ordinary note open;
- ordinary typing/autosave;
- ordinary FTS search;
- protected unlock;
- protected typing/autosave;
- revision creation;
- revision restore;
- attachment ingestion;
- attachment opening;
- Lock;
- Protect;
- Unprotect.

No speculative micro-optimizations. Only remove redundant work or fix algorithmically obvious regression if found.

### O5 — Targeted + regression QA
- Strengthen `validate-vault-2b5-consolidation.sh` around O1–O4.
- Run 2b.1–2b.5 static/adversarial guards.
- Run inherited E0–E3 / 2a.16–2a.19 static closure.
- Run cumulative A1–A7/B1–B6 static architecture closure.
- Run dependency-free JS/editor/PDF behavior QA.
- Perform post-edit objective re-audit and source/archive byte-for-byte integrity comparison.

## Native release boundary

This environment has no Rust toolchain. `cargo fmt --check`, `cargo check`, strict Clippy, and Rust tests remain mandatory on macOS and must not be represented as executed here.

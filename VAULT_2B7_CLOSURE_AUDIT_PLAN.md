# Rustnote Vault 2b.7 — Closure Repair Audit Plan

Date: 2026-09-01  
Baseline: Rustnote v0.12.5 build 2b.6  
Pass: focused 2b closure repair after independent de-novo rejection of the earlier 2b.6 certification

## Governing rule

This pass does not redesign Vault. It repairs concrete violations of the 2b contract using the existing authorities:

- `PrivacyCoordinator`
- `OwnedPlaintext` / durable managed ownership
- `SensitiveNoteData`
- granular Vault objects
- `PrivacyPublicationToken`

No compatibility architecture or parallel privacy subsystem may be introduced.

## Audit findings that reopened 2b

1. `Protect` could call a protected public reader while its own privacy admission was held and self-wait.
2. `Lock` had the same re-entry hazard.
3. protected hard deletion and related batch/rollback cleanup could enter public attachment readers while `Delete` admission was held.
4. protected revision Restore waited behind a same-note transition instead of rejecting it.
5. protected outgoing links were persisted in encrypted `MetadataState` but the public outgoing-link/backlink readers still depended on scrubbed plaintext `note_links` rows.
6. ordinary attachment quarantine used raw rename/delete after database ownership could disappear, leaving no crash-recoverable successor owner.
7. the prior validators passed despite those defects, so the closure validators needed to test the missed behavioral boundaries rather than only the presence of abstractions.

## Finite repair objectives

### O1 — Remove privacy-transition re-entry

- `Protect` must return its already-authorized note state without calling `get_note()` under Protect admission.
- `Lock` must read only the public stub while Lock admission is held.
- transition-owned hard-delete/purge/rollback cleanup must not call admission-aware protected attachment readers.
- preserve the normal public reader behavior outside mutation guards.

### O2 — Restore fail-closed transition admission

- protected revision Restore must atomically reject an existing same-note or library transition.
- reader capture and Restore admission must remain linearized so no transition can enter between them.
- no stale queue-behind behavior is permitted.

### O3 — Complete protected-link read cutover

- encrypted `MetadataState.outgoing_note_links` is authoritative for protected sources.
- locked protected sources expose no link set.
- unlocked protected sources regain outgoing target and backlink behavior.
- ordinary source → protected target title is decrypted only while the target is unlocked.
- no persistent plaintext relationship shadow may be reintroduced.

### O4 — Make canonical attachment ownership continuous

For ordinary plaintext attachment bytes, removing the final durable database reference must establish a generic durable successor owner **inside the same SQLite transaction before commit**.

Apply this to:

- attachment removal;
- attachment-operation recovery;
- unreferenced attachment-row pruning;
- ordinary revision restore;
- per-note revision pruning;
- global revision age/count pruning;
- revision storage-budget pruning;
- single-note hard deletion;
- expired-Trash batch purge;
- portability rollback.

Pending Add operations remain authoritative owners and must not gain a competing generic owner. Corrupt Add ownership JSON must fail closed.

### O5 — Make quarantine transfer crash recoverable

Canonical plaintext → `.orphaned` must be:

1. pre-own successor;
2. persist prepared transfer;
3. typed rename via `OwnedPlaintext`;
4. verify successor ownership;
5. retire predecessor only after source disappearance is conclusive.

Normal cleanup must never adopt ownership after discovering plaintext. Startup may only reconcile ownership that was already durable before the crash.

### O6 — Separate protected ciphertext GC from plaintext quarantine

Protected ciphertext must not be retained as `.orphaned` plaintext. If hard deletion removes its last `vault_blob_refs` row:

- create a durable canonical cleanup obligation in the same transaction before commit;
- commit database deletion first;
- delete the now-unreferenced ciphertext directly after commit;
- if the process dies before unlink, startup recovery consumes the already-durable cleanup obligation.

### O7 — Strengthen proof

Add explicit regressions for:

- Protect and Lock non-re-entry;
- protected permanent delete non-re-entry;
- Restore conflict rejection;
- protected link lock/unlock behavior;
- ordinary source → protected target title lock/unlock behavior;
- quarantine owner lifetime and transfer recovery;
- pending Add ownership exclusivity and corrupt Add failure;
- revision-only blob ownership during pruning/deletion;
- expired-Trash ownership handoff;
- protected ordinary-revision residue fail-closed behavior;
- protected ciphertext direct cleanup;
- restart recovery after database commit but before ciphertext unlink.

Strengthen 2b.7 and inherited E0–E3 validators so the original blind spots cannot silently return.

## Acceptance procedure

1. Re-audit I1–I7 directly against source.
2. Re-audit all 16 2b Definition-of-Done requirements.
3. Run direct SQL writer sweep.
4. Run direct sensitive filesystem writer sweep.
5. Run privacy-transition caller sweep.
6. Run raw path/plaintext-lifecycle sweep.
7. Recheck UI publication-token architecture.
8. Recheck absence of `ProtectedNotePayload` / permanent dual representation.
9. Run 2b.7 focused guard and inherited 2b.1–2b.6 guards.
10. Run cumulative E0–E3 / 2a.16–2a.19 guards.
11. Run architecture A1–A7/B1–B6 static gates, Dioxus static baseline, browser behavior corpus and shell syntax.
12. Package only after the final tree is frozen and repeat integrity comparison after extraction.

## Native release boundary

This environment has no `cargo`, `rustc` or `rustfmt`. Native Rust compilation, `cargo fmt`, Clippy and the 273-workspace-test Rust suite therefore remain a mandatory Mac release gate. The source pass must not claim those commands were executed here.

# Rustnote Vault 2b.2 — PrivacyCoordinator + unified plaintext authority

Build: `0.12.5 / 2b.2`
Baseline: `0.12.5 / 2b.1`
Scope: Pass 2b.2 only.

## Governing boundary

This pass implements the 2b contract's transition/plaintext-ownership consolidation only. It does **not** perform the 2b.3 UI publication-token migration or the 2b.4 granular protected-note persistence cutover. The 2b.1 encrypted object-store foundation remains intact and the existing protected-note steady-state representation remains unchanged until 2b.4.

The pass is governed by 2b invariants I1–I4 plus all inherited E0–E3 / 2a.16–2a.19 security invariants.

## Pre-edit audit

The 2b.1 baseline had strong but separately exposed primitives:

1. `Storage::operation_lock` serialized database/storage mutation.
2. `PlaintextDerivativeGate` blocked and drained note/library plaintext producers.
3. `VaultRuntime::{begin_note_revocation, begin_library_revocation}` rejected overlapping revocation, published revocation, drained readers and retired Vault-session plaintext.
4. `managed_plaintext_staging` durably owned library-resident staging before creation.
5. Vault materialization and process-private temporary paths used separate ownership APIs.
6. Protect/Unprotect/Delete/purge/rollback/restore callers still knew how to compose multiple privacy primitives and their ordering.
7. Protect and Unlock did not share one explicit transition-admission state machine with Lock/Unprotect/Delete.
8. Sensitive create/delete/rename behavior was spread across stage IDs, raw paths, Vault guards and native temp registries.

That was the architectural problem 2b.2 was designed to remove.

## Objective-by-objective closure

- [x] **O1 — One `PrivacyCoordinator` authority.** `Storage` owns one coordinator. The coordinator owns transition admission, the plaintext-derivative gate, Vault runtime and storage-operation mutex. `Storage` no longer owns parallel derivative/Vault-runtime transition primitives.

- [x] **O2 — Typed privacy mutation guards.** Note, multi-note and library mutation guards now carry the admitted transition lifetime. Same-note conflicts fail closed; library and note transitions are mutually exclusive; unrelated notes remain independently admissible.

- [x] **O3 — One canonical transition order.** Revoking note transitions follow: admission → block/drain plaintext producers → publish/drain Vault revocation → acquire storage mutation authority. Library transitions follow the corresponding library order. Unlock enters the same admission/storage authority without falsely revoking an unlocked session.

- [x] **O4 — Production callers migrated.** Protect, Lock, Unlock, Unprotect, permanent deletion, multi-note purge, portability rollback, complete-archive restore and repository restore enter through coordinator APIs. Static validation rejects production reintroduction of direct Vault revocation or derivative-transition composition.

- [x] **O5 — One typed `OwnedPlaintext` concept.** Managed plaintext now carries stable identity, path, scope, lifecycle and cleanup authority. Supported Rustnote-owned lifetimes cover durable managed staging, durable Vault-operation material, Vault-session material and process-private material. Explicit user export destinations remain outside Rustnote cleanup ownership rather than pretending to be managed plaintext.

- [x] **O6 — Sensitive lifecycle behind the authority.** Managed plaintext creation requires an owner first; delete moves through retiring and retires ownership only after confirmed disappearance; sensitive rename preserves predecessor ownership until transfer finalization. Durable restore transfers record a prepared predecessor→successor relationship before filesystem rename.

- [x] **O7 — High-risk paths migrated.** Attachment ingest/replacement staging, portability staging, restore materialization, protected attachment materialization, Vault operation plaintext, image/native staging and process-private handoff paths use the typed ownership authority where Rustnote controls sensitive plaintext lifetime. Remaining raw filesystem mutations in these changed namespaces are ciphertext, narrow pre-ledger residue recovery, ordinary non-sensitive files, or adversarial test setup.

- [x] **O8 — Adversarial coverage added.** Coverage includes same-note conflict, note-vs-library conflict, all-or-nothing multi-note admission, multi-note producer freeze, multi-note reader revocation, reader-vs-Lock, producer-vs-Protect, failed unlink, failed process-private/Vault-operation rename, failed durable rename, corrupt transfer metadata, prepared-transfer successor pinning and both restart boundaries around durable rename.

- [x] **O9 — Static anti-regression authority guard.** `validate-vault-2b2-privacy-authority.sh` proves coordinator ownership/order/caller migration, restricts plaintext constructors and sensitive rename methods to their authority modules, audits key sensitive lifecycle paths, checks schema/recovery markers and executes clean-schema-vs-migration SQLite equivalence.

- [x] **O10 — Re-audit, regression QA and cleanup.** Inherited E0–E3 static closure, 2a.16–2a.19, 2b.1 and the dedicated 2b.2 validator pass. Dependency-free editor/document/PDF regression QA passes. Superseded transition re-exports and direct caller choreography were removed. No newly added production raw sensitive filesystem lifecycle bypass was found in the final differential sweep.

## Important defects found and corrected during the pass

### Multi-note freeze ordering

The first coordinator draft admitted all note IDs but gated/revoked them sequentially. While waiting on the first note, a later target could still admit a producer/reader. The final implementation freezes all target derivative scopes in one state mutation and marks all target unlocked Vault sessions revoking before waiting on any one drain.

### Prepared restore transfer recovery

The first durable-transfer draft preserved ownership metadata across a crash but could discard the predecessor on startup if the process died **after transfer preparation but before filesystem rename**. That protected cleanup ownership but could lose the only copy of the incoming blob.

The final recovery distinguishes both filesystem outcomes:

```text
predecessor still exists
→ successor ownership must exist
→ complete predecessor → successor rename
→ sync destination parent
→ retire predecessor transfer row

predecessor missing
→ successor ownership + successor bytes must exist
→ finalize the already-completed transfer
```

If neither state is valid, recovery fails closed instead of forgetting ownership.

### Successor-owner retirement

A prepared predecessor now pins its successor owner. Ordinary cleanup cannot retire that successor while the predecessor transfer still references it.

### Clone/copy fallback

A failed best-effort CoW clone that leaves destination residue no longer falls through to an overwrite. Durable copy fallback uses create-new semantics and fails closed if a destination unexpectedly exists.

## Schema change

Schema version advances from 19 to 20 only to add durable prepared-transfer metadata to `managed_plaintext_staging`:

- `transfer_destination`;
- `transfer_state`, constrained to the prepared state when present.

Startup runs managed transfer recovery before legacy residue cleanup. An executable SQLite check in the 2b.2 validator proves that a clean schema-20 database and a reconstructed schema-19 database migrated through 19→20 have equivalent table structure.

## Superseded architecture removed from caller reach

Production feature/storage callers no longer receive crate-wide access to derivative transition guards or direct Vault revocation orchestration. The remaining low-level mechanisms are security implementation details behind `PrivacyCoordinator`.

Likewise, durable plaintext-owner constructors and recoverable sensitive rename operations are deliberately restricted to the storage/security modules that own their durable ledger semantics.

## Deliberate non-goals retained

2b.2 does not introduce:

- `PrivacyPublicationToken` migration — 2b.3;
- granular steady-state Vault-note cutover — 2b.4;
- encrypted search;
- SQLCipher/whole-database encryption;
- sync/CRDT architecture;
- a permanent compatibility layer.

## Closure status

Within the source/static/dependency-free execution environment available here, O1–O10 are satisfied and the changed tree has been re-audited against the 2b.2 contract. Native Rust compilation/format/Clippy/tests remain an external release gate because this environment contains no `cargo`, `rustc` or `rustfmt`; no claim of native toolchain success is made here.

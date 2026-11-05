# Vault 2b.10 — Cross-cutting invariant closure

## Baseline
Rustnote v0.12.5 build 2b.9 backup-ownership closure.

## Audit findings to close
1. `DurablePlaintextNamespace::establish()` can recreate authority around an already-nonempty plaintext tree after marker loss.
2. Protected-note-derived global status/toast publication is not uniformly linearized by `PrivacyPublicationToken`, and a toast can outlive Lock.
3. `remote-sync` can enqueue forbidden note/attachment `sync_queue` rows for protected entities.

## Objectives
1. Make durable namespace ownership genuinely fail-closed: fresh establishment rejects a missing marker around a nonempty managed root.
2. Provide one explicit adoption path for independently proven pre-existing namespaces; keep steady-state callers on verify-only authority.
3. Route all status emitted from an unlocked protected editor through a note-scoped publication token at one central boundary.
4. Make protected-derived toast visibility conditional on that token remaining publishable so Lock/revocation removes it immediately from render state.
5. Eliminate `is_publishable()` -> unleased global-status publication in protected editor paths through the central status boundary.
6. Enforce protected note/attachment sync-queue prohibition centrally at enqueue time under `remote-sync`, including delete/upsert paths.
7. Add adversarial source/Rust tests for marker-loss adoption, Lock/toast lifetime, stale protected status publication, and protected sync queue attempts.
8. Re-run inherited E0-E3, 2b.1-2b.9, architecture/static gates, then independently re-audit the three failure classes.
9. Update build identity to 2b.10 and package only after frozen-tree verification.

## Completion

All nine objectives are implemented and source-level verified. Packaging is permitted only after the final frozen-tree runnable gate, independent source audit, and ZIP extraction/byte comparison succeed.

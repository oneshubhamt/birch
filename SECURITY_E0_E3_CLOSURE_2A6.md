# Rustnote 2a.6 — E0–E3 Security Closure & Adversarial QA Contract

Build: **0.12.5 / 2a.6**

## Purpose

2a.6 closes the lifecycle defects found by the de novo audit of E0–E3 without changing Rustnote's cryptographic format. The v1 vault remains AEGIS-256 with a 256-bit key, 256-bit nonce and the full 256-bit authentication tag. Human passwords protect random note keys through Argon2id.

## 1. Durable vault-transition contract

SQLite and the attachment filesystem are not one transaction. Every protect/unprotect transition that can temporarily move plaintext therefore has a durable `vault_operations` recovery record before filesystem preparation begins.

The only phases are:

- `prepared`
- `database_committed`

The prepared record is committed with SQLite `synchronous = FULL` before plaintext is renamed/staged. The canonical database-state transition and the phase change to `database_committed` occur in the same SQLite transaction. Files needed after a database commit are `sync_all`-ed before that commit; attachment directory renames are followed by parent-directory synchronization where supported.

Startup recovery runs after schema migration and before normal database/attachment integrity checks. Recovery is idempotent:

- protect + prepared → restore ordinary plaintext attachment locations and remove uncommitted ciphertext;
- protect + database_committed → destroy retiring plaintext and finish managed-residue scrub;
- unprotect + prepared → destroy unpublished plaintext staging;
- unprotect + database_committed → publish authenticated plaintext staging, then remove obsolete ciphertext.

The journal row is removed only after recovery/finalization succeeds. Durable journal JSON contains only opaque ciphertext identities; plaintext attachment hashes are never persisted there. Crash-staging filenames are opaque. Recovery derives a plaintext blob's canonical SHA-256 identity from the staged bytes themselves. If a protect operation crashes after database commit before targeted cache cleanup, startup conservatively clears Rustnote-managed derived cache/handoff namespaces rather than persisting plaintext identities merely to support cleanup.

## 2. Autosave/emergency-journal privacy barrier

Before any vault transition, the application establishes a synchronous per-note autosave barrier. Command ordering discards earlier queued drafts before the transition proceeds. While the barrier is active, stale UI drafts cannot enter the normal autosave queue or emergency journal.

The emergency-journal decision is authoritative: storage is queried before plaintext is journaled. The barrier remains active while the note is protected **or while any durable vault operation is still pending**. Storage/privacy-state errors fail closed.

## 3. Backup ownership contract

A user-selected automatic-backup path is a container, not a Rustnote-owned directory. Rustnote owns only:

`<selected directory>/Rustnote Repository/`

Vault cleanup may recursively replace that child repository, but must never recursively delete the selected parent or unknown siblings.

For pre-2a.6 root-layout repositories, Rustnote may remove legacy artifacts only when a non-empty `repository.json`, the complete expected directory structure, every indexed manifest, snapshot database, and referenced blob validate as a real Rustnote repository. Empty or ambiguous lookalikes are retained. Unknown files/siblings always survive. The E3 privacy-scrub marker is versioned to `2` so libraries already scrubbed under 2a.4/2a.5 receive this safe legacy-backup cleanup once.

## 4. Password-memory contract

Submitted passwords move into a Rust-owned `SecretString` at the first practical Rust boundary. Its initialized bytes and spare allocation capacity are overwritten on drop. Browser/Dioxus input widgets necessarily retain their own transient text while the user is typing; that browser-owned memory is outside the Rust zeroization guarantee.

The note key remains random 256-bit material. A password never directly encrypts note content.

## 5. Argon2id v1 policy

The v1 serialized KDF profile is named `Argon2idParams::V1_INTERACTIVE` and is 64 MiB, time cost 3, lanes 4. These parameters are serialized in every key envelope, so future tuning remains migratable.

The **empirical device-acceptance gate remains separate from source correctness**: the full E0 benchmark must be run on the intended minimum device before E0 is declared performance-closed. 2a.6 must not fabricate that measurement. The expected command is:

`./scripts/run-security-e0-bench.sh --full`

## 6. Multi-process temporary-data contract

Startup cleanup must not delete staging/materialized plaintext belonging to another live Rustnote process. Process-owned directories are removed only when the owner is known dead, or—where liveness cannot be established—after a conservative age threshold. Unknown/live ownership fails safe toward retention.

## 7. Schema/migration contract

Schema 17 adds only the durable `vault_operations` table/index. Existing 12→17, 13→17, 14→17, 15→17 and 16→17 paths apply each migration exactly once. The de novo audit's earlier duplicate-15→16 suspicion was checked against source and was a false positive; 2a.6 must not introduce a compensating fake migration.

## 8. Behavioral/adversarial QA contract

Source tests must cover at minimum:

- prepared protect recovery restores retired plaintext from opaque staging names;
- committed unprotect recovery hashes and publishes staged plaintext from opaque staging names;
- durable vault-operation metadata contains no plaintext attachment hash;
- stale autosave cannot recreate an emergency-journal plaintext marker after protection;
- backup reset preserves unrelated files in the selected parent;
- fully verified legacy Rustnote backup artifacts are purged without deleting unknown siblings, while ambiguous roots are retained;
- full note/attachment AEGIS round-trip remains lossless;
- full 256-bit AEGIS authentication tag remains invariant.

Executable closure additionally requires `cargo fmt`, `cargo check`, Clippy with `-D warnings`, vault tests, workspace tests, architecture validators and E0–E3 security validators on a real Rust toolchain.

## Definition of source-level closure

2a.6 is source-level closed only when:

1. vault-transition recovery is durable and idempotent;
2. autosave cannot out-race protection into plaintext journaling;
3. backup cleanup cannot delete unrelated user data;
4. Rust-owned submitted password copies zeroize;
5. live-process temporary data is not cleaned by another process;
6. schema 17 clean creation and supported migrations are equivalent;
7. E0–E3 static security and architecture contracts pass;
8. AEGIS-256 v1 remains unchanged, including its full 32-byte authentication tag.

Native executable QA and the minimum-device E0 performance measurement are explicit external acceptance gates when the current build environment cannot run them.

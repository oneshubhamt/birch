# Rustnote Vault 2b.9 — Backup Ownership Closure

## Scope

This pass is deliberately narrow. It closes the automatic-backup ownership defect found by the independent post-2b.8 audit. It does not redesign the Vault object store, transition coordinator, publication-token system, editor, search, or encrypted-note format.

## Defect being closed

Build 2b.8 proved repository ownership with `.rustnote-repository-owner` inside `Rustnote Repository`. Vault invalidation recursively deletes that same tree. An interrupted deletion could therefore remove the ownership proof before all historical plaintext was gone. On restart the durable privacy-reset obligation survived, but Rustnote could no longer prove that the remaining repository was safe to delete. That violated I2: ownership could disappear before plaintext.

## Final authority model

The dedicated repository is now governed by `DurablePlaintextNamespace`:

```text
<user-selected backup parent>/
├── .rustnote-backup-repository-owner   <- durable authority
└── Rustnote Repository/                <- managed plaintext namespace
    ├── repository.json
    ├── blobs/
    ├── snapshots/
    ├── manifests/
    ├── .staging/
    └── quarantine/
```

The marker is a sibling of the managed tree, never a child of it. Its identity is bound to the current Rustnote library. The marker therefore survives arbitrary partial or complete recursive deletion of `Rustnote Repository`.

## Ownership before plaintext

Repository initialization establishes or verifies the external namespace before `RepositoryLayout::ensure()` can create the repository or its descendants. Steady-state repository operations use `verified_backup_repository_layout_unlocked()`, which reopens the namespace with `DurablePlaintextNamespace::open_existing()` and fails closed if the external authority is absent or corrupt.

`open_existing()` is verify-only. It cannot manufacture a replacement marker around already-existing plaintext.

Strict namespace deletion now also revalidates the external marker immediately before destructive work. A namespace handle therefore cannot remain an effective ownership proof after its durable marker has disappeared.

## Vault purge state machine

The effective sequence is:

```text
publish durable backup privacy-reset obligation
        ↓
verify external durable namespace authority
        ↓
recursively remove Rustnote Repository
        ↓
fsync the repository parent
        ↓
prove repository root is absent
        ↓
recreate a clean repository while external authority still exists
        ↓
publish a clean repository index
        ↓
clear the durable privacy-reset obligation
```

A crash before the final step leaves both the privacy-reset obligation and the external authority available for restart recovery. If the repository root was already completely removed, restart can still prove ownership and recreate the clean child. If only arbitrary children were removed, restart can finish deleting the remainder.

## Removable-volume and cross-library behavior

A remembered backup parent that is missing remains privacy-pending; Rustnote does not recreate the mount path. During pending cleanup, an empty substitute parent without the expected external marker is not treated as the original repository. A malformed marker or a marker for another library fails closed. Unrelated files in the user-selected backup parent are outside the managed namespace and are never recursively deleted.

## Consolidation / legacy cleanup

The steady-state `BackupRepositoryOwner` type and its independent read/write ownership machinery are removed.

The immediately superseded 2b.8 inline marker is supported only as a bounded one-way adoption proof:

```text
verify 2b.8 inline owner + library identity
        ↓
establish/fsync external DurablePlaintextNamespace marker
        ↓
remove inline marker through the canonical namespace
        ↓
all future operation uses external authority only
```

Older root-level/version-1 repository purge/ownership machinery is removed. Root-level lookalikes are not claimed or deleted as Rustnote-owned repositories.

## Added adversarial coverage

The source includes regression tests for:

- nonempty repository with no durable authority;
- external owner location and missing-index recovery;
- one-way 2b.8 inline-owner adoption;
- corrupt external owner;
- cross-library reuse without the mutable index;
- interrupted Vault purge after index/manifest/snapshot deletion while plaintext remains;
- crash after complete repository deletion but before privacy-obligation retirement;
- steady-state marker loss;
- strict namespace cleanup after marker loss;
- removable historical backup disappearance/reconnection;
- substitute mount behavior;
- root-level legacy lookalikes remaining untouched.

A focused static guard, `scripts/validate-vault-2b9-backup-ownership-closure.sh`, asserts the architecture and the absence of the superseded ownership engine.

## Environment limitation

The available execution environment has no `cargo`, `rustc`, or `rustfmt`. Consequently the Rust tests added by this pass cannot be executed here, and `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test` remain mandatory native Mac release gates. The runnable dependency-free/static architecture and security suites are executed separately and recorded in `QA_VAULT_2B9_CLOSURE.md`.

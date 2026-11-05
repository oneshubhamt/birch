# Rustnote Vault 2b.9 — Backup Ownership Closure Plan

Baseline: `v0.12.5 build 2b.8`.

This is a narrow closure pass against the original 2b contract. It does not redesign the Vault object store, PrivacyCoordinator, publication-token model, granular encrypted persistence, search, or editor behavior.

## De-novo affected-surface audit

The 2b.8 backup repository still uses a repository-local `BackupRepositoryOwner` marker at `Rustnote Repository/.rustnote-repository-owner` as its durable deletion authority. Vault privacy reset recursively deletes that same repository tree. Because recursive entry order is not authoritative, process death can delete the owner marker and mutable index first while leaving snapshot/blob plaintext behind. The durable backup-location privacy-reset obligation survives, but restart can no longer prove that the surviving tree is Rustnote-owned and therefore refuses cleanup. This violates I2: ownership can disappear before plaintext disappears.

The backup subsystem is also the remaining parallel plaintext-ownership convention rather than using the canonical `DurablePlaintextNamespace` authority. Older root-level/version-1 repository compatibility paths further complicate the ownership proof and should be removed where they are not required to preserve a current privacy cleanup obligation.

## Finite objectives

- [x] **O1 — External durable repository authority.** Represent the dedicated `Rustnote Repository` tree with `DurablePlaintextNamespace`; its marker must live outside the recursively deletable repository root.
- [x] **O2 — Ownership before repository plaintext.** Establish/verify the namespace before repository directories, index, manifests, snapshots, blobs, staging, or quarantine plaintext can be created or used.
- [x] **O3 — Monotonic Vault purge.** Vault cleanup must verify the external authority, recursively remove the repository, durably sync the parent, prove absence, and only then clear the privacy-reset obligation. The namespace authority must survive partial deletion and process death.
- [x] **O4 — Interrupted-purge recovery.** Restart must automatically resume cleanup after crashes at every meaningful boundary, including after arbitrary repository children (old inline owner/index/snapshot/blob/manifest) have already disappeared.
- [x] **O5 — Cross-library and substitute-mount safety.** An external owner for another library, malformed owner, missing remembered removable volume, or substitute empty mount must fail closed without deleting unrelated user data or manufacturing a false clean state.
- [x] **O6 — Current-format migration without parallel steady-state ownership.** If the immediately superseded 2b.8 inline owner is encountered, positively verify it once, publish the external namespace authority first, then retire the inline marker. Normal operation must not depend on the old marker afterward.
- [x] **O7 — Legacy cleanup simplification.** Remove older root-level/version-1 compatibility ownership machinery wherever doing so cannot strand a currently known Rustnote plaintext privacy obligation. Do not preserve a second steady-state repository format.
- [x] **O8 — Adversarial QA.** Add tests/guards for partial purge after owner/index/snapshot/blob/manifest deletion, crash after repository removal but before privacy-obligation retirement, malformed/cross-library external authority, and no-parent-user-data deletion.
- [x] **O9 — Inherited regression.** Keep the runnable A/B/Dioxus, E0–E3, and 2b.1–2b.8 source/static gates green; add a focused 2b.9 backup-ownership guard.
- [x] **O10 — CLEER cleanup.** Delete superseded `BackupRepositoryOwner` steady-state machinery and avoid introducing another ownership database/service.
- [x] **O11 — Re-audit before packaging.** Reconstruct the original backup failure from source after implementation; package only if the old failure sequence is impossible and no alternate backup plaintext lifecycle remains.

## Acceptance

This pass closes only when the backup repository satisfies I1/I2 source-level across creation, normal use, Vault purge, process death, removable-volume recovery, and cross-library protection. Native Cargo check/Clippy/tests remain a release gate if the execution environment still lacks Rust.


## Verification outcome

All eleven objectives were re-audited against the implemented source after editing. The previously failing 2b.8 crash sequence is no longer possible in the 2b.9 steady-state architecture because the authoritative namespace marker is outside the recursively deleted repository root and is revalidated by steady-state use and strict cleanup. The immediately superseded inline marker is accepted only as a one-way adoption proof; the external authority is durably established first and the inline marker is then removed. Older root-level/version-1 repository ownership machinery is no longer a production cleanup path.

The runnable static/security regression gates are green. Native Rust formatting/compilation/clippy/tests remain an external Mac release gate because this execution environment has no Rust toolchain.

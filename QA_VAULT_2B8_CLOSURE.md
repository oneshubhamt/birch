# Rustnote Vault 2b.8 — De-novo Closure QA

## Verdict

**Source-level closure: PASS.**

The three findings that prevented 2b.7 certification were reconstructed directly from the 2b.8 source rather than inferred from the new validator. Each is repaired in the implementation.

**Native release certification: still pending on a Rust-capable environment.** This sandbox has no `cargo`, `rustc`, or `rustfmt`, so it cannot truthfully certify compilation, Clippy, or Rust unit/integration execution for this exact tree.

## De-novo reconstruction of the original findings

### Finding 1 — export/output preparation

PASS.

- Recovery metadata is persisted under a durable application configuration path, not OS temp.
- The recovery record is fsynced before any hidden output sibling can be created.
- `.tmp`, `.previous`, and temporary output directories receive typed ownership before materialization/rename.
- Replacement performs predecessor capture → parent sync → publication → parent sync → predecessor deletion → parent sync → journal retirement.
- Startup recovery reconciles abandoned records and skips records locked by a live Rustnote process.
- Same-destination transactions are serialized through the ledger, including canonicalized parent aliases.
- Recovery records are shape-validated, size-bounded, identity-validated, and checksummed.
- Native Save-As and storage output callers delegate to the same authority.
- The old Markdown stale-temp scavenger and duplicate native output replacement engine are absent.

### Finding 2 — emergency recovery journal

PASS.

- A durable namespace marker outside the journal tree is established before the journal directory is created/opened.
- Journal write reserves ownership before `create_file`.
- Journal rename/replacement/quarantine/delete operations use typed ownership.
- Production journal code has no raw `fs::rename` or `OpenOptions::new` plaintext lifecycle.
- Cleanup failure therefore leaves the namespace authority intact rather than silently retiring the obligation.

### Finding 3 — preview/media caches

PASS.

- Persistent image-rendition and media-cache namespace owners are established before producer output.
- Temporary output is reserved before render/remux and typed publication follows verification.
- Persistent cache deletion uses typed namespace ownership.
- Attachment canonical/quarantine owners are retained until strict derivative cleanup succeeds.
- Cleanup is synchronized against derivative producers through the `PrivacyCoordinator` rather than caller-built lock choreography.

## Additional defect found by the de-novo audit

The first 2b.8 implementation of `DurablePlaintextNamespace::remove_subtree_strict` had asymmetric containment semantics: reservation rejected an intermediate symlink escape but strict deletion did not perform the equivalent real-path check. It could also error when a nested target was already absent.

This was repaired before packaging. New tests cover both cases.

## Frozen-tree validation

All runnable source/static gates passed on the final code tree:

- Architecture A1, A2, A3, A4, A1–A4 closure, A5, A6, A7.
- Architecture B1, B2, B3, B4, B5, B6.
- Dioxus 0.7.10 source/lock static baseline.
- Security E0, E1, E2, E3.
- E0–E3 2a.16 ownership, 2a.17 residue, 2a.18 UI lifetime, 2a.19 independent adversarial closure, and cumulative closure.
- Vault 2b.1 foundation.
- Vault 2b.2 privacy authority.
- Vault 2b.3 publication token.
- Vault 2b.4 granular cutover.
- Vault 2b.5 consolidation/performance static guard.
- Vault 2b.6 independent I1–I7 closure guard.
- Vault 2b.7 closure-repair guard.
- New Vault 2b.8 plaintext-authority closure guard.
- Bash syntax validation across all project shell validators.
- An additional one-off de-novo audit reconstructed the three original findings without relying on `validate-vault-2b8-plaintext-authority-closure.sh`; it passed.

## Seven invariants after this repair

| Invariant | Source-level result |
| --- | --- |
| I1 — ownership precedes plaintext | PASS for the audited managed-plaintext families |
| I2 — ownership survives until plaintext disappears | PASS for the audited managed-plaintext families |
| I3 — one privacy-transition authority | PASS; inherited architecture remains intact |
| I4 — conflicting transitions fail closed | PASS; inherited architecture remains intact |
| I5 — protected publication requires current authorization | PASS at source/static level |
| I6 — sensitive persistence deny-by-default | PASS at source/static level |
| I7 — mutation cost scales with changed state | PASS architecturally/source-level |

## Release gates not executable here

The following are **not failures**, but remain mandatory before a release certificate:

```
cargo fmt --all -- --check
cargo metadata --locked --format-version 1
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
```

The native 2b.8 adversarial tests embedded in `src/native/output.rs`, `src/security/plaintext.rs`, and `src/security/privacy_coordinator.rs` will execute as part of that Rust test gate.

The original 2b.5 request for empirical wall-clock comparison against the 2a.19 baseline also remains unmeasured in this environment. The repair does not change the granular Vault object model or ordinary-note hot-path architecture, but empirical performance certification should still be performed separately.

## Closure statement

On direct source reconstruction, the **specific reasons the 2b.7 de-novo audit rejected I1/I2 are fixed in 2b.8**. No remaining alternate output, journal, or persistent-derived-cache plaintext lifecycle was found in the final escape-hatch sweep.

This is sufficient to package the source as the 2b.8 plaintext-authority closure build. It is not sufficient to claim a native release certificate until the pinned Rust 1.97.1 toolchain gate runs successfully.

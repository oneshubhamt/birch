# Native compile and toolchain evolution

**Historical range:** Rustnote 2b.13–2b.41  
**Baseline archive:** `514b797fb0a6a128050cf75ac2428fc12095106a`  
**Original records reviewed:** 58/58 plan/implementation files.

This document is the current reader-facing history for the native compile/Clippy/test repair sequence. It preserves the architectural lessons and compatibility constraints that matter now without treating each pass file as living documentation. Exact originals remain available through the immutable ledger at the end.

Current commands live in [../DEVELOPMENT.md](../DEVELOPMENT.md); current evidence semantics live in [../QA.md](../QA.md). The paired historical QA reports are consolidated in [../qa/HISTORICAL_EVIDENCE.md](../qa/HISTORICAL_EVIDENCE.md).

## What the native series demonstrated

Static/source validation caught many architecture regressions, but the macOS Rust toolchain repeatedly found defects that static phrase checks could not: ownership/move errors, crate-visibility mismatches, feature-gated test compile failures, strict-Clippy dead code, scheduler-sensitive test hangs, native dependency deployment-target mismatch, Dioxus task-lifetime defects and one filesystem check-then-use race.

The durable lesson is not “run another static validator.” It is:

```text
format
-> prepare generated browser runtimes
-> compile/check the real native targets
-> strict Clippy
-> execute Rust tests
-> execute feature/test-support variants when relevant
-> run dependency-backed browser tests
-> inspect the produced native artifact
-> then perform manual/adversarial OS integration
```

A historical PASS applies only to the revision and command actually run.

## Chronological lessons

### 2b.13–2b.16 — first real native compile integration

**2b.13** was triggered by the first real macOS workspace compile after the Vault/security consolidation. It found Dioxus RSX parse problems, wrong façade placement, missing exports/imports, Rust move/borrow/type errors and warnings that static validators had missed. The repair retained narrow security boundaries: attachment transport received opaque guards rather than public access to `PlaintextOperationPermit`, `OwnedPlaintext` or private-temp internals. A manual `Debug` implementation for `VaultPlaintextPath` deliberately redacts the managed plaintext path rather than deriving a leaking formatter.

**2b.14** fixed Dioxus `FnMut` capture ownership: reusable publication tokens and `ApplicationServices` stay in the handler; each spawned future receives its own clone.

**2b.15** exposed the binary/library visibility boundary. The durable rule is to expose semantic, opaque transport capabilities—not widen private security primitives merely to make the binary compile. It also removed warnings structurally instead of adding blanket `allow` attributes.

**2b.16** removed one genuinely redundant PDF transport local after confirming the actual authorization lifetime remained in `pdf_attachment`.

### 2b.17–2b.19 — test-target compilation and strict Clippy

**2b.17** showed that `--all-targets --all-features` can expose crate-boundary and test-only API drift that the production library build does not. The zero-dependency `internal-test-support` feature was introduced so binary tests can reach narrowly scoped library fixtures without widening production APIs.

**2b.18** fixed a moved-value test defect and strict-Clippy findings. Large call signatures were consolidated into cohesive request/context values rather than silenced. Test-only lock access stayed behind a hidden feature/test façade.

**2b.19** established the preferred dead-code pattern for production-only crash-recovery symbols: compile them only under the configuration that uses them. Do not use warning suppression when `cfg(not(test))` expresses the real ownership.

### 2b.20–2b.21 — native tests found real state/lifetime defects

**2b.20** went beyond compile cleanup. Native testing exposed startup identity continuity and privacy-ordering problems:
- same canonical data path can recover its historical library identity only for a demonstrably pristine bootstrap shell;
- test/internal-support configuration and output journals must never write the real per-user configuration directory;
- Lock/Unprotect/Delete publish Vault revocation before derivative drain, while whole-library Restore preserves its stronger derivative-first ordering;
- retained protected mutation must linearize against transition admission;
- live Protect cleanup must scrub persistent image/media derivatives, not rely on restart recovery.

**2b.21** separated test-induced hangs from real production state bugs. It fixed several tests that retained the very authorization a Lock/Unprotect was supposed to drain, widened only post-release completion deadlines for debug Argon2 latency, and corrected a real Protect-readiness bug: note protection may wait for work scoped to that note, but must not be permanently blocked by unrelated library-scoped plaintext ownership rows.

### 2b.22–2b.28 — lifetime tests and validator robustness

**2b.22** replaced a blocking protected-read “revocation probe” inside a Lock lifetime test with a nonblocking test-only state inspection. It also made a validator insensitive to rustfmt line wrapping.

**2b.23** fixed a protected attachment round-trip test that held a `VaultReadPermit` across Unprotect; the resource is now explicitly dropped before a reader-draining transition.

**2b.24** found one real production regression and one test deadlock:
- global tag rename/delete must serialize ordinarily when there are zero protected notes rather than requesting an invalid empty protected-note authorization set;
- protected PDF transport tests must observe revocation without starting a second blocking read.

**2b.25** fixed only a test helper module path.

**2b.26** repaired a test assumption about first Autosave revision creation and made inherited validators formatting-robust. Production revision policy did not change.

**2b.27** applied the nonblocking revocation probe to binary PDF response/file-handle lifetime tests. The test helper is available only to tests/`internal-test-support`.

**2b.28** fixed a binary-test binding name while retaining explicit lifetime cleanup ordering.

### 2b.29–2b.30 — macOS compatibility contract

**2b.29** made the native deployment contract explicit:
- minimum macOS **12.0**;
- `.cargo/config.toml` forces `MACOSX_DEPLOYMENT_TARGET=12.0`;
- `Info.plist` declares the same floor;
- AEGIS remains **0.9.15** with its optimized native backend, not the slower pure-Rust feature;
- the final Mach-O minimum version is inspected instead of trusting source configuration alone.

**2b.30** removed a Python 3.11-only `tomllib` dependency from the deployment-target validator so that validator could run on older Python 3 installations. This does **not** mean the whole current repository is Python-3.10-compatible: the later 2b.43 version validator currently imports `tomllib`, and CI deliberately provisions Python 3.12.

### 2b.31–2b.34 — full native QA and semantic/test boundary cleanup

**2b.31** is the most important native-system closure in this sequence. The serial native suite and full browser suite found:
- scheduler-sensitive tests using unrealistically short observation deadlines;
- redundant A0 re-execution of the whole Rust suite;
- a real filesystem TOCTOU race in path normalization (`symlink_metadata` followed by `canonicalize`);
- stale browser assertions.

The path fix uses a direct canonicalize-and-peel loop: `NotFound` means peel the missing suffix and retry from the parent. This preserves symlink-aware canonical comparison without the check-then-use window.

**2b.32** corrected a production compile failure caused by calling a test/`internal-test-support` helper from production. The durable interface is the semantic `note_session_is_active`; raw key-presence compatibility helpers stay test-oriented.

**2b.33–2b.34** continued the same rule: obsolete convenience/raw helpers compile only where they are actually used. Strict Clippy should expose dead boundaries, not be neutralized by suppression.

### 2b.35–2b.37 — privacy-safe selection continuity

**2b.35** separated privacy-required content revocation from unnecessary selection loss. Protect/Lock may remove decrypted note state while preserving only non-secret transition state (stable note ID, approved public/generic label, progress text). It also records a performance rule: do not weaken Argon2/privacy cleanup because debug-mode Protect looks slow; compare release-mode behavior first.

**2b.36** split selection-request freshness from deliberate same-note view remount identity. `note_selection_generation` remains selection/privacy freshness; `note_view_generation` owns view remount. The user-approved public locked-note label remains visible while private title/body/tags/attachment names remain scrubbed.

**2b.37** removed two dead references left by that refactor without changing behavior.

### 2b.38–2b.41 — Dioxus task lifetime ownership

**2b.38** found that component-scoped tasks could be cancelled when Vault actions intentionally unmounted the initiating card/editor. It temporarily moved those workflows to root-scoped `spawn_forever`.

**2b.39** added privacy-aware deferred navigation, generation-cancelled latest-selection intent, an 8-second editor-session stabilization window aligned with browser readiness, clearer password errors and opaque toasts.

**2b.40** simplified Vault UI ownership: Encrypt/Unlock/Lock/Remove Encryption are selected-note-only actions; cross-card actions route through canonical selection first. This removed redundant target checkpointing and a dead selection-publication helper.

**2b.41 supersedes the task-lifetime mechanism of 2b.38.** Root-scoped `spawn_forever` produced Dioxus hoisted-signal lifetime warnings because `RunningApp`-owned Signals could be used from a scope that outlived them. Current UI workflows instead capture `RunningApp`'s `ScopeId` and spawn into that scope: child remounts do not cancel the work, but replacing/unmounting `RunningApp` does.

## Persistent current constraints

| Constraint | Current meaning |
| --- | --- |
| Rust toolchain | Pinned **1.97.1**, with rustfmt and Clippy |
| macOS deployment | **12.0** minimum in Cargo environment, bundle metadata and built Mach-O validation |
| Native crypto | AEGIS **0.9.15**, optimized native backend |
| Generated runtimes | Browser editor/PDF runtimes must be prepared before native build/test paths that consume them |
| Warning policy | Strict Clippy with `-D warnings`; prefer deletion/`cfg`/cohesive APIs over blanket suppression |
| Test-only library bridges | Narrow, hidden, and gated by `test` or `internal-test-support`; never a reason to widen production security APIs |
| Security lifetime tests | Existing authorized resource must be dropped before a transition that drains it; revocation observation must not start a second blocking read |
| Debug crypto timing | Short negative “still blocked” checks are useful; positive post-release completion needs a realistic finite bound. Performance decisions use release mode |
| Dioxus async work | Task scope must match the lifetime of captured Signals; root-scoped work must not capture shorter-lived component state |
| Filesystem comparison | Avoid metadata-check-then-canonicalize races; path normalization must tolerate a concurrently disappearing missing suffix without weakening ownership checks |

## Current native gate and one open coverage gap

The configured macOS CI runs:

```sh
bash scripts/prepare-rustnote-browser-runtimes.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
bash scripts/validate-macos-binary-deployment-target.sh target/release/rustnote
```

That is the current CI truth.

However, historical 2b.17+ native closure repeatedly used `--all-features`, and the present source still contains binary tests gated by:

```rust
#[cfg(feature = "internal-test-support")]
```

including protected PDF response/file-handle Lock lifetime regressions in `src/attachment_protocol.rs`. Because current CI does not enable `--all-features`, those feature-gated binary tests are **not part of the configured CI test invocation**. This is an open verification/CI-coverage finding, not a claim that the implementation is defective.

For comprehensive feature-boundary work, an additional native gate is therefore useful:

```sh
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1
```

This documentation pass does not change CI.

## Version promotion after the native series

The historical 2b.43 promotion changed version metadata only:
- package version **0.12.6**;
- build ID **2b.43**;
- runtime identifier **0.12.6+2b.43**;
- corresponding Cargo.lock and macOS bundle metadata.

Its report recorded static/source/browser promotion checks while leaving native Rust compilation/Clippy/tests as a Mac gate. Current source later includes the 2b.43.1–2b.43.3 backup-key work; see [project status](../PROJECT_STATUS.md).

## Supplementary-document disposition

Batch 5 also audited the non-pass Markdown in its scope:

- `.github/ISSUE_TEMPLATE/bug_report.md` and `.github/pull_request_template.md` are **functional GitHub templates**, not redundant narrative history; retain.
- `assets/welcome-note.md` is **bundled asset content**, not repository documentation. `Dioxus.toml` declares `asset_dir = "assets"`; retain with its sibling welcome-note assets.
- `editor-spec/README.md` is the living authority guide for the shared editor document specification; retain.
- `editor-src/README.md` is the living editor source-ownership map; retain.
- `pdf-src/README.md` is the living PDF source/runtime ownership guide; retain.
- `tools/e0-crypto-bench/README.md` is the living research benchmark guide; retain.
- `VERSION_PROMOTION_V0.12.6_BUILD_2B43.md` is historical promotion evidence with no current validator dependency found; its durable facts are consolidated above and it may leave the working tree.

## Q2 QA/document decoupling closure

Q2 removed the executable exact-path dependency on all 54 remaining native plan/implementation records for passes **2b.13–2b.21 and 2b.24–2b.41**. Before deletion, the 27 historical validators were executed non-recursively against the same revision as the replacement suite: **15 still matched current source and 12 were stale/superseded**. The maintained concern-based source suite, dependency-backed browser suite, all-target/all-feature check, strict Clippy, all-feature Rust tests, release build and deployment-target inspection then passed together on GitHub-hosted macOS/Linux CI.

The stronger feature matrix exposed a real parallel-startup race in private temporary-root creation. Q2 repaired it by accepting a concurrent `AlreadyExists` only after re-validating the final path as a real directory, with a 32-worker regression; the full native gate then passed.

The 54 plan/implementation files are now **Git-only historical evidence**. Their immutable links remain in the ledger below. The 27 `QA_NATIVE_COMPILE_*.md` reports intentionally remain in the working tree until Q4. The four 2b.22/2b.23 plan/implementation records were already Git-only after Batch 5.

## Complete 58-file immutable ledger

| Original | Role | Working-tree disposition |
| --- | --- | --- |
| [`NATIVE_COMPILE_2B13_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B13_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B13_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B13_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B14_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B14_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B14_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B14_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B15_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B15_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B15_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B15_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B16_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B16_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B16_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B16_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B17_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B17_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B17_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B17_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B18_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B18_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B18_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B18_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B19_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B19_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B19_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B19_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B20_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B20_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B20_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B20_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B21_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B21_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B21_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B21_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B22_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B22_IMPLEMENTATION.md) | Implementation record | Git-only after Batch 5 |
| [`NATIVE_COMPILE_2B22_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B22_PLAN.md) | Plan/audit trigger | Git-only after Batch 5 |
| [`NATIVE_COMPILE_2B23_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B23_IMPLEMENTATION.md) | Implementation record | Git-only after Batch 5 |
| [`NATIVE_COMPILE_2B23_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B23_PLAN.md) | Plan/audit trigger | Git-only after Batch 5 |
| [`NATIVE_COMPILE_2B24_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B24_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B24_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B24_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B25_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B25_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B25_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B25_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B26_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B26_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B26_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B26_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B27_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B27_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B27_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B27_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B28_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B28_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B28_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B28_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B29_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B29_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B29_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B29_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B30_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B30_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B30_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B30_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B31_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B31_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B31_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B31_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B32_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B32_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B32_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B32_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B33_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B33_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B33_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B33_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B34_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B34_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B34_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B34_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B35_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B35_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B35_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B35_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B36_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B36_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B36_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B36_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B37_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B37_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B37_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B37_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B38_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B38_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B38_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B38_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B39_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B39_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B39_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B39_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B40_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B40_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B40_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B40_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B41_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B41_IMPLEMENTATION.md) | Implementation record | Git-only after Q2 — executable dependency removed |
| [`NATIVE_COMPILE_2B41_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/NATIVE_COMPILE_2B41_PLAN.md) | Plan/audit trigger | Git-only after Q2 — executable dependency removed |


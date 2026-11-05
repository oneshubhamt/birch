# QA — Vault 2b.42.7 profiling

## Objectives

1. Profiling is disabled by default and requires exactly `RUSTNOTE_VAULT_PROFILE=1`.
2. Disabled spans do not capture timestamps or format output.
3. Only static typed labels can reach profiler output.
4. Protect, Lock, Unlock and Remove Encryption have end-to-end spans.
5. UI checkpoint, autosave barriers, privacy drains, Protect preparation, crypto, attachments, managed residue cleanup, object writes, FTS, WAL, VACUUM and canonical publication are represented by stable timing boundaries.
6. The previously unexplained post-Protect residue scrub is decomposed into cache, staging, handoff, backup-repository, and SQLite sub-stages.
7. No Vault transition, storage cleanup or publication behavior is optimized or reordered.

## Automated coverage

- `scripts/validate-vault-2b42-7-profiling.sh`
- `tests/js/vault-profile-contract.test.mjs`
- `src/vault_profile.rs` unit tests (native Rust gate)
- complete dependency-free browser/PDF behavior suite
- applicable inherited 2b.42.1–2b.42.6 and Vault/E0–E3 validators

## Native measurement workflow

Build and run the release application with profiling enabled:

```bash
RUSTNOTE_VAULT_PROFILE=1 cargo run --release --locked
```

Exercise Protect, Lock, Unlock and Remove Encryption on representative notes. Capture stderr and compare the end-to-end operation duration with its nested stages. Do not optimize from one run; repeat operations and compare representative small/attachment-heavy notes.

Profiling remains diagnostic only. 2b.42.8 owns the no-VACUUM forensic experiment.

## Assistant-side closure results

- 2b.42.7 static validator: **8/8 PASS** after the native-measurement refinement.
- targeted profiling behavior contract: **8/8 PASS** after the native-measurement refinement.
- 2b.42.1 through 2b.42.7 validators: **PASS**.
- complete dependency-free browser/PDF suite: **93/93 PASS**.
- applicable inherited 2b.1, 2b.4, 2b.5, 2b.6, 2b.8, 2b.9, 2b.11 and 2b.12 guards: **PASS**.
- E0–E3 2a.16 ownership guard: **PASS**.
- E0–E3 2a.17 residue/backup guard: **PASS**.
- shell syntax: **84/84 PASS**.
- JavaScript/MJS syntax: **51/51 PASS**.
- changed Rust lexical/delimiter structural audit: **3/3 PASS** for the production Rust files changed by the profiler refinement.
- `Cargo.toml`, `Cargo.lock`, and `build-id.txt`: **unchanged** from the 2b.42.6 post-audit repair checkpoint.
- build/dependency artifact sweep: **clean**.

Rust compilation, strict Clippy and native Rust tests are not claimed in the assistant environment because no Rust toolchain is installed. The prepared-editor dependency suite is likewise deferred to the Mac native gate.

The disabled-path overhead is structurally limited to the typed action match plus cached `OnceLock<bool>` checks; it performs no clock read or formatting while disabled. Native release measurement remains the final empirical confirmation.

## Package integrity

The refined 2b.42.7 checkpoint contains **516 regular files** and **89 executable files** before packaging. Final ZIP extraction verification is recorded at handoff. `build-id.txt` intentionally remains `2b.41` because this is a verified 2b.42 subpass checkpoint, not the final promoted 2b.42 build.


## Native-measurement refinement

The first native release profile showed Protect at roughly 0.67–0.77 s for small notes and roughly 2.15–2.40 s for attachment-heavy notes. VACUUM was only about 15–18 ms, while the previous stage set left roughly 0.45–0.50 s of small-note Protect unattributed. Source audit showed that `attachments_cleanup` ended before `scrub_managed_plaintext_residue_after_protection`. This refinement instruments that existing scrub and its sub-phases, plus autosave barrier/release and Protect preparation/finalization. It performs no optimization and leaves the forensic cleanup sequence unchanged.

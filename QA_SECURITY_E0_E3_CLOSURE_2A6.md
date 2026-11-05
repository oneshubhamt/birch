# QA — E0–E3 Security Closure & Adversarial QA

Build: **0.12.5 / 2a.6**

## Closure objectives

- [x] Durable SQLite↔filesystem vault-transition journal exists before plaintext movement.
- [x] Protect/unprotect recovery is phase-driven, idempotent, and runs before normal integrity checks.
- [x] Crash staging uses opaque filenames and durable journal metadata contains no plaintext attachment hashes.
- [x] Prepared protect recovery restores plaintext; committed protect recovery destroys retired plaintext and completes managed-residue scrub.
- [x] Prepared unprotect recovery destroys unpublished plaintext; committed unprotect recovery publishes authenticated plaintext and removes obsolete ciphertext.
- [x] Files required after a database commit are flushed and rename parent directories are synchronized where supported.
- [x] Autosave establishes a synchronous per-note vault barrier before protection transitions.
- [x] Emergency journaling verifies authoritative storage state and fails closed on state errors.
- [x] Autosave remains blocked while a note is protected **or any durable vault operation is pending**.
- [x] User-selected backup directory is only a container; Rustnote recursively owns/deletes only `Rustnote Repository/`.
- [x] Legacy root-layout backup deletion requires a non-empty, fully validated Rustnote repository/snapshot structure; ambiguous roots and unknown siblings survive.
- [x] Rust-owned submitted passwords move into a zeroizing `SecretString`.
- [x] Temporary/staging cleanup does not delete another known-live Rustnote process' data.
- [x] Schema 17 adds only the vault operation journal and clean schema 17 equals schema 16 + migration 16→17.
- [x] Full AEGIS-256 32-byte / 256-bit authentication tag remains unchanged.
- [x] Production Cargo dependency graph is unchanged.

## De novo findings closed in 2a.6

1. **Crash consistency:** replaced best-effort cross-filesystem rollback with a durable two-phase vault-operation recovery record.
2. **Autosave race:** stale UI/autosave drafts can no longer recreate plaintext emergency-journal data after encryption starts.
3. **Backup ownership:** removed recursive deletion authority over the user-selected parent directory.
4. **Password lifetime:** submitted Rust-side password copies are zeroized on drop, including spare allocation capacity.
5. **Temporary cleanup:** process ownership/liveness now prevents one live Rustnote instance from deleting another's staging/materialized data.
6. **Schema:** fixed the newly discovered fresh-schema version label so BASE_SCHEMA correctly reports version 17.
7. **Recovery metadata privacy:** removed plaintext attachment hashes from the durable journal and crash-staging path names.
8. **Legacy backup proof:** deletion now requires complete expected layout plus valid indexed snapshots/manifests/database/blob structure; empty/ambiguous roots are retained.
9. **Validator quality:** E0–E3 closure validation now checks crash phases, authoritative barriers, opaque journal state, backup ownership, schema equivalence markers, process responsibility, and full-tag invariants rather than only older implementation markers.

## Static/behavioral QA executed in packaging environment

- `validate-security-e0-e3-closure.sh`: **PASS**.
  - E0 static contract: PASS.
  - E1 static contract: PASS.
  - E2 static contract: PASS.
  - E3 static contract: PASS.
- Architecture A1–A7/B1–B6 in static mode: **PASS**.
- Dependency-free editor/PDF behavior tests exercised by B4/B5/B6: **PASS**.
- B6 static closure reports 0 direct `Result<_, String>` APIs and 137 Rust test declarations in source.
- Shell syntax scan: **PASS**.
- First-party JavaScript syntax scan: **PASS**.
- `Cargo.toml`, `rust-toolchain.toml`, `Dioxus.toml` TOML parsing: **PASS**.
- Changed Rust source lexical/delimiter structural scan: **PASS**.
- Independent SQLite execution:
  - clean BASE_SCHEMA → user_version 17;
  - 2a.5 BASE_SCHEMA 16 + `SCHEMA_16_TO_17` → user_version 17;
  - both contain **48 equivalent SQLite schema objects**.
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `Dioxus.toml`, `editor-src`, `editor-build`, `editor-spec`, `pdf-src`, and pinned `assets/pdfjs`: unchanged from 2a.5.
- `rustnote-vault` wire cryptography is unchanged; its only change is promotion/aliasing of the existing 64 MiB / t=3 / lanes=4 Argon2id profile name.

## Adversarial regression tests added/retained in source

- prepared protection crash restores opaque-staged plaintext attachment bytes;
- committed deprotection crash publishes opaque-staged authenticated plaintext bytes;
- stale autosave cannot recreate an emergency-journal plaintext marker;
- unrelated files in the user-selected backup parent survive reset;
- fully verified legacy Rustnote repository artifacts are purged while unrelated siblings survive;
- ambiguous legacy root repositories are retained rather than destructively guessed;
- protected attachment encryption/decryption remains lossless;
- AEGIS-256 full 256-bit authentication tag remains a compile-time/tested invariant.

## Explicit external acceptance gates

This packaging environment has **no Rust toolchain** and outbound access cannot obtain one. Therefore it cannot execute `cargo fmt`, `cargo check`, Clippy, or Rust tests. These must pass on the development Mac before 2a.6 is called native-executable closed.

E0 also deliberately requires one full crypto benchmark on the intended minimum native hardware. The source freezes the v1 envelope profile at **Argon2id 64 MiB / time cost 3 / lanes 4**, but empirical performance acceptance remains pending until `./scripts/run-security-e0-bench.sh --full` is run on that hardware and reviewed.

## Closure verdict

**Source/static E0–E3 closure: PASS.**

**Native executable + minimum-device E0 performance closure: PENDING external Mac run.**

Do not begin S1 until those external commands pass and the benchmark confirms acceptable v1 unlock/encryption performance.

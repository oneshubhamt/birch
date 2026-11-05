# QA — Vault 2b.42.8 No-VACUUM Forensic Closure

## Objective audit

- PASS: native macOS experiment demonstrated historical-only and current plaintext markers absent from DB and WAL without VACUUM.
- PASS: pre-Protect Rustnote-managed backup plaintext was proven present and then invalidated.
- PASS: recreated post-Protect managed snapshot/repository were marker-free.
- PASS: Production VACUUM removed from the canonical Protect residue path.
- PASS: experimental `SqliteResidueScrubMode` and test-only alternate Protect helper removed.
- PASS: obsolete `VaultProfileStage::Vacuum` removed.
- PASS: duplicate second WAL truncation removed because no write remains between checkpoints.
- PASS: secure-delete, full synchronous boundary, FTS rebuild, and verified WAL truncation remain.
- PASS: forensic regression now calls the real production Protect path.
- PASS: no runtime/environment/UI bypass exists.

## Native evidence received

Native macOS forensic test: PASS.

```text
running 1 test
...no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups ... ok

1 passed; 0 failed; 290 filtered out
```

The accompanying 2b.42.8 validator also reported 8/8 PASS on the experimental checkpoint.

## Dependency-free final-source QA

- `scripts/validate-vault-2b42-8-no-vacuum-forensic.sh`: 8/8 PASS.
- `tests/js/vault-no-vacuum-forensic-contract.test.mjs`: 6/6 PASS.
- `tests/js/vault-profile-contract.test.mjs`: 8/8 PASS after removing the obsolete VACUUM stage.
- 2b.42.1–2b.42.8 validator chain: PASS.
- dependency-free browser/editor/PDF/source suite: 99/99 PASS.
- inherited Vault 2b.1, 2b.4, 2b.5, 2b.6, 2b.8, 2b.9, 2b.11, 2b.12 guards: PASS.
- E0–E3 2a.16 canonical plaintext ownership guard: PASS.
- E0–E3 2a.17 persistent residue/backup guard: PASS.
- shell syntax: 85/85 PASS.
- JS/MJS syntax: 52/52 PASS.
- `Cargo.toml`, `Cargo.lock`, and `build-id.txt`: unchanged from the experimental 2b.42.8 checkpoint.
- production `VACUUM;` SQL: absent.
- experimental scrub-mode/test-helper scaffolding: absent.
- source tree: 520 regular files, 90 executable regular files, 0 symlinks; no target/node_modules/__pycache__/.DS_Store contamination.

The architecture A0 runtime validator cannot run in this environment because the prepared generated editor runtime is intentionally absent; it remains part of the native final gate.

## Final native closure gate

The canonicalized production source must be run on macOS with:

```bash
cargo fmt --all
cargo fmt --all -- --check
./scripts/prepare-rustnote-mac.sh
./scripts/validate-prepared-runtime.sh
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked \
  no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups \
  -- --nocapture --test-threads=1
cargo test --workspace --all-features --locked -- --test-threads=1
./scripts/validate-vault-2b42-8-no-vacuum-forensic.sh
```

The final package should not be promoted beyond 2b.42.8 until those native checks are green. The `block v0.1.6` future-incompatibility advisory is inherited dependency output and is separate from the Rustnote E3 decision.

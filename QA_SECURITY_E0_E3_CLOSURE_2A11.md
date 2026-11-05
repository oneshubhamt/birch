# Rustnote 0.12.5 build 2a.11 — E0–E3 adversarial lifecycle closure QA

## Objectives

- [x] Keep protected payload reads inside the Vault read lease through validation/transformation and reject late publication after revocation begins.
- [x] Invalidate live note-list/selection generations, redact protected list cache state, and unmount the selected note before Lock waits.
- [x] Replace PID-only sensitive-temp liveness with a random session plus OS-released exclusive owner-file lock.
- [x] Preserve migration cleanup for legacy PID directories and cover the owner-lock setup crash window.
- [x] Preserve live handoff ownership artifacts during full E3 payload scrubs.
- [x] Persist library-scoped historical automatic-backup locations before configuration can forget them.
- [x] Reject cross-library transfer/reuse of one remembered backup directory.
- [x] Purge remembered historical Rustnote repositories under backup ownership locks without deleting unrelated user files.
- [x] Preserve schema 17, E3-v4 FTS cleanup, snapshot fail-closed behavior, and all earlier E0–E3 cryptographic/storage invariants.

## Native acceptance

The sandbox still has no Rust/Cargo toolchain and cannot resolve/download the pinned toolchain. These commands remain mandatory on the development Mac before executable-release acceptance:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
cargo audit
```

The added concurrency, temp-owner and backup-history regressions are executable Rust tests and must be run by the workspace test command; static marker checks are not substitutes.

## Sandbox QA

- [x] E0, E1, E2 and E3 static security validators pass for build 2a.11.
- [x] Cumulative 2a.11 E0–E3 source-level closure validator passes.
- [x] A1–A7 and B1–B6 cumulative static architecture validators pass.
- [x] A4 caught the first temp-owner implementation using SQLite outside storage; the implementation was redesigned around `std::fs::File` exclusive locking rather than weakening the architecture guard.
- [x] Dependency-free editor/PDF browser behavior suite: 36/36 pass.
- [x] First-party shell syntax, JavaScript/MJS syntax and TOML parsing pass.
- [x] Rust token/delimiter lexical scan passes across 133 source files.
- [x] `git diff --check` passes.
- [ ] Pinned Rust 1.97.1 `fmt`, metadata/check, Clippy, executable Rust tests and `cargo audit` cannot run in this sandbox because no Rust toolchain is installed and external toolchain download is unavailable.
- [ ] ZIP integrity and fresh-extraction validation are completed by the final packaging step.

Static/source closure is therefore clean; native executable-release acceptance remains explicitly pending until the commands above run on the development machine.

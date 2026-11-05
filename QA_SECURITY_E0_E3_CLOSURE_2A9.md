# Rustnote 0.12.5 build 2a.9 — E0–E3 persisted tag-filter closure QA

## Objective

Close the final de novo E3 finding from 2a.8: Rustnote-managed `settings.note_list_filter` could retain a protected-only literal tag name after note protection. Preserve every previously closed E0–E3 and 2a.6–2a.8 invariant. E0 device acceptance is treated as resolved by project decision.

## Implemented

- Protection transaction sanitizes the persisted note-list tag filter after protected tag membership/orphan rows are removed.
- The note-list preference write boundary independently drops tag filters not present in the public tag catalog, preventing stale delayed UI writes from recreating the residue.
- E3 privacy migration marker advances to version 3; legacy 2a.8 filter residue is sanitized at initialization and SQLite/WAL residue cleanup runs when the preference changed.
- Libraries without current protected notes receive only the narrow preference cleanup, not broad cache/backup invalidation.
- Successful protection refreshes/prunes the live tag catalog and tag-filter state; refresh failure fails closed.
- Regression tests cover immediate protection, stale preference rewrite and legacy marker-2 migration.
- Current E2/E3 security documentation uses ProseMirror terminology.

## Native acceptance

Run on the development Mac:

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

Packaging-environment results are appended after the final 2a.9 QA run.

## Packaging-environment QA results

Completed against the finalized 2a.9 source:

- E0 static security contract: **PASS**.
- E1 static security contract: **PASS**.
- E2 static security contract: **PASS**.
- E3 static security contract: **PASS**.
- Cumulative 2a.9 E0–E3 source-closure validator: **PASS**.
- A7 final architecture guards, static mode: **PASS**; 147 Rust tests discovered.
- B6 cumulative architecture guards, static mode: **PASS**; A1–A7 and B1–B6 chains remain green.
- Dependency-free PDF behavioral suite: **16/16 PASS**.
- Full dependency-free JS subset: **36 PASS**. Two additional ProseMirror behavior test files cannot load in this sandbox because the prepared `prosemirror-state` npm dependency is absent; this is an environment limitation, not a test assertion failure.
- All shell scripts parse under the Bash-compatible validator environment: **PASS**.
- First-party editor/PDF/build JavaScript syntax: **PASS**.
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, and `Dioxus.toml` TOML parsing: **PASS**.
- Edited Rust files (`settings.rs`, `protection.rs`, `ui/mod.rs`, `ui/app.rs`, `ui/note_card.rs`) structural delimiter scan and whitespace hygiene: **PASS**.
- Unchanged-from-2a.8 integrity check: `rustnote-vault`, schema 17, `editor-src`, `editor-build`, `editor-spec`, `pdf-src`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, and `Dioxus.toml`: **PASS**.
- Stale `Wordgard` terminology in current E2/E3 security contract/QA material: **removed**; ProseMirror is identified correctly.
- Final package-level verification: the release ZIP was extracted into a clean directory; **281/281 files matched the finalized source tree**, ZIP integrity passed, and the extracted package itself passed E0–E3/2a.9 security closure plus cumulative A1–A7/B1–B6 static validation.

## Environment limitation

This sandbox still has no Rust/Cargo toolchain and cannot provision it because outbound dependency/toolchain access is unavailable. Therefore `cargo fmt`, `cargo metadata --locked`, `cargo check`, strict Clippy, Rust tests and `cargo audit` are **unexecuted here, not failed**. They remain the native acceptance gate on the development Mac.

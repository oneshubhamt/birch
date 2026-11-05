# Rustnote 0.12.5 build 2a.8 — E0–E3 final closure QA

## Scope

Build 2a.8 closes the residual descriptor-lifetime, endpoint-test and rendition-permission findings from the de novo 2a.7 audit. E0 real-device acceptance is treated as resolved by project decision.

The cryptographic wire format is unchanged: AEGIS-256, 256-bit key, 256-bit nonce, full 256-bit/32-byte authentication tag, with Argon2id wrapping random per-note keys.

## Implemented closure

- PDF session caching retains metadata only; no `File`, `Arc<File>` or `PdfReadHandle` is cached.
- Every PDF data read opens a fresh request-scoped handle only after current vault authorization.
- `ManagedAttachment` continues to own the complete transport resource/permit for the protocol request.
- Media opens use `LeasedMediaFile`, coupling the OS file handle to the vault permit for the complete stream/read lifetime.
- Deterministic protected-media revocation test verifies Lock waits until the opened media file is dropped.
- Existing protected-PDF transport revocation test remains and is explicitly identified as PDF transport coverage.
- PDF session-cache regression test proves cached metadata survives independently while a fresh handle correctly observes unlink.
- Converter-created rendition output is passed through `harden_existing_file()` before publication; Unix mode is normalized to `0600` and symlink/non-file outputs fail closed.
- The private-file unit test verifies permission re-hardening.
- `webbrowser` remains patched at 1.2.4.
- macOS packaging and Dioxus-baseline validators no longer reject valid future build IDs because of a stale 2a.6 equality check.

## QA status

This document is finalized after the 2a.8 static/architecture/package QA run. Native Rust/Cargo/audit execution must be reported separately if the sandbox still lacks the pinned toolchain.

## Final sandbox verification

Completed on the packaged 2a.8 source before release:

- E0/E1/E2/E3 static security validators: PASS.
- 2a.8 cumulative E0-E3 source-closure validator: PASS.
- A1-A7 and B1-B6 cumulative architecture validation in documented static mode: PASS.
- Dependency-free editor/PDF browser behavior suite: 36/36 PASS.
- B6 dependency-free PDF behavior suite: 16/16 PASS.
- All first-party shell scripts: syntax PASS.
- All first-party JavaScript/MJS sources under `editor-src/`, `pdf-src/` and `scripts/`: syntax PASS.
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and `Dioxus.toml`: TOML parse PASS.
- All 133 first-party Rust source files: lexical delimiter/structure scan PASS.
- Changed Rust sources: whitespace hygiene PASS.
- Dioxus 0.7.10 source/lock baseline in static mode: PASS.
- Build identity: `2a.8`.
- `webbrowser`: patched `1.2.4`; vulnerable `1.2.1` absent.
- Unchanged from 2a.7: `Cargo.toml`, `rust-toolchain.toml`, `Dioxus.toml`, `crates/rustnote-vault/`, `editor-src/`, `editor-build/`, `editor-spec/`, and `pdf-src/`.

The sandbox does not contain Cargo/Rust and cannot download the pinned toolchain because outbound toolchain download is unavailable. Therefore the following are **not reported as executed** here and remain the native acceptance gate on the development Mac:

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

The E0 real-device performance acceptance is treated as resolved by project decision for this closure pass.

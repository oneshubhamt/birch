# Rustnote 0.12.5 build 2a.7 — E0–E3 final security closure QA

## Scope

Build 2a.7 closes the residual source-level E0–E3 findings from the de novo 2a.6 audit, with E0 real-device benchmark acceptance treated as resolved by project decision.

The cryptographic wire format is unchanged: AEGIS-256, 256-bit key, 256-bit nonce, full 256-bit/32-byte authentication tag, and Argon2id-wrapped random per-note keys.

## Implemented closure

- One per-note vault session/revocation authority.
- Counted `VaultReadPermit` lifetime for protected decrypt, attachment, PDF and media reads.
- `Lock` publishes revocation before waiting for active reads, then removes the key/session and registered decrypted material before successful completion.
- Late materialization after revocation is rejected.
- Protected resources retain their lease through storage/application/transport consumers; protected bare-path escape at the application attachment boundary is removed.
- Deterministic concurrency regression tests cover lock/read revocation and transport-resource lifetime.
- Sensitive staging uses the common private staging abstraction; Unix directories are restricted to 0700 and files to 0600.
- Protected payload identity/graph validation is strengthened across attachments, blobs, revisions, sources/citations, PDF annotations and outgoing note links.
- Pre-commit Remove Encryption plaintext staging semantics and metadata/external-egress limits are documented.
- Passphrase guidance favors a long unique passphrase or password-manager-generated password.
- Transitive `webbrowser` is advanced from vulnerable 1.2.1 to patched 1.2.4; no broad dependency upgrade was performed.

## QA completed in the source sandbox

PASS:

- `validate-security-e0.sh` in static mode
- `validate-security-e1.sh` in static mode
- `validate-security-e2.sh` in static mode
- `validate-security-e3.sh` in static mode
- `validate-security-e0-e3-closure.sh`
- static A1–A7 architecture validation
- static B1–B6 architecture validation
- canonical editor-document specification/contract checks
- first-party JavaScript syntax checks
- shell syntax checks
- dependency-free browser/PDF behavioral suite (36/36)
- changed-Rust delimiter/string/comment structural scan
- TOML parsing for `Cargo.toml`, workspace manifests and `Cargo.lock`
- source ZIP integrity and checksum (performed at packaging)

The sandbox does not contain Rust/Cargo/rustfmt, so it cannot truthfully execute native Rust compilation or RustSec tooling. That is an environment limitation, not a passed gate.

## Required native Mac acceptance

Run from the unpacked source:

```bash
cd ~/Developer/rustnote

cargo fmt --all
cargo fmt --all -- --check
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
cargo audit

./scripts/validate-security-e0.sh
./scripts/validate-security-e1.sh
./scripts/validate-security-e2.sh
./scripts/validate-security-e3.sh
./scripts/validate-security-e0-e3-closure.sh
```

If these native gates pass, E0–E3 can be treated as closed and the project can proceed to S1 without further encryption-foundation work.

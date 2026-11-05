# Rustnote v0.12.5 build 2b.26 — QA

## Targeted static closure

- Metadata-locality test establishes a recent protected revision before the final Autosave locality assertion.
- Production first-Autosave behavior remains unchanged.
- 2b.23 protected attachment lifetime guard is multiline/`cargo fmt` tolerant while retaining ordering checks.
- E2 static validation classifies 2b.26 as schema v21/current protected-sync architecture.

## Inherited validation

Run the complete 2b.25 → 2b.11 chain and E0–E3 2a.16–2a.19 closure guards, plus Vault 2b.1–2b.10 and core E0–E3 static validation where supported.

## Environment limitation

This container does not provide the Rust toolchain, so `cargo check`, Clippy and native Rust tests must be run on macOS. The decisive native regression is `protected_source_citation_and_pdf_annotation_mutations_touch_metadata_only`.

## Results in this environment

- `validate-vault-2b26-autosave-baseline-validator-closure.sh`: PASS, including inherited 2b.25 → 2b.11 and E0–E3 2a.16–2a.19 guards.
- Vault 2b.1–2b.10 validators: PASS individually.
- Core E0, E1, E2 and E3 validators: PASS in their documented static-only modes.
- `bash -n scripts/*.sh`: PASS.
- `node --check scripts/*.mjs`: PASS.
- Dependency-free browser/editor/PDF behavior suite: 36/36 PASS.
- Rust source diff versus 2b.25: test-only change in `src/storage/protection.rs`; production Rust code unchanged.
- Native Cargo/Clippy/Rust tests: not available in this container; required on macOS.

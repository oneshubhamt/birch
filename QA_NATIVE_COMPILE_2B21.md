# Rustnote 2b.21 — QA Record

## Static/source QA completed in packaging environment

- `validate-vault-2b21-native-test-state-lifetime-closure.sh` passed.
- Inherited 2b.20 → 2b.11 closure chain passed.
- Vault 2b.1 → 2b.10 static/adversarial validators passed individually.
- E0–E3 2a.16 ownership, 2a.17 residue, 2a.18 UI lifetime, and 2a.19 adversarial closure validators passed through the inherited chain.
- All shell scripts passed `bash -n` syntax checking.
- JavaScript/MJS files covered by the runtime/test surfaces passed `node --check`.
- `scripts/run-browser-behavior-tests.mjs` passed all 36 dependency-free editor/PDF behavioral tests: 36 passed, 0 failed.
- Lexical/delimiter integrity passed for all Rust files modified by 2b.21: `src/autosave/emergency_journal.rs`, `src/security/privacy_coordinator.rs`, and `src/storage/protection.rs`.
- Archive extraction and exact tree-integrity comparison is performed after packaging and recorded in the delivery response.

## Native evidence still required

The packaging environment has no Cargo/rustc/rustfmt. Native compile, Clippy and Rust unit/integration tests must be run on macOS and are not claimed by this document.

Priority native regressions for 2b.21:

1. emergency-journal protected cleanup fixture;
2. overlapping multi-note admission test;
3. ordinary attachment command Protect lifetime;
4. ordinary note export Protect lifetime;
5. pending attachment mutation Protect barrier;
6. active derivative Protect lifetime;
7. managed residue scrub;
8. protected revision restore granular graph test.

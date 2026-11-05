# QA — Vault 2b.42.3 note-local UI

## Required invariants

1. Lock/revocation of B does not change the derived note-privacy tuple for A.
2. Same-note revocation invalidates A's note-local publication tokens.
3. Properties, current-note Sources/citations, revision previews, attachment UI, editor caches and note-derived dialogs contain no surviving plaintext cache after same-note revocation admission.
4. Note-local capture/scope construction never uses `list_generation`.
5. Aggregate surfaces remain explicitly multi-note and are not silently converted in this pass.

## Targeted assistant-side QA

Run:

```bash
./scripts/validate-vault-2b42-1-scoped-publication-core.sh
./scripts/validate-vault-2b42-2-navigation-source-target.sh
./scripts/validate-vault-2b42-3-note-local-ui.sh
node --test tests/js/note-local-privacy-contract.test.mjs
```

Then run applicable inherited privacy/security guards and the dependency-free browser/PDF suite. A pre-existing stale Vault UI-contract assertion in the broader suite may remain until 2b.42.5; it must be baseline-compared rather than attributed to this pass.

## Native gate

On macOS after installing this checkpoint:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1
./scripts/validate-vault-2b42-1-scoped-publication-core.sh
./scripts/validate-vault-2b42-2-navigation-source-target.sh
./scripts/validate-vault-2b42-3-note-local-ui.sh
node --test tests/js/note-local-privacy-contract.test.mjs
```

Manual adversarial checks for this subpass: properties A → Lock B; revision preview A → Lock B; attachment panel/file command A → Lock B; Sources A → Lock B; then repeat each with Lock A and verify A's local plaintext disappears immediately while B transitions do not clear A.

## Observed assistant-side result

- 2b.42.1 scoped publication core guard: PASS.
- 2b.42.2 navigation source/target guard: PASS.
- 2b.42.3 note-local UI guard: PASS.
- Dedicated note-local JS contract: 8/8 PASS.
- 2b.11 atomic privacy-authority guard: PASS.
- Independent 2b.6 I1-I7 guard: PASS.
- 2b.12 AEGIS/privacy-disclosure guard: PASS.
- E0-E3 2a.16 canonical plaintext ownership guard: PASS.
- E0-E3 2a.17 persistent residue/backup guard: PASS.
- Shell syntax: 79/79 PASS.
- JS/MJS syntax: 14/14 PASS.
- Changed Rust delimiter/lexical structural audit: PASS for `editor.rs`, `note_properties.rs`, `right_panel.rs`, and `sources.rs`.
- Dependency-free browser/PDF runner: 52/53 PASS. The sole failure is the same stale `editor-ui-contract.test.mjs` Vault-action assertion present in the untouched 2b.42.2 baseline; it expects the removed global `wait_for_note_list_publication_token` path and is deferred to 2b.42.5.

Native Rust formatting/compile/Clippy/tests were not executable in the assistant environment because no Rust toolchain is installed. They remain mandatory before native acceptance.

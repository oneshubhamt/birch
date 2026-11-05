# Rustnote 2b.17 — QA

## Static closure performed here
- targeted 2b.17 test-compile guard;
- inherited 2b.16 native-warning guard;
- inherited Vault/security static and adversarial guards available without Cargo;
- shell syntax checks;
- archive path/hash equivalence after packaging.

## Native gate
This environment does not contain Cargo/rustc. The authoritative next gate on macOS remains:

```bash
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
```

## Additional dependency-free behavioral regression
`node scripts/run-browser-behavior-tests.mjs` passed 36/36 editor/PDF/browser behavioral tests on the final 2b.17 tree.

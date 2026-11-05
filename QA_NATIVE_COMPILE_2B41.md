# Rustnote 2b.41 — QA

## Assistant-side verification

- Static guard requires `RunningApp` task ownership and rejects all `spawn_forever` in `src/ui`.
- UI contract updated to require `state.spawn(...)` for Vault and note-selection workflows.
- Inherited 2b.40 selected-note Vault-action contract remains required.
- Dependency-free browser behavior suite is run after implementation.
- Shell syntax and whitespace/diff integrity are checked.

## Native Mac gates

The assistant environment has no Rust toolchain. The Mac must run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1
```

The 2b.40 environment run must no longer emit `dioxus_signals::warnings::__copy_value_hoisted` for these UI workflows.

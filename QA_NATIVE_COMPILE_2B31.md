# Rustnote 2b.31 — QA

## Objective closure

1. **PrivacyCoordinator failure** — production transition/mutation ordering was re-audited and left unchanged. The failing test family observed transition admission with duplicated one-second scheduler-sensitive spin loops. A single test-only helper now waits up to 15 seconds with a 1 ms sleep for the privacy-generation admission signal; positive post-release completion waits are also 15 seconds, while short negative waits proving the transition is still blocked remain intact.
2. **Architecture-run ENOENT failures** — the four failures occurred only in the redundant default-parallel Cargo rerun, but deeper audit found a genuine production path-normalization TOCTOU risk. `normalize_directory_for_comparison` now canonicalizes directly and, on `NotFound`, peels the missing suffix and retries. This makes historical library/backup path comparison robust if a path disappears concurrently. A0 itself no longer redundantly runs the full Cargo suite; full Rust behavior remains the separate serial workspace gate.
3. **ProseMirror atomic-boundary failures** — production editor code was re-audited and left unchanged. The failing tests used invalid/stale boundary fixtures: lists are not atomic boundary nodes, and fixtures beginning with an atomic node omitted the required leading automatic boundary. The tests now exercise canonical valid documents.
4. **UI contract failures** — production UI behavior was present. The source-contract regexes were stale: note preview placeholder logic intentionally excludes attachment matches, and default line spacing is mapped from an optional enum into the bridge string. The assertions now match those semantics instead of obsolete spelling.
5. **Production-diff scope** — compared with packaged 2b.30, the sole production Rust logic change is the path-normalization race closure in `src/storage/locations.rs`. The PrivacyCoordinator Rust changes are confined to its `#[cfg(test)]` module. No editor runtime source changed.

## Available environment results

- `./scripts/validate-vault-2b31-native-full-qa-closure.sh`: PASS through inherited 2b.30 → 2b.11 and E0–E3 closure guards.
- Vault 2b.1 through 2b.10 validators: PASS individually.
- Shell syntax (`bash -n scripts/*.sh`): PASS.
- Changed JavaScript syntax (`node --check`): PASS.
- `tests/js/editor-ui-contract.test.mjs`: **9/9 PASS**.
- Dependency-free browser/editor/PDF behavior suite: **36/36 PASS**.
- Full ProseMirror/browser behavior suite: requires installed editor-build dependencies and must be confirmed on the user's Mac; expected current suite size is **101** tests.
- Rust native compile/Clippy/tests: sandbox has no Rust toolchain; must be confirmed on the user's Mac.

## Required Mac closure

1. `cargo fmt --all -- --check`
2. prepare + prepared-runtime validation
3. `cargo check --workspace --all-targets --all-features --locked`
4. strict Clippy
5. targeted PrivacyCoordinator regression
6. targeted storage tests that previously surfaced `ENOENT`
7. corrected ProseMirror atomic-boundary regressions
8. serial full workspace Rust test suite
9. full 101-test browser behavior suite
10. 2b.31 closure validator
11. architecture A0 guard (architecture validation only; no redundant Cargo test rerun)
12. macOS app build/launch if all above pass

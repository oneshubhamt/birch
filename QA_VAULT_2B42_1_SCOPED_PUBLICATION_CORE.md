# Rustnote Vault 2b.42.1 — QA

## Required behavioral tests encoded in Rust

`src/security/privacy_coordinator.rs` now directly tests:

1. Lock B leaves note-local publication A valid and does not drain A's active lease.
2. Lock B invalidates B and waits for an already-started B publication lease.
3. Lock B invalidates multi-note publication and waits for an already-started multi-note lease.
4. Library Restore invalidates A, B and multi-note authorization and waits until every active publication scope drains.
5. The inherited 2b.11 compound-operation tests remain present and are keyed to the affected note generation.

`src/ui/privacy.rs` additionally tests the pure scoped UI state machine:

- note B changes only B + multi-note generations;
- overlapping B revocations remain fail-closed until the last guard releases;
- library revocation invalidates every UI scope without iterating/mutating per-note generations.

## Assistant-side validation

The pass includes:

`scripts/validate-vault-2b42-1-scoped-publication-core.sh`

It verifies the scoped fields, capture conditions, admission/drain rules, exceptional protected-Restore path, application/storage APIs, atomic UI state mutation, no double generation bump on guard release, required test names, and absence of the obsolete global publication authority in the authority layers.

## Native gate still required

This environment does not provide `cargo`/`rustc`, so no native Rust compile or unit-test pass is claimed here. On macOS, this pass must at minimum pass:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked -- --test-threads=1
./scripts/validate-vault-2b42-1-scoped-publication-core.sh
```

The broader 2b.42 release gate remains deferred until the later subpasses are complete.

## Observed assistant-side results

The following checks were actually executed against this 2b.42.1 tree:

- shell syntax: **77/77 scripts passed `bash -n`**;
- `validate-vault-2b42-1-scoped-publication-core.sh`: **PASS**;
- `validate-vault-2b11-atomic-privacy-authority-closure.sh`: **PASS**;
- `validate-vault-2b6-independent-closure.sh`: **PASS**;
- `validate-vault-2b12-aegis-privacy-disclosure-closure.sh`: **PASS**;
- `validate-security-e0-e3-ownership-2a16.sh`: **PASS**;
- `validate-security-e0-e3-residue-2a17.sh`: **PASS**;
- lexical delimiter/balance check of the two modified Rust files: **PASS**.

The dependency-free browser/PDF suite reports **36/37**. The same single failure is reproducible on the untouched WIP checkpoint: its historical UI contract still searches for `wait_for_note_list_publication_token`, while the WIP already renamed/migrated that broader navigation helper. 2b.42.1 deliberately does not modify navigation or that JS contract; this belongs to the later navigation/UI passes.

Three historical validators are intentionally stale for the new authority model:

- 2b.2 expects one global `privacy_generation` atomic;
- 2b.3 expects one global `active_publications` counter;
- 2a.18 expects one global UI `active_revocations` counter.

Those failures are syntax/architecture assumptions that 2b.42.1 is specifically replacing. The successor 2b.42.1 validator checks the scoped equivalent directly. No compatibility shim or weaker global authority was reintroduced to satisfy them.

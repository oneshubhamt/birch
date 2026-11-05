# QA — Rustnote Vault 2b.4 granular protected-note cutover

## Verdict

**Source/static/dependency-free closure: PASS.**

**Native Rust release gate: NOT EXECUTED in this sandbox.** `cargo`, `rustc`, and `rustup` are not installed here. Therefore this report does not claim a native compile/test/clippy pass that was not actually run.

## Security / Vault validation executed

Passed:

```text
Security E0 static contract
Security E1 static contract
Security E2 static contract
Security E3 static contract
2a.16 canonical plaintext ownership guard
2a.17 persistent residue / backup closure guard
2a.18 UI lifetime + protected revision restore guard
2a.19 independent adversarial closure guard
2a.19 cumulative E0–E3 closure guard
Vault 2b.1 foundation guard
Vault 2b.2 PrivacyCoordinator + plaintext authority guard
Vault 2b.3 PrivacyPublicationToken guard
Vault 2b.4 granular protected-note cutover guard
```

The 2b.4 guard now additionally rejects routine-save full-graph hydration, full-history traversal during revision append, metadata/attachment mutations that decrypt unrelated current-state objects, and absence of an authenticated small-object corruption regression.

## Architecture regression QA

The cumulative static architecture chain through **A1–A7 and B1–B6** passed. This includes:

- UI/application/storage boundary guards;
- application execution/error boundaries;
- domain/native/PDF decomposition guards;
- editor workflow boundaries;
- canonical editor-document specification checks;
- first-party JavaScript and shell syntax validation;
- typed-error/storage-owned search SQL guards.

A WIP regression was found during this sweep: revision-preview UI imported a storage resource directly. It was fixed with an application-owned opaque wrapper before closure.

## Dependency-free behavioral QA

`scripts/run-browser-behavior-tests.mjs` passed **36/36 tests** with editor-build dependencies absent. Coverage included editor UI contracts, note-list resilience, PDF behavior, PDF runtime integrity, and PDF layout integrity.

The cumulative B5/B6 dependency-free PDF suites also passed during architecture validation.

## Targeted 2b.4 adversarial coverage present in Rust source

The source contains executable Rust regressions for, among other inherited and 2b.4-specific cases:

- granular mutation object-ID locality;
- root/manifest transaction rollback safety;
- missing current component fail-closed behavior;
- authenticated granular-object ciphertext tampering;
- immutable revision append and retention/pruning;
- unreachable encrypted-object garbage collection;
- protected revision restore without plaintext-table repopulation;
- protected ↔ unprotected ↔ protected cycles and residue rules;
- encrypted attachment round-trip;
- tampered attachment authentication with no untracked plaintext;
- protected reader/Lock lifetime races;
- protected attachment/native-operation lifetime races.

These Rust tests could not be executed here because the Rust toolchain is unavailable; their source presence and static guards were verified.

## Direct source sweeps

Passed:

- no production `ProtectedNotePayload`;
- no `protected_notes` / `protected_note_blobs` production reference outside the intentional migration schema;
- no routine protected-save `read_vault_current_graph`;
- no routine protected-save revision-chain traversal;
- no WIP/TODO/FIXME/HACK or `todo!`/`unimplemented!` markers on the audited 2b.4 surfaces;
- direct Vault object/root writers remain concentrated in `storage/vault_objects.rs`, with expected canonical graph deletion during Unprotect and test fixtures;
- UI no longer imports storage implementation types for revision preview authorization.

## Native release gate to run on macOS

From the source root:

```bash
cargo fmt --all
cargo fmt --all -- --check

cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked

RUSTNOTE_E0_STATIC_ONLY=1 \
RUSTNOTE_E1_STATIC_ONLY=1 \
RUSTNOTE_E2_STATIC_ONLY=1 \
RUSTNOTE_E3_STATIC_ONLY=1 \
./scripts/validate-security-e0-e3-closure.sh

./scripts/validate-vault-2b1-foundation.sh
./scripts/validate-vault-2b2-privacy-authority.sh
./scripts/validate-vault-2b3-publication-token.sh
./scripts/validate-vault-2b4-granular-cutover.sh

node scripts/run-browser-behavior-tests.mjs
```

For the full native architecture gate after dependencies/runtime preparation, run the repository's normal preparation workflow and cumulative architecture validators as well.


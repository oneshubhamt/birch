# QA — Vault 2b.12 AEGIS/privacy-disclosure closure

## Scope

Targeted verification for the fixed AEGIS-256 v1 profile, Argon2 hostile-envelope resource bound, and truthful backup privacy disclosure introduced in 2b.12.

## Static acceptance

Run:

```sh
./scripts/validate-vault-2b12-aegis-privacy-disclosure-closure.sh
```

The guard verifies:

- build identity and 2b.12 closure records;
- exactly pinned `aegis`/Argon2/getrandom/zeroize dependencies;
- absence of the AEGIS `pure-rust` feature in the production Vault manifest;
- 256-bit key, nonce, and full authentication tag plus 128-bit Argon2 salt;
- 192-bit protected-blob random prefix + 64-bit counter geometry;
- authenticated AEGIS format/context/chunk metadata;
- fixed Argon2id 64 MiB / 3 / 4 profile and the identical hostile-envelope ceiling;
- OS CSPRNG use;
- the 2b.12 E0 product-decision override;
- read-only post-Protect backup privacy-reset status reporting;
- conservative local-encryption UI wording for pending/unknown backup state;
- persistence of the Backup-settings warning.

## Native tests added/affected

`rustnote-vault` includes a regression asserting that the v1 Argon2 profile is also the hostile-envelope resource ceiling.

`note_card` includes regressions asserting that:

- a pending backup reset discloses the earlier-plaintext-copy risk;
- a failed post-commit status refresh produces conservative backup wording.

These tests must be executed on a development machine with the Rust toolchain.

## Full inherited regression

The release candidate must also pass the inherited Vault 2b.1–2b.11 validators and cumulative E0–E3 static/adversarial closure guards.

## Environment limitation

If `cargo`, `rustc`, `rustfmt`, or Clippy are unavailable, their gates are **unexecuted**, not passed. The user will run the native Rust release gate locally.

## Verification completed in this implementation environment

Passed on the final 2b.12 source tree:

- `validate-vault-2b12-aegis-privacy-disclosure-closure.sh`;
- all inherited Vault 2b.1–2b.11 static/adversarial validators;
- Security E0, E1, E2 and E3 static contracts;
- cumulative E0–E3 2a.16 ownership, 2a.17 residue/backup, 2a.18 UI lifetime/revision restore and 2a.19 independent adversarial closure;
- Architecture A1–A7 static gates;
- cumulative Architecture B6→B1/A1–A7 static chain;
- dependency-free PDF/editor/browser behavioral tests reached by the B-chain;
- shell syntax for every first-party `scripts/*.sh` file.

Not executed here because the environment has no Rust toolchain:

- `cargo fmt --all -- --check`;
- `cargo metadata/check/clippy`;
- `cargo test -p rustnote-vault` and workspace Rust tests;
- Architecture A0/native Rust safety net.

# QA — Vault 2b.11 Atomic Privacy Authority Closure

## Targeted adversarial coverage

The source contains deterministic regression tests/guards for:

- protected compound plaintext operation vs admitted Lock;
- ordinary compound plaintext operation vs admitted Protect;
- multi-note protected reader-set acquisition vs whole-library Restore admission;
- retention of derivative + Vault authorization for the complete sensitive operation lifetime;
- absence of production caller-side derivative-then-Vault choreography;
- absence of a reverse storage-lock -> privacy-admission edge;
- direct revision handle lookup without revision-chain traversal;
- rejection of a valid same-note orphan revision object that is not retained by the authenticated revision index;
- revision-index locality: ordinary autosave does not open/rewrite the revision index;
- confinement of durable `OwnedPlaintext` constructors to canonical durable-ledger modules.

## Available validation gate

Run from the source root:

```sh
./scripts/validate-vault-2b11-atomic-privacy-authority-closure.sh
./scripts/validate-vault-2b10-cross-cutting-invariant-closure.sh
./scripts/validate-vault-2b9-backup-ownership-closure.sh
./scripts/validate-vault-2b8-plaintext-authority-closure.sh
./scripts/validate-vault-2b7-closure-repair.sh
./scripts/validate-vault-2b6-independent-closure.sh
./scripts/validate-vault-2b5-consolidation.sh
./scripts/validate-vault-2b4-granular-cutover.sh
./scripts/validate-vault-2b3-publication-token.sh
./scripts/validate-vault-2b2-privacy-authority.sh
./scripts/validate-vault-2b1-foundation.sh
./scripts/validate-security-e0-e3-ownership-2a16.sh
./scripts/validate-security-e0-e3-residue-2a17.sh
./scripts/validate-security-e0-e3-ui-lifetime-2a18.sh
./scripts/validate-security-e0-e3-adversarial-closure-2a19.sh
./scripts/validate-security-e0-e3-closure.sh
./scripts/validate-architecture-a1-a4-closure.sh
./scripts/validate-architecture-a5.sh
./scripts/validate-architecture-a6.sh
./scripts/validate-architecture-a7.sh
./scripts/validate-architecture-b1.sh
./scripts/validate-architecture-b2.sh
./scripts/validate-architecture-b3.sh
./scripts/validate-architecture-b4.sh
./scripts/validate-architecture-b5.sh
./scripts/validate-architecture-b6.sh
```

Browser/PDF dependency-free behavior checks are included by the relevant architecture validators.

## Native release gate

This execution environment does not provide `cargo`, `rustc`, or `rustfmt`, so native compilation/clippy/test results must not be represented as verified here.

On the target Mac, run the project's normal native release gate, including formatting, locked/offline workspace preparation as appropriate, `cargo check`, `cargo clippy -- -D warnings`, tests, and macOS packaging validation before treating the source as a native release binary.

## Acceptance

2b.11 is statically closed in this environment only if all available validators above pass on the packaged source and archive extraction is byte-for-byte equivalent to the staged source tree. Native release certification remains conditional on the Mac Rust toolchain gate.

## Verification result in this environment

The final staged 2b.11 tree passed:

- Vault 2b.11, 2b.10, 2b.9, 2b.8, 2b.7, 2b.6, 2b.5, 2b.4, 2b.3, 2b.2, and 2b.1 static/adversarial guards;
- E0-E3 2a.16 ownership, 2a.17 residue/backup, 2a.18 UI lifetime/revision restore, 2a.19 adversarial, and cumulative static closure guards;
- A1-A7 and A1-A4 closure static architecture guards;
- B1-B6 static architecture guards;
- dependency-free editor/PDF browser behavior tests exercised by B4-B6;
- delimiter/source-structure sanity over every Rust file changed from 2b.10.

A0/full Rust acceptance could not execute because this sandbox has no Rust toolchain. That limitation is intentionally not converted into a pass.

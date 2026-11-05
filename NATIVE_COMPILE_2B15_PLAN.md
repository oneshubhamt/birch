# Rustnote 2b.15 — Transport Visibility + Warning Closure Plan

## Audit findings
The second native compile run reached the binary target and exposed a crate-boundary defect: `src/attachment_protocol.rs` was calling library-internal `pub(crate)` plaintext/private-temp APIs. The same run also surfaced warnings that would fail the intended `cargo clippy ... -D warnings` gate.

## Objectives
1. Keep `PlaintextOperationPermit`, `OwnedPlaintext`, and `native::private_temp` non-public.
2. Replace binary access to security internals with narrow public transport-facing opaque guards.
3. Preserve I1: ownership must exist before a protected preview directory is materialized.
4. Keep ingress authorization alive for the complete image-staging lifetime without exposing the underlying coordinator permit.
5. Remove the exact dead-code/private-interface warnings reported by native `cargo check` instead of suppressing them with blanket `allow` attributes.
6. Preserve all 2b.11–2b.14, E0–E3, granular Vault, and publication-lifetime invariants.
7. Advance build identity and add a targeted regression guard.

## Out of scope
No Vault format, crypto algorithm/KDF, database schema, editor behavior, revision model, or feature semantics are changed.

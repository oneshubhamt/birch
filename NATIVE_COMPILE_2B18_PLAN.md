# Rustnote 2b.18 — Strict Clippy compile-closure plan

## Audit finding
Native `cargo check` / strict Clippy on 2b.17 exposed one moved-value test defect, two binary-test crate-boundary calls to the private protected-note lock helper, and a finite set of `-D warnings` Clippy findings. The findings do not require a Vault-format, privacy-policy, or persistence redesign.

## Objectives
1. Retain only the protected note identifier before an owning checkpoint save consumes the test `Note`; do not clone the complete note.
2. Keep `Storage::lock_protected_note` crate-private and give feature-gated binary tests a narrow hidden test façade.
3. Replace Clippy-reported oversized function signatures with cohesive request/context values at attachment, protected-blob, editor workflow, attachment-action, and close-durability boundaries.
4. Apply the reported semantic-preserving Clippy simplifications (`let` chains, explicit OpenOptions truncation intent, auto-deref cleanup, `?`, and `then_some`).
5. Add a targeted 2b.18 static guard and preserve the full inherited 2b/E0–E3 validator chain.
6. Make no cryptographic format, database schema, authorization-ordering, or user-visible behavior changes.

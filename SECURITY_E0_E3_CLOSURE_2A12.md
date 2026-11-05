# Rustnote 0.12.5 build 2a.12 — E0–E3 security-boundary consolidation

This pass is deliberately bounded. It preserves the existing E0–E3 cryptographic and persistence architecture and consolidates the ten concrete security-boundary defects identified after 2a.11. It does not expand Vault into a general defence against arbitrary removable-volume, hostile-filesystem, or cross-user operating-system policy.

## Unified plaintext-derivative transition barrier
A note-scoped `PlaintextDerivativeGate` blocks new plaintext derivative work during protect/unprotect transitions and waits for already-authorized work to finish. Producers retain owned permits for their actual lifetime, including background media workers.

## Protected image/PDF staging
Image-edit and annotated-PDF staging re-authorize the current attachment/note state after entering the derivative boundary and reject protected content. Staging records note/attachment ownership so targeted cleanup can remove pre-transition material.

## Cache generation / encryption races
Protection acquires the derivative transition before the storage mutation lock, drains in-flight producers, holds the transition through canonical commit and residue cleanup, and releases it only when later producers can re-authorize against the new state.

## Fail-closed managed-cache cleanup
Image rendition, media compatibility, targeted staging, attachment handoff, backup-repository reset and SQLite residue cleanup return errors to Vault finalization. The E3 managed-residue marker advances to v5 only after these managed cleanup steps succeed.

## Sensitive-temp parent ownership
Sensitive temp ownership no longer applies private permissions to an arbitrary caller-supplied/system parent. Rustnote creates and hardens only its own temporary namespaces/session children.

## Backup ownership identity and constructor invariant
Every library has a durable library identity. Automatic backup repositories carry an immutable owner marker bound to that library, and the canonical `Storage::at_with_backup` construction path validates/establishes repository ownership and persists backup-location history.

## New-format temp-session crash semantics
V2 sensitive-temp sessions use an OS-held owner lock plus a cleanup-claim marker. Markerless v2 sessions are abandoned rather than trusted by PID; legacy PID heuristics remain migration-only.

## Key-copy and zeroization hygiene
Random and password-unwrapped Vault keys are written directly into `Zeroizing<[u8; 32]>` storage rather than passing through avoidable ordinary stack key copies.

## Filename/privacy logging hygiene
Attachment/media diagnostic paths changed in this pass do not emit attachment filenames. Persistent WebView caching of attachment/media bytes is disabled with `no-store` so Rustnote does not create an untargetable plaintext browser cache.

## Password-memory hygiene
Rust-owned password inputs wipe replaced/cancelled values and move submitted text into `SecretString`; browser/DOM transient copies remain an unavoidable UI-runtime boundary.

## Bounded scope
2a.12 fixes the ten agreed architectural defects and immediate correctness consequences. It intentionally does not add speculative removable-drive availability policy or unrelated filesystem threat models. Native Rust compilation, Clippy and executable Rust tests remain an external acceptance gate when the toolchain is unavailable in the implementation environment.

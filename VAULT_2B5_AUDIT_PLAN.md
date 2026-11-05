# Rustnote Vault 2b.5 — Consolidation & Performance Closure Audit Plan

## Governing baseline

Pass 2b.5 is executed against the original **Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model** contract, with the completed 2b.4 tree as the code baseline. Earlier validators are treated as regression tools, not as proof of completion.

## Audit findings carried into this pass

The de-novo pre-2b.5 audit found four closure defects that must be repaired before consolidation can be certified:

1. note/library transition admission did not prevent a new Vault reader/plaintext producer from entering before revocation/drain began, and protected revision Restore was not a first-class note transition;
2. several protected-derived Dioxus state handoffs validated a publication token before `Signal::set` but did not hold the coordinator publication lease during the actual handoff;
3. `ProtectedPublicPersistenceSpec` classified private persistence but did not mechanically drive/validate every scrub action;
4. the 2b.4 behavioral matrix lacked direct locality/restart coverage for several protected metadata/attachment/revision surfaces and retained one stale attachment expectation.

## Finite 2b.5 objectives

### O1 — Repair inherited 2b.1–2b.4 contract misses
- make reader/plaintext-producer admission linearize with `PrivacyCoordinator` transition admission;
- make `Restore` a first-class `NotePrivacyTransition` and route protected revision restore through it;
- make actual protected-derived Dioxus state handoffs acquire the publication lease;
- make the canonical sensitive-data authority mechanically drive the ordinary-persistence scrub plan;
- finish the missing 2b.4 behavioral/adversarial cases.

### O2 — Consolidate and delete superseded machinery
Audit the changed tree for:
- dead transition code;
- obsolete/privacy-duplicate generation logic;
- old staging/recovery helpers;
- obsolete monolithic protected-note paths;
- duplicate filesystem cleanup/rename logic;
- duplicate revision persistence;
- development-build compatibility code no longer required for the current architecture;
- security-sensitive raw filesystem operations on Rustnote-managed plaintext.

Remove only machinery that is genuinely superseded. Current crash-recovery journals, startup cleanup obligations, and the UI-local pre-admission revocation window are not compatibility code and must remain.

### O3 — Performance closure by architecture
Verify algorithmic behavior for:
- startup;
- ordinary note open/save/search;
- protected unlock;
- protected typing/autosave;
- revision creation/restore;
- attachment ingest/open;
- Lock/Protect/Unprotect.

No speculative micro-optimization. Fix only measured or algorithmically obvious regressions.

### O4 — Re-audit all seven permanent invariants
Reconstruct I1–I7 directly from source after the changes, with special attention to ownership transfer, transition linearization, publication handoff, deny-by-default persistence, and granular mutation locality.

### O5 — QA and release gate
Run all QA available without the native Rust toolchain:
- strengthened 2b.1–2b.4 validators;
- inherited E0–E3 / 2a.16–2a.19 static-adversarial closure;
- A1–A7 + B1–B6 static architecture closure;
- first-party JS/shell syntax validation;
- dependency-free browser/editor/PDF behavior suite;
- targeted source sweeps for alternate writers/raw managed-plaintext operations.

Native `cargo check`, `cargo clippy`, and Rust tests remain a mandatory Mac release gate and must not be represented as executed in an environment without Rust.

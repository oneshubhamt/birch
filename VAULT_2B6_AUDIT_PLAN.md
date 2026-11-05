# Rustnote Vault 2b.6 — Independent De-Novo Architectural/Security Closure

Baseline: `rustnote_v0.12.5_build_2b.5.3_consolidation_performance_closure`

Governing source: the original Rustnote Vault 2b architecture contract. Prior 2b.1–2b.5 validators are regression evidence only and are not accepted as proof for this pass.

## De-novo audit findings before closure edits

The independent source reconstruction found two defects that require local repair; neither requires an architectural contract amendment.

1. **I2 fail-closed absence proof defect.** Several durable plaintext-retirement paths used `Path::exists()`. Rust's `exists()` maps metadata errors to `false`, so a permission/I/O/path-resolution failure could be mistaken for proof that the predecessor plaintext no longer existed. In the uncreated-reservation and restore-transfer paths, this could allow the durable cleanup row to be retired without conclusive filesystem evidence.
2. **Encrypted-object commit authority leak.** `commit_vault_manifest_root` accepted a caller-supplied `superseded_mutable_objects` list. Current callers assembled it correctly, but the authoritative commit primitive itself could be instructed to delete an immutable revision or a still-referenced object. That left object-lifecycle correctness partly as a caller convention instead of a property of the object-store boundary.

The independent sweeps did not find a separate transition authority, a second protected-note persistence representation, persistent protected FTS plaintext, or a protected-data UI handoff that bypasses the publication lease at the actual plaintext publication boundary.

## Finite 2b.6 objectives

1. **Close I2 at the typed plaintext authority.** Replace ambiguous `Path::exists()` decisions in production `OwnedPlaintext` lifecycle code with no-follow metadata checks that distinguish `NotFound` from every other error. Ensure durable owner-row retirement rolls back if absence cannot be proved.
2. **Make encrypted-object retirement coordinator-owned.** Derive superseded current-state object IDs from the old/new manifests inside `commit_vault_manifest_root`; remove the caller-supplied deletion list so revision/history objects cannot enter ordinary mutable-object retirement.
3. **Reconstruct and verify I1–I7 independently.** Complete plaintext create/destroy/transfer, transition, publication, object-commit, revision, attachment, backup/import/export, autosave, SQLite/WAL/FTS, restore, hard-delete, corruption/recovery and concurrency sweeps directly from production source.
4. **Perform the six mandatory final sweeps.** Direct SQLite writers; direct sensitive filesystem writers; privacy-transition callers; raw `PathBuf` plaintext lifecycle; UI publication; obsolete `ProtectedNotePayload`/dead-code representation.
5. **Add independent 2b.6 guardrails.** Add focused regression coverage for metadata-error-as-absence and a new static/dependency-free validator derived from I1–I7 rather than chaining prior 2b validators.
6. **Run targeted and full available regression QA.** Run the independent validator, inherited Vault/security architecture validators, shell syntax checks and browser behavior suite. Record native Rust toolchain checks as an explicit external release gate if this environment still lacks `cargo`/`rustfmt`.
7. **Re-audit the changed source and package only after objectives pass.** No closure declaration before the source-level objectives and available QA are both green.

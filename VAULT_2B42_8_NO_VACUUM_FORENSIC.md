# Vault 2b.42.8 — No-VACUUM Forensic Closure

## Purpose

Determine whether Rustnote's E3 post-Protect plaintext-residue guarantee requires SQLite `VACUUM`, and make the production decision from forensic evidence rather than performance assumptions.

2b.42.7 profiling showed that `VACUUM` cost only tens of milliseconds. Therefore 2b.42.8 is primarily a security/forensic simplification pass, not a meaningful performance optimization.

## Experiment

The first 2b.42.8 checkpoint kept production behavior unchanged and introduced a `#[cfg(test)]` no-VACUUM Protect path. Every Protect stage remained identical except the SQLite compaction step.

The native regression deliberately created:

- a historical-only plaintext marker retained through revision history;
- a different current plaintext marker;
- a Rustnote-managed automatic backup proven to contain historical plaintext before Protect.

After no-VACUUM Protect it inspected the database file and WAL separately at byte level, verified the pre-Protect managed backup was invalidated, created a fresh post-Protect managed snapshot, and scanned the managed repository again.

## Native decision result: PASS

On macOS, the decisive Rust regression passed:

`storage::protection::tests::no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups`

Result supplied from the native run:

- 1 passed;
- 0 failed;
- 290 filtered out;
- 2b.42.8 source validator: 8/8 PASS.

The evidence therefore supports that the tested E3 boundary does not require `VACUUM` when Rustnote already performs secure deletion, rebuilds FTS5 from current live rows, and verifies WAL truncation.

Canonical security sequence: secure_delete + FTS5 rebuild + verified WAL TRUNCATE.

## Production decision: remove per-Protect `VACUUM`

The final canonical production SQLite residue path is:

```text
PRAGMA secure_delete = ON
+ PRAGMA synchronous = FULL
→ FTS5 rebuild from live rows
→ verified WAL TRUNCATE
```

There is no alternate scrub mode, no runtime toggle, no environment variable, no feature flag, and no test-only Protect implementation. The experiment scaffolding is removed so there is one production authority.

The former second WAL truncation is also removed. It existed only after `VACUUM`; once `VACUUM` is gone there are no intervening writes between the two checkpoints, so retaining a duplicate truncation would add no security property.

## Security invariants retained

Removing `VACUUM` does not change:

- `PRAGMA secure_delete = ON` at the privacy boundary;
- full synchronous durability at the forensic scrub boundary;
- FTS5 rebuild from current live rows;
- verified `wal_checkpoint(TRUNCATE)` with busy/frame validation;
- protected-object encryption;
- attachment retirement and managed plaintext ownership;
- cache/staging/handoff cleanup;
- automatic-backup repository invalidation;
- privacy-transition admission/drain behavior;
- canonical publication semantics.

The forensic regression remains and now exercises the canonical production Protect path itself.

## Performance implication

The expected improvement is intentionally small: profiling measured `VACUUM` at roughly 13–26 ms per Protect. The reason to remove it is that native evidence found it redundant for the tested E3 postcondition, allowing the architecture to be simpler without weakening the demonstrated security boundary.

## Remaining closure

Because the production source was canonicalized after the experimental native PASS (experimental mode removed, obsolete profiler stage removed, duplicate post-VACUUM WAL checkpoint removed), the final source must still pass native format/check/Clippy, the production forensic regression, and the full Rust suite before 2b.42.8 is declared fully closed. 2b.42.10 will independently re-audit the broader E0–E3/I1–I7 residue model.

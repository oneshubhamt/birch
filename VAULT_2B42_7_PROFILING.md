# Vault 2b.42.7 — Profiling only

## Scope

This pass adds opt-in timing telemetry only. It does not optimize, remove, reorder, bypass, or weaken any Vault/privacy/storage operation.

Enable profiling for a process with:

```bash
RUSTNOTE_VAULT_PROFILE=1
```

Any other value, including an unset variable, keeps profiling disabled.

## Output contract

Profiling writes timing lines to stderr in this form:

```text
[rustnote-vault-profile] kind=stage name=privacy_drains elapsed_us=1234
[rustnote-vault-profile] kind=operation name=protect elapsed_us=5678
```

The vocabulary is closed in Rust enums. Output contains only:

- static operation/stage names;
- elapsed microseconds.

It never includes note IDs, titles, passwords, paths, attachment names, hashes, byte counts, content, or other user-derived values.

## Measured boundaries

End-to-end operation spans:

- `protect`
- `lock`
- `unlock`
- `remove_encryption`

Nested stages:

- `ui_checkpoint`
- `autosave_barrier`
- `autosave_release`
- `privacy_drains`
- `protect_preparation`
- `crypto`
- `attachments_encrypt`
- `attachments_stage`
- `attachments_cleanup`
- `managed_residue_scrub`
  - `cache_cleanup`
  - `staging_cleanup`
  - `handoff_cleanup`
  - `backup_repository_reset`
  - `sqlite_residue_scrub`
- `transition_finalize`
- `attachments_decrypt`
- `attachments_finalize`
- `object_writes`
- `fts`
- `wal`
- `vacuum`
- `canonical_publication`

The attachment stages are intentionally decomposed because Protect, Lock and Remove Encryption have different attachment lifetimes. `managed_residue_scrub` is a parent span added after native 2b.42.7 measurements showed that the previous profiler stopped `attachments_cleanup` before the broader post-Protect privacy scrub. Its cache/staging/handoff/backup/SQLite children expose that formerly residual security cost without changing cleanup order. `sqlite_residue_scrub` contains the existing nested `fts`, `wal`, `vacuum`, `wal` sequence. Repeated `wal` lines represent the pre- and post-VACUUM truncation checkpoints.

## Disabled-path cost

The environment variable is read once through `OnceLock<bool>`. Each span then performs a cached boolean check. When disabled, no `Instant` is created and no formatting or output occurs.

## Deliberately deferred

2b.42.7 makes no performance claim and performs no optimization. In particular, the production FTS/WAL/VACUUM sequence is unchanged. Native release measurements from this instrumentation are inputs to later performance decisions and the 2b.42.8 forensic experiment.

## Reading a profile

The operation total is intentionally broader than the listed stages. After the first native profile, `protect_preparation`, `managed_residue_scrub`, `transition_finalize`, and autosave barrier/release spans were added specifically to reduce the unexplained Protect residual. The remaining difference can still include executor scheduling, UI bookkeeping, status refreshes and—while profiling is enabled—the small cost of writing nested timing lines. Do not expect nested values to sum exactly to the operation total.

For the cleanest native measurements, redirect output to a file rather than relying only on an interactive terminal. Repeated measurements are more useful than a single run.

# Rustnote 0.12.5 build 2a.17 — persistent residue and backup closure

## Purpose

2a.17 closes the persistent-residue defects identified after canonical plaintext ownership: historical/offline automatic-backup repositories and attachment `.orphaned` quarantine. It deliberately preserves the 2a.16 portability staging authority rather than introducing a duplicate cleanup system.

## Backup privacy-obligation model

Backup-location history is now version 3 and stores a bounded `pending_privacy_reset_directories` set per durable library identity. Vault cleanup publishes that obligation before touching repositories.

For each remembered location:

1. publish the pending privacy obligation durably;
2. if the repository is unavailable, leave it pending and allow local protection to finish;
3. if available, acquire the backup-directory ownership lock;
4. require positive Rustnote ownership proof rather than trusting the parent path alone;
5. purge the owned repository;
6. publish only minimal clean-state owner/index metadata;
7. clear the pending obligation only after the cleanup and clean-state publication succeed.

This ordering prevents an unplugged external volume from being silently skipped and prevents an empty local directory created at the same mount path from being mistaken for the returned repository.

## Current removable-volume disappearance

The currently configured backup directory may disappear after `Storage` opens. Vault reset therefore revalidates the current parent before cleanup and never recreates a vanished mount path. If it is gone or cannot be proven, the privacy obligation stays pending.

The live `Storage` object then refuses ordinary backup reconciliation, snapshot creation, retention pruning, snapshot inspection and restore while its current repository is privacy-pending. `backup_repository_status` remains safe and reports the unresolved count without touching or recreating the repository. Reopening after the real repository returns reacquires a fresh directory lock and discharges privacy cleanup before normal use.

The Settings location writer applies the same rule: a missing privacy-pending backup path is not created merely by selecting/saving it.

## Attachment quarantine

New protection collects the plaintext SHA-256 identities of protected attachments and strictly scans `attachments/.orphaned` for deterministic quarantine names ending in those identities. Matching regular-file/symlink artifacts are removed; unrelated quarantine entries remain. A matching unexpected filesystem object or failed deletion is an integrity failure, so the protection journal remains recoverable rather than declaring cleanup complete.

For libraries that already reached older E3 markers, the mapping from old protected notes to their former plaintext hashes is intentionally absent from ordinary SQLite. The v6 migration therefore performs one conservative Rustnote-owned `.orphaned` quarantine clear, then completes the usual cache/handoff/backup/SQLite scrub and records `vault.e3_privacy_scrub = 6`.

## Portability/import staging

The 2a.16 audit was rechecked. `.portability-import-*.rustnote` staging and extracted bundle blobs already use `managed_plaintext_staging` before the first plaintext write, retain ownership on cleanup failure, and transfer ownership transactionally to canonical references. 2a.17 leaves that architecture unchanged and guards it as an inherited invariant.

## Targeted regressions

2a.17 adds or strengthens coverage for:

- an offline historical repository remaining privacy-pending while local Vault cleanup succeeds;
- Settings/open refusing to recreate a missing pending mount path;
- an empty substitute parent failing positive repository proof;
- a reconnected historical repository being purged before normal use and then clearing its obligation;
- the currently configured repository disappearing after open without being recreated, while normal backup work is blocked;
- backup status exposing unresolved privacy-reset count without touching a pending current repository;
- matching `.orphaned` content-hash residue being removed while unrelated quarantine survives;
- E3-v6 upgrade clearing legacy orphan quarantine when historical protected-hash ownership is no longer targetable;
- inherited 2a.16 portability ownership remaining authoritative.

## Scope boundary

2a.17 does **not** claim final E0–E3 closure. The remaining planned work is 2a.18 for UI/async plaintext lifetime and protected revision restore, followed by the independent 2a.19 adversarial closure audit.

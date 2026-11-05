# Rustnote 2a.10 — E0–E3 Behavioral Final Closure

Build: **0.12.5 / 2a.10**

## Purpose

2a.10 closes the defects found by a fresh behavioral audit of the 2a.9 package. It does not change the vault wire format, AEGIS-256/Argon2id design, schema version 17, ProseMirror editor architecture, PDF.js runtime, or ordinary attachment architecture. E0 hardware acceptance remains treated as resolved by project decision.

## Attachment-schema invariant closure

Protected attachment rows remain opaque routing placeholders while sensitive metadata lives in the authenticated protected payload. The ordinary `attachments` table requires `(embedded = 0) => unreferenced_at IS NOT NULL`; protection therefore writes the protection-transition timestamp into `unreferenced_at`. Protected-note pruning already excludes rows owned by `protected_notes`, so satisfying the schema invariant does not permit the placeholder to be deleted.

A direct Rust regression must protect a note containing a real attachment and prove that the protection transaction commits under the actual schema CHECK constraint.

## FTS5 forensic-residue closure

Deleting the protected note's logical `notes_fts` row is not treated as sufficient forensic cleanup. FTS5 can retain obsolete terms in segment/shadow-table history. The E3 SQLite scrub therefore performs:

1. `PRAGMA secure_delete = ON` and `synchronous = FULL`;
2. `INSERT INTO notes_fts(notes_fts) VALUES('rebuild')` from current live rows;
3. WAL truncate/checkpoint;
4. `VACUUM`;
5. final WAL truncate/checkpoint.

This retains FTS5 for ordinary content while reconstructing the live index without deleted protected terms.

## Existing-library E3-v4 migration

The privacy marker advances from `vault.e3_privacy_scrub = 3` to **`4`**. A 2a.9/v3 library therefore receives the stronger FTS cleanup on first 2a.10 initialization.

The one-time SQLite forensic scrub runs even if no protected notes currently remain, because a note protected and later deleted or unprotected under an earlier build can still have left obsolete FTS segments. Broader cache/handoff/backup namespace reset remains limited to libraries that currently contain protected notes.

## Persistent-snapshot fail-closed boundary

No Rustnote-managed persistent database snapshot may be produced while `vault_operations` is non-empty. One canonical check is placed in the database-snapshot primitive used by:

- automatic backup snapshots;
- safety snapshots;
- complete archive export;
- restore rollback snapshots.

Snapshot validation also rejects a candidate archive/snapshot database that itself contains unfinished vault operations. Thus a post-commit/privacy-cleanup failure cannot be preserved into another durable Rustnote artifact before recovery finishes.

## Behavioral regression contract

Executable Rust QA must directly cover:

- attachment-bearing note protection committing successfully under the schema constraint;
- a unique protected plaintext marker absent from the active SQLite database and WAL after E3 cleanup;
- that same marker absent from a subsequent automatic-backup database snapshot;
- an existing v3/2a.9 library receiving the v4 FTS rebuild cleanup and marker upgrade;
- automatic backup and complete archive export refusing an unfinished vault operation;
- the existing attachment round-trip, crash recovery, autosave barrier, Lock revocation, PDF/media descriptor lifetime, private staging, payload validation, tag-filter cleanup, and backup-ownership regressions remaining intact.

## Definition of done

2a.10 source closure requires all E0–E3 static contracts plus the cumulative 2a.10 closure validator to pass, schema 17 to remain unchanged/equivalent, the new behavioral Rust tests to exist, all earlier 2a.6–2a.9 hardening invariants to remain present, and native Cargo/Clippy/test/audit acceptance to be run on a Rust-equipped development machine before final executable release acceptance.

# Rustnote 2a.11 — E0–E3 Adversarial Lifecycle Closure

Build: **0.12.5 / 2a.11**

## Purpose

2a.11 closes the three lifecycle defects found by the de novo audit of 2a.10: ordinary protected-read publication racing Lock, PID-reuse sensitive-temp cleanup, and automatic-backup locations forgotten after configuration changes. It does not change schema version 17, the AEGIS-256/Argon2id vault format, ProseMirror, PDF.js, or the normal-note storage model.

## Protected-read revocation and publication contract

A protected payload read acquires one `VaultReadPermit` before decryption and retains it while the payload is validated and transformed into the value the storage operation intends to publish. `Lock` publishes `revoking = true` before waiting, refuses new permits, and waits for active protected operations to drain. Immediately before a protected read leaves the storage operation, `read_permit_is_publishable` verifies that the same note/session generation is still current and not revoking; a read that lost the race is discarded rather than returned.

The application boundary independently invalidates note-list and note-selection generations before Lock, redacts the protected note from the loaded list cache, and unmounts the selected note before the synchronous revocation wait. Already-dispatched asynchronous list/selection work therefore cannot repopulate live UI state after Lock. This is the enforceable Rustnote-internal publication guarantee; it does not claim retroactive deterministic erasure of arbitrary allocator/WebView copies already produced while the note was legitimately unlocked.

A deterministic Rust regression forces an in-flight protected payload read to overlap Lock and proves both that Lock waits for the scoped read and that the late result is suppressed once revocation is visible.

## Sensitive temporary ownership contract

PID existence is no longer accepted as proof that a sensitive temporary directory belongs to a live Rustnote process. New vault, image, PDF and attachment-handoff sessions use a random per-process/session directory containing a private owner file held under the standard-library exclusive file-lock API. The operating system releases that lock on both normal file close and hard process death, allowing cleanup to distinguish live from abandoned sessions without trusting a reusable PID.

Legacy PID-only directories remain migration-compatible. A legacy directory carrying the current process PID is explicitly stale, because the new process did not create legacy PID-only state. The cleanup parser also recognizes a `session-<pid>-<token>` directory whose owner setup was interrupted. No sensitive path is returned to callers until the owner file is locked, so plaintext cannot legitimately be published into that setup window.

A full E3 handoff scrub deletes plaintext payloads but preserves a live session's owner artifacts, so privacy cleanup cannot unlink the very lock used to prove that another current operation is alive.

## Historical automatic-backup ownership contract

Rustnote persists a bounded privacy ledger of automatic-backup locations keyed by the canonical library data directory. The previous and next locations are durably recorded before a storage-location configuration update can forget the old path; normal `Storage::open` also records the resolved location.

A backup directory may belong to only one library identity in this ledger. Attempting to associate one backup folder with a different library fails rather than silently transferring ownership. This prevents a later Vault cleanup for library A from deleting a repository subsequently reused by library B.

When protection changes, E3 resets every remembered Rustnote-owned repository for the current library. Historical parents are temporarily covered by the existing backup-directory ownership locks before deletion. Rustnote removes only the dedicated `Rustnote Repository` child (or the already-supported fully proven legacy root artifacts), never the user-selected parent or unrelated sibling files. Only the current repository is recreated.

## Behavioral regression contract

Executable Rust QA must directly cover:

- Lock waiting on an in-flight scoped protected payload read and suppressing its late publication;
- live sensitive-temp ownership preventing cleanup;
- abandoned owner-lock sessions being removed without PID heuristics;
- legacy/current-PID reuse cleanup;
- the owner-setup crash window for `session-<pid>-<token>` directories;
- full attachment-handoff scrub preserving a live ownership lock while deleting plaintext payloads;
- backup history retaining multiple locations for one library while remaining library-scoped;
- cross-library reuse of one backup repository being rejected;
- historical Rustnote repository purge preserving unrelated files in the user-selected parent;
- all inherited 2a.6–2a.10 crash, autosave, backup, FTS, snapshot, attachment, PDF/media and tag-filter regressions.

## Definition of done

2a.11 source closure requires the E0/E1/E2/E3 static contracts and cumulative 2a.11 closure validator to pass, schema 17 and cryptographic formats to remain unchanged, all new adversarial regressions to exist, architecture/JS/schema/static QA to remain clean, and the pinned Rust 1.97.1 `fmt`, `check`, `clippy`, workspace tests and dependency audit to pass on a Rust-equipped machine before executable-release acceptance.

# Rustnote 2a.9 — E0–E3 Persisted Tag-Filter Final Closure

Build: **0.12.5 / 2a.9**

## Scope

2a.9 is a narrow closure pass over 2a.8. E0 real-device acceptance is treated as resolved by project decision. The vault wire format, AEGIS-256/Argon2id design, schema version 17, protected-payload format, ProseMirror editor architecture, PDF.js runtime, ordinary attachment model and 2a.8 descriptor-lifetime fixes remain unchanged.

## Persisted tag-filter privacy contract

`settings.note_list_filter` is Rustnote-managed persistent state and may contain literal tag labels. When protection removes a note's tag memberships, any selected tag that no longer exists in the public tag catalog must be removed from this setting inside the same SQLite protection transaction.

The settings write boundary independently enforces the same rule: a delayed or stale preference write cannot re-persist a tag that no longer exists on any ordinary/public note. Boolean filter state (`pinned`, `with_attachments`, `without_tags`) and still-public tag filters are preserved.

## Existing-library migration contract

The E3 privacy marker advances to `vault.e3_privacy_scrub = 3`. Existing 2a.8 libraries are re-evaluated on initialization. If a legacy filter is changed, SQLite secure-delete/WAL truncation/VACUUM cleanup removes the previous filter bytes rather than leaving them only logically deleted. Libraries without current protected notes do not have unrelated caches or backup repositories purged merely for this preference migration.

## Live-session privacy contract

After a successful Encrypt Note operation, the in-memory note-list tag filter and public tag catalog are refreshed from authoritative storage. Protected-only tag labels are removed immediately. If the catalog refresh fails after encryption has already committed, Rustnote fails closed in the live UI by clearing tag-filter labels and the visible tag catalog rather than retaining protected-only labels.

## Terminology correction

The E2/E3 security documentation identifies the active editor correctly as **ProseMirror**. Historical stale `Wordgard` wording is removed from the current E2/E3 security contract/QA material.

## Inherited security guarantees

All E0–E3 guarantees and later 2a.6–2a.8 hardening remain required, including durable vault transitions, autosave privacy barriers, safe backup ownership, secret zeroization, multiprocess staging safety, atomic Lock revocation, request-scoped protected PDF/media descriptors, private sensitive staging, strong protected-payload validation and patched `webbrowser` dependency handling.

## Definition of done

2a.9 source closure requires:

- protection-transaction tag-filter scrub;
- settings-boundary stale-write defense;
- one-time 2a.8→2a.9 migration with SQLite residue cleanup when needed;
- live UI tag/filter pruning after protection;
- deterministic Rust regression tests covering direct protection and legacy migration;
- E0/E1/E2/E3 plus cumulative closure static validators;
- cumulative architecture/browser/PDF static QA;
- native Cargo/Clippy/test/audit acceptance reported separately if the packaging environment lacks Rust.

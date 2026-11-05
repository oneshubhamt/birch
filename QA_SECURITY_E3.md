# QA — Security E3 managed plaintext-residue closure

Build: **0.12.5 / 2a.12**

## Objective check

- [x] E1/E2 cryptographic format unchanged: AEGIS-256 + Argon2id.
- [x] Full AEGIS-256 32-byte / 256-bit authentication tag remains a hard invariant.
- [x] No SQLCipher, WebCrypto, database replacement, cloud sync or PWA work.
- [x] SQLite protection transition enables secure-delete behavior, rebuilds FTS5 from live rows, truncates WAL, VACUUMs sanitized state and truncates WAL again.
- [x] Existing protected libraries receive a versioned managed-residue cleanup; 2a.10 advances the E3 privacy marker to v4 so 2a.9 libraries receive FTS5 forensic rebuild cleanup.
- [x] Protected plaintext is forbidden from the emergency journal and stale protected-note drafts are discarded.
- [x] Pre-encryption Rustnote-managed automatic backup history is invalidated when protection is introduced.
- [x] Protected previews/media avoid persistent plaintext caches.
- [x] Pre-encryption rendition/media/handoff residues are invalidated.
- [x] Protected Open/Reveal bypasses ordinary handoff copying.
- [x] Vault materializations and sensitive staging paths have normal/startup cleanup.
- [x] Protected PDF reads use request-scoped leased file handles; no plaintext-bearing protected PDF file descriptor survives beyond the authorized request or completed Lock.
- [x] Live editor is disposed when the note editor component unmounts, including re-lock.
- [x] External user-created exports and SSD/COW physical-erasure limitations are explicitly outside the guarantee.
- [x] Privacy deletion is isolated in the explicit `native::privacy` responsibility module; callers do not bypass the A6 native boundary.

## Targeted source QA

- E0/E1/E2/E3 static security contract validation.
- Full-tag invariant scan (`TAG_BYTES == 32` and `Aegis256::<TAG_BYTES>`).
- Direct low-level crypto import/isolation scan.
- SQLite schema check: the current security baseline remains **schema version 17**; 2a.11 introduces no schema migration.
- SQLite migration equivalence remains unchanged from the 2a.6 schema-17 closure (16→17 vs clean 17).
- Managed plaintext residue markers checked across storage protection, emergency journal, backups, image/media caches, native handoff/staging, vault runtime and PDF transport.
- Protected PDF request-scoped descriptor/lease regression markers.
- Rust test added for a unique plaintext marker existing before protection and being absent from SQLite/WAL and Rustnote-managed rendition/media/handoff/backup residues after protection.
- Sensitive staging failure/discard paths use privacy removal rather than ordinary unlink where practical, including failed protected-attachment plaintext restoration staging.
- Editor/ProseMirror source and `rustnote-vault` low-level crypto implementation are not redesigned by E3.

## Security interpretation

Best-effort overwriting is a logical application cleanup aid, **not** a physical secure-erase promise on SSD/APFS/COW storage. FileVault/BitLocker/Android device encryption remains necessary for strong at-rest physical-media protection.

## Environment limitation

The packaging sandbox does not provide `cargo`, `rustc` or `rustfmt`. Static validators therefore run with their Cargo sections disabled. Rust compilation, Clippy and executable Rust tests must be run on the development Mac before treating the build as natively verified.

## Close-out results in packaging environment

- E0, E1, E2 and E3 static security validators: **PASS**.
- Cumulative 2a.11 E0–E3 closure validator: **PASS**.
- A1–A7 and B1–B6 cumulative static architecture validators: **PASS**.
- Dependency-free editor/PDF behavioral suite reached by B6: **36/36 PASS**; B6 PDF subset: **16/16 PASS**.
- All shell scripts: syntax scan **PASS**.
- First-party JavaScript/MJS: syntax scan **PASS**.
- All Rust sources: delimiter/lexical structural scan **PASS**.
- `Cargo.toml`, `Cargo.lock`, `crates/rustnote-vault`, `src/storage/schema.rs`, `editor-src` and `pdf-src`: unchanged from the 2a.10 baseline.
- SQLite schema remains version **17**; 2a.11 adds no migration.
- Native Rust compilation, Clippy, executable Rust tests and `cargo audit` remain pending on the development Mac because this packaging environment has no Rust toolchain.

## 2a.10 behavioral closure QA additions

- [x] Protected attachment placeholders preserve the attachment-table invariant (`embedded = 0` requires non-null `unreferenced_at`).
- [x] FTS5 is rebuilt before final WAL/VACUUM cleanup so deleted protected terms are not retained in obsolete index segments.
- [x] E3 privacy marker advances from v3 to v4; the v4 SQLite scrub runs even when no protected notes currently remain.
- [x] The database-snapshot primitive refuses any source library with unfinished `vault_operations`, covering automatic/safety backups, complete archives and rollback snapshots.
- [x] Snapshot validation rejects an archive database that itself contains unfinished vault operations.
- [x] Direct Rust regressions cover attachment-bearing encryption commit, physical DB/WAL marker cleanup, post-protection backup marker absence, v3→v4 FTS cleanup and pending-vault snapshot refusal.

## 2a.11 adversarial lifecycle QA additions

- Protected payload reads retain their Vault read permit through derivation and fail the publication gate if Lock has begun.
- Lock invalidates loaded list/selection generations and clears selected protected UI state before waiting.
- Sensitive temporary ownership uses an OS-released exclusive file lock rather than PID identity alone; legacy/PID-reuse and pre-owner-lock crash windows have regressions.
- Historical automatic-backup locations are durably remembered per library, cross-library reuse is rejected, and E3 purge covers remembered Rustnote-owned repositories without recursively deleting user-selected parents.
- Native Rust execution remains mandatory for final acceptance.


## 2a.12 security-boundary consolidation QA additions

- [x] Note-scoped derivative transition barrier covers managed plaintext producers and protection transitions.
- [x] Image/PDF staging re-authorizes and rejects protected content.
- [x] Managed derivative cleanup is fail-closed; E3 marker advances to v5.
- [x] Sensitive-temp ownership no longer chmods the system/caller parent; v2 sessions use owner-lock cleanup claims.
- [x] Backup repositories carry durable library ownership identity established by Storage construction.
- [x] Key/password/logging hygiene is tightened without changing cryptographic formats.
- [x] Attachment/media responses use `no-store` to avoid persistent WebView plaintext caching.

# Rustnote Security E3 Contract — Managed Plaintext-Residue Closure

Build: **0.12.5 / 2a.12**

## Objective

E3 closes Rustnote-managed plaintext derivatives around the selective note-encryption system introduced in E2. It does not change the E1 cryptographic design: Rustnote encrypted-format v1 remains **AEGIS-256 with the full 256-bit authentication tag**, and Argon2id continues to protect each random per-note key.

E3 does not replace SQLite, FTS5, ProseMirror, PDF.js, the attachment store, or the operating-system filesystem. It adds privacy cleanup and authorization at the boundaries where decrypted or pre-encryption data can otherwise persist.

## Managed plaintext-residue guarantee

After a note protection transition reports success, Rustnote must not intentionally retain readable plaintext for that protected note in Rustnote-managed:

- canonical SQLite note/FTS/revision/source/PDF-annotation state;
- SQLite WAL frames or ordinary unused/free database pages;
- emergency/crash-recovery journal files;
- automatic/safety backup repository snapshots that predate protection;
- image rendition or media-compatibility caches;
- attachment Open/Reveal handoff copies;
- protected-attachment materialization directories;
- image/PDF staging files left by completed, failed, or crashed Rustnote operations;
- cached PDF file handles that could bypass a later note lock;
- persisted note-list filters containing protected-only literal tag labels.

This is an application-level logical-residue guarantee, not a promise of physical-media destruction.

## SQLite WAL / free-page contract

**Current governing rule (2b.42.8 override).** The E2 protection transaction uses SQLite
`secure_delete` while sensitive rows and persistent FTS entries are removed. After commit, E3:

1. rebuilds `notes_fts` from its current live rows under `secure_delete`, so obsolete FTS5 segment content is rewritten from the sanitized logical state;
2. performs a verified `PRAGMA wal_checkpoint(TRUNCATE)`;
3. treats a busy/incomplete checkpoint or any managed-residue cleanup failure as a protection-finalization failure.

Per-Protect `VACUUM` is **not** part of the current privacy path. Native forensic regression
testing established that `secure_delete + FTS rebuild + verified WAL TRUNCATE` removes the
historical-only and current protected markers covered by Rustnote's managed SQLite/WAL threat
model without the extra `VACUUM` pass. This current rule supersedes the earlier base-E3 and
2a.10 wording that required `VACUUM`.

Development-era one-time E3 migration-marker details in older amendments are retained below as
historical rationale, not as a current backward-compatibility requirement. The current application
baseline is validated against the canonical cleanup behavior above.

## Emergency recovery contract

Protected-note plaintext must never be scheduled or persisted into Rustnote's disk emergency journal.

If startup recovery finds an obsolete valid plaintext draft whose canonical note is now protected, the draft is discarded rather than restored. Rustnote-owned journal files are best-effort overwritten before unlinking.

## Automatic backup contract

An automatic/safety snapshot made before a note was encrypted can contain the old plaintext database and attachment blobs. Rustnote therefore invalidates the **app-managed automatic backup repository** when selective protection changes from plaintext to encrypted state. The repository is recreated empty and subsequent automatic backups are made from the encrypted canonical state.

This policy deliberately favors privacy over retaining Rustnote-managed pre-encryption backup history.

A user-created archive/export that was previously written outside Rustnote's managed directories is outside Rustnote's control and is **not** silently deleted.

## Cache / preview contract

Protected content must not create a persistent plaintext preview cache.

- Protected original image/media responses use no-store caching semantics where applicable.
- Protected image renditions are ephemeral and removed after serving rather than written into the normal rendition cache.
- Persistent media-compatibility caching remains disabled for protected resources.
- Pre-encryption image/media caches for a newly protected note are invalidated and privacy-removed when possible.

## Attachment handoff contract

A decrypted protected attachment must not be copied into Rustnote's ordinary persistent Open/Reveal handoff cache. Native Open/Reveal for such a resource operates on the private vault-runtime temporary path directly.

Any pre-encryption handoff copies belonging to attachment plaintext hashes being protected are invalidated during the protection transition.

## Temporary/staging contract

Decrypted protected attachments live only in a private process-temporary vault directory and are removed on note lock, permanent deletion, normal runtime teardown, and startup cleanup after abnormal termination.

Rustnote-owned image-import/edit and PDF-export staging roots are aggressively cleaned at startup. Disposable staging files are best-effort overwritten before unlinking on discard/failure/completion paths.

## PDF session authorization contract

Efficient PDF range reading may keep a file handle open. Every PDF document read must therefore re-authorize the attachment against current storage/vault state **before** consulting the file-handle cache. If the note has been locked, Rustnote evicts the cached session and refuses the read.

A cached decrypted PDF handle must never act as a capability that survives locking the note.

## Browser/editor memory boundary

When an unlocked protected note's editor view is unmounted, including on re-lock, Rustnote explicitly disposes the live browser editor instance rather than deliberately retaining its document.

Rustnote cannot promise deterministic erasure of every historical copy from allocator memory, WebView/browser internals, swap, crash dumps, or another privileged process. Protecting a device against those threats also requires operating-system security such as FileVault/BitLocker/Android device encryption and protection against malware while the device is unlocked.

## Physical-media limitation

Modern SSD wear levelling, APFS/COW filesystems and filesystem snapshots mean an application cannot honestly guarantee that overwriting then unlinking a file destroys every historical physical block. E3 performs best-effort overwrite for Rustnote-owned disposable plaintext where practical, but **whole-device encryption is the physical-media security boundary**.

This limitation is not a weakness of AEGIS-256: it concerns plaintext that existed before/while the user explicitly had content decrypted.

## Search boundary

E3 does not introduce searchable encrypted content. Protected notes continue to stay out of persistent FTS5 while locked or temporarily unlocked. The later RAM-only search pass remains responsible for making explicitly unlocked protected content searchable without writing its search index to disk.

## Export boundary

E2's containment remains in force: ordinary plaintext/portable export of a protected note is blocked rather than exporting empty placeholder data. Complete Rustnote archives preserve ciphertext losslessly.

An explicit future feature may allow the user to deliberately export decrypted content, but that resulting external file is then outside the vault boundary.

## Cryptographic invariants retained

E3 does not alter `rustnote-vault` or its file format:

- AEGIS-256 remains algorithm id 1;
- key size remains 256 bits;
- nonce size remains 256 bits;
- **authentication tag remains the full 256 bits / 32 bytes**;
- Argon2id remains the password KDF;
- passwords are not direct data-encryption keys;
- protected attachment encryption remains bounded-memory streaming.

Hash-linked revision history and optional device signatures remain later blockchain-inspired integrity work; E3 introduces no blockchain.

## Explicit non-goals / limits

E3 does not:

- encrypt the whole SQLite database;
- add SQLCipher or WebCrypto;
- implement encrypted notebooks;
- add cloud sync or PWA support;
- make protected content globally searchable;
- implement post-quantum device key exchange;
- delete files the user previously exported outside Rustnote;
- claim guaranteed physical secure erase on SSD/COW storage;
- protect plaintext from malware/administrators while the user has explicitly unlocked and is viewing it.

## E3 definition of done

E3 is complete when source and executable QA establish that:

1. the E1/E2 AEGIS-256 + Argon2id model is unchanged, including its full 256-bit tag;
2. protection removes persistent FTS and canonical plaintext as before;
3. SQLite WAL/free-page cleanup is explicit and fail-closed;
4. protected notes cannot be reintroduced through the emergency journal;
5. pre-encryption app-managed automatic backups are invalidated;
6. preview/media caches and handoffs cannot retain protected plaintext by design;
7. vault and staging temporaries are cleaned on normal and startup/crash-recovery paths;
8. locking a protected PDF revokes access even if a PDF read session was previously cached;
9. ordinary unprotected-note behavior and SQLite/FTS/editor architectures remain intact;
10. external-export and physical-media limitations are documented rather than falsely hidden;
11. persisted and live note-list tag filters cannot retain labels that became protected-only.

## 2a.7 final-closure amendment — atomic Lock revocation

The earlier requirement that cached PDF handles not survive Lock is strengthened into one application-wide rule: **Lock is an atomic Rustnote-internal plaintext revocation barrier for the protected note session.**

Protected attachment/PDF/media operations hold a per-note read permit for the complete lifetime in which Rustnote can use decrypted bytes. Lock first publishes revocation, refuses new permits, waits for existing permits to drain, then zeroizes/removes the note key and deletes registered materialized plaintext before it reports success. A late materialization may not register after revocation and must delete its unpublished plaintext.

A cached file descriptor alone is not authorization. Every PDF/media request must possess a fresh transport lease issued from the current unlocked session.

## 2a.7 final-closure amendment — private staging and unprotect semantics

Sensitive staging directory leaves are private by construction (0700 on Unix) and sensitive files are created private (0600 on Unix). Vault, image and PDF plaintext staging use the shared native privacy abstraction.

The user's successful authorization of **Remove encryption** is the explicit request to release confidentiality, but Rustnote still keeps pre-commit reconstructed plaintext in a private operation-owned staging tree. Before database commit it is unpublished; crash recovery destroys it. After database commit recovery may publish it to ordinary canonical storage.

## 2a.7 final-closure amendment — metadata and external egress

Vault v1 does not claim existence-, timing- or ciphertext-size hiding. Structural state and approximate content size can remain observable. Plaintext deliberately handed to another application or user-selected external destination is outside Rustnote's revocable internal vault boundary; Lock cannot recall such external copies.


## 2a.8 final-closure amendment — plaintext descriptor lifetime

A completed Lock must not leave a Rustnote-owned plaintext-bearing OS descriptor from the revoked protected-note session. PDF read-session caching therefore stores metadata only; each PDF data request opens a fresh request-scoped handle while its `ManagedAttachment` retains the current vault permit. Media transport binds its opened `File` and `VaultReadPermit` into one leased object whose lifetime covers the actual stream.

Converter-created sensitive rendition output is validated as a regular non-symlink file and normalized to Unix mode `0600` before Rustnote publishes or serves it. The enclosing protected staging directory remains `0700`.


## Persisted tag-filter contract — 2a.9 final-closure amendment

Rustnote's note-list filter is a Rustnote-managed SQLite preference and may contain literal tag names. When a note becomes protected, tag names that no longer exist in the public tag catalog must be removed from that persisted preference in the same protection transaction. The settings write boundary must independently enforce the same invariant so a delayed or stale UI preference save cannot recreate a protected-only tag name. The live note-list tag catalog and tag filters are also pruned immediately after successful protection so Lock does not leave protected-only tag labels visible in the current session.


## 2a.10 behavioral final-closure amendment — FTS forensic scrub and snapshot barrier

Deleting a protected note from `notes_fts` is a logical requirement but is not, by itself, a forensic-erasure guarantee for FTS5 segment history. The 2a.10 pass introduced a full `notes_fts` rebuild plus WAL cleanup and, at that time, a `VACUUM` pass and one-time migration marker. **The later 2b.42.8 override above is now governing:** the FTS rebuild and verified WAL truncation remain mandatory, while per-Protect `VACUUM` and development-era migration-marker compatibility are historical implementation steps rather than current requirements.

Protected attachment placeholders must remain valid ordinary-schema rows while their sensitive metadata lives only inside the encrypted payload. Because the attachment schema requires every non-embedded attachment to have `unreferenced_at IS NOT NULL`, protection stores the transition timestamp rather than violating that invariant. Protected-note pruning continues to exclude these placeholders.

Every persistent database snapshot crosses one canonical fail-closed boundary. Automatic backups, safety snapshots, complete archives and restore rollback snapshots are refused while `vault_operations` contains an unfinished protect/unprotect transition. Snapshot validation likewise rejects an archive database that itself contains unfinished vault operations. This prevents a post-commit/pre-cleanup failure state from being preserved into another Rustnote-managed durable artifact.

Behavioral acceptance includes executable regressions proving: attachment-bearing protection commits under the real SQLite CHECK constraint; protected FTS marker bytes are absent from SQLite/WAL after cleanup and from a subsequent automatic-backup database; a v3/2a.9 library is advanced through the v4 FTS scrub; and automatic backup/complete archive creation fail closed while a vault operation is pending.

## 2a.11 adversarial lifecycle amendment — owned reads, temp ownership and backup history

Protected note payload reads now keep their `VaultReadPermit` through payload validation and derivation, then pass a final same-generation/non-revoking publication gate. The live application invalidates note-list and note-selection generations, redacts the protected note's cached summary and unmounts a selected protected note before Lock waits, so already-dispatched asynchronous reads cannot re-enter live UI state after revocation. This is an internal publication/lifecycle guarantee, not a claim that arbitrary allocator or WebView copies already created while legitimately unlocked can be deterministically recalled.

Sensitive vault/image/PDF/handoff sessions no longer treat PID liveness as ownership proof. New session directories publish a private owner file held under an OS exclusive file lock; hard process death releases the lock automatically. Cleanup retains live locked sessions, removes abandoned owner sessions, remains compatible with legacy PID-only directories, and treats an incomplete `session-<pid>-<token>` created before owner-lock publication as recoverable stale state.

Automatic-backup privacy ownership is durable across location changes. Rustnote records a bounded history keyed by canonical library directory before replacing location configuration. One remembered backup parent cannot silently transfer to a different library identity. E3 invalidates all remembered Rustnote-owned repositories for the current library under backup-directory ownership locks, preserves unrelated parent/sibling files, and recreates only the currently configured repository.


## 2a.12 security-boundary consolidation amendment

The managed plaintext-residue guarantee now includes a note-scoped plaintext-derivative transition barrier covering persistent preview/cache generation, media compatibility work, image/PDF staging and attachment handoff materialization. Protect/unprotect transitions drain already-authorized producers, block new producers, hold the barrier through canonical persistence and cleanup, and require later work to re-authorize current protection state.

Image-rendition/media-compatibility/staging/handoff cleanup is fail-closed for protection finalization. The one-time E3 marker advances to `vault.e3_privacy_scrub = 5` so libraries that completed earlier E3 migrations receive the stronger managed-derivative cleanup. Attachment/media protocol responses use `no-store` rather than asking the WebView to persist plaintext bytes.

Sensitive temporary ownership no longer modifies an arbitrary system parent. V2 temp sessions use OS-lock ownership plus cleanup claims, and new-format markerless sessions are abandoned rather than trusted by PID. Backup repositories are bound to a durable library identity and the storage constructor establishes that ownership/history invariant. Key/password/logging hygiene is tightened without changing the AEGIS-256/Argon2id formats or E0/E1 cryptographic policy.

## 2a.13 adversarial behavioral closure amendment

The 2a.12 security boundaries are now exercised as one bounded adversarial state-transition matrix rather than extended with another architecture. Behavioral acceptance explicitly covers Encrypt versus active plaintext derivative work, Lock versus scoped protected reads, all four canonical durable protect/unprotect journal states, post-commit managed-filesystem cleanup failure and restart retry, stale asynchronous publication barriers, backup-location reuse/cross-library ownership, repeated protect/unprotect cycles, pending attachment ingest/removal races, SQLite/WAL/FTS/revision/autosave residue, and v2 sensitive-temp abnormal termination.

A protection transition that has committed canonical ciphertext but fails strict managed-residue cleanup remains a failed operation with its durable `database_committed` journal row intact. Startup recovery completes that privacy finalization before normal integrity checks. The four journal states remain the complete canonical crash model; 2a.13 does not add artificial transition phases for crashes that occur before durable preparation or after durable completion.

This amendment is a behavioral closure pass only. It preserves the 2a.12 derivative gate, AEGIS-256/Argon2id formats, storage architecture, backup ownership model, E3-v5 residue marker and documented external/physical-media limits.

## 2a.14 independent final closure audit amendment

The independent 2a.14 audit treats prior validators as regression aids rather than proof and re-traces the original E0–E3 plaintext-equivalent surfaces directly in source.

Three concrete E3 lifetime/residue defects were found and closed without adding a new Vault architecture:

1. PDF annotation-pages and annotation-index protocol responses must retain the existing `ManagedAttachment` guard for the full response-construction scope. For a protected PDF that guard owns the Vault read lease, so Lock cannot complete while annotation plaintext is still being read or serialized by the native protocol.
2. Startup recovery of a valid plaintext emergency-journal record for an already protected note is fail-closed. Failure to remove that known plaintext record is an error, not a warning followed by normal startup.
3. Emergency-journal quarantine is part of the managed plaintext boundary. Note-specific Vault autosave clearing removes quarantined records whose deterministic filename belongs to that note, and startup privacy recovery removes matching legacy quarantine records for already protected notes. Quarantine cleanup is target-scoped; ordinary-note quarantine is not globally erased.

These changes refine existing resource-lease and autosave privacy barriers. They do not introduce a new encryption, cache, staging, backup, or transition architecture.

## 2a.15 plaintext ownership remediation amendment

The 2a.15 remediation tightens three fail-closed invariants discovered by a de-novo audit of 2a.14:

1. **Lock cleanup ownership:** revocation no longer removes the note session/key or plaintext tracking entries before filesystem cleanup succeeds. Failed deletion leaves the session revoking and every cleanup obligation registered so a later Lock retry must finish cleanup before success.
2. **Protected attachment materialization ownership:** the destination path is registered with `VaultRuntime` before file creation or decryption. Successful authentication/verification atomically promotes that tracked path into the materialized-attachment registry; failure remains tracked until deletion succeeds.
3. **Emergency-journal pre-quarantine privacy:** startup enumerates protected note IDs and removes their deterministic final/tmp/previous/quarantine records before parsing recovery candidates, then repeats the protected sweep after recovery as defence in depth. Existing non-file nodes at journal record paths are cleanup failures, not successful absence.

These are regression requirements for E3. They do not by themselves constitute a new independent full E0-E3 closure certification.

## 2a.16 canonical plaintext ownership amendment

The 2a.16 closure pass generalizes plaintext lifetime from local cleanup convention into an ownership invariant:

> No Rustnote-managed plaintext file may be created, copied, renamed, or materialized unless a cleanup authority already owns that exact lifetime, and no owner may be forgotten before deletion succeeds or ownership transfers atomically to another authoritative owner.

Two ownership domains are intentionally retained rather than forced into one abstraction:

- process-private image/PDF derivative staging is owned by the in-memory derivative registry inside an app-private crash-recoverable session root; ownership is reserved before file creation and a rename updates the registered path while the registry lock is held;
- library-resident staging is owned durably by SQLite before the first plaintext byte is written. `managed_plaintext_staging` covers attachment `.incoming` files, staged Rustnote import archives, and newly materialized bundle blobs until a more specific durable ingest/operation/canonical database owner claims them.

Generic ownership transfer must occur in the same SQLite transaction that creates the specialized durable owner or canonical attachment/revision reference. The reverse transition is equally strict: when deletion/undo removes the final canonical reference to plaintext, a durable cleanup owner must exist in that same transaction before the reference can disappear. A failed filesystem deletion keeps the owning row/registry entry intact. Startup removes abandoned generic library staging before normal recovery and fails closed if cleanup cannot be proven. Protect, backup snapshots, permanent delete, Trash purge, portability rollback/undo, attachment recovery, and untracked-`.incoming` reconciliation must respect this ownership boundary. Note deletion boundaries must enter the note-scoped plaintext-derivative transition before the storage operation lock, drain existing producers, and revalidate deletion eligibility before owner-backed cleanup.

Temporary files that remain outside this ledger are acceptable only where an equivalent stronger owner exists: a Vault plaintext-path owner, a note-scoped derivative transition plus a deterministically scrubbed cache namespace, a vault-operation journal, an app-private temp-session owner, or an explicit user-selected external output boundary.

Complete-archive and backup-repository restore/rollback staging is specifically an app-private temp-session case: the OS-lock-backed process owner must exist before any restore child is created, abandoned sessions must be removed fail-closed before startup, and security-sensitive attachment finalization must not manufacture an unowned `.previous` sibling while replacing a canonical blob.

## 2a.17 persistent residue and backup closure amendment

The managed plaintext-residue guarantee extends beyond active temporary files to Rustnote-owned historical repositories and attachment quarantine.

**Durable backup privacy obligations.** Before a Vault protection scrub invalidates backup repositories, Rustnote durably marks every remembered repository for that library as requiring privacy reset. An unavailable/removable repository does not block local encryption, but its obligation remains durable. A privacy-pending location may not be recreated at a missing mount path, treated as clean because an empty substitute parent exists, or used for normal repository reconciliation, snapshot creation, pruning, inspection or restore. On a later open, Rustnote must positively prove the remembered Rustnote repository is present, acquire the backup-directory ownership lock, purge it, recreate only minimal owned clean-state repository metadata, and only then clear the obligation. Settings expose the number of unresolved repositories without storing note plaintext in that status.

A configured backup repository that disappears after the library was opened follows the same rule: protection completes locally, the obligation remains pending, and the existing Storage instance refuses normal backup operations until the repository is reconnected and Rustnote is reopened under a fresh directory lock. Successful cleanup of historical repositories leaves only minimal Rustnote ownership/index metadata so future removable-volume identity can be proven without retaining old snapshots.

**Attachment orphan quarantine.** For each plaintext attachment content identity included in a newly protected note, protection strictly removes every matching Rustnote-owned `attachments/.orphaned` artifact whose deterministic quarantine filename ends in that SHA-256. Unrelated quarantine history is preserved. Failure to remove a matching artifact fails protection finalization. Because pre-2a.17 protected payloads do not leave their former plaintext hashes in ordinary durable metadata, the one-time E3 migration conservatively clears the Rustnote-owned orphan quarantine before advancing the local privacy marker to `vault.e3_privacy_scrub = 6`.

**Portability/import residue.** 2a.17 does not add a competing import cleanup mechanism. The 2a.16 canonical plaintext-ownership ledger remains authoritative for staged `.portability-import-*.rustnote` archives and newly extracted bundle blobs: ownership precedes first plaintext write, canonical reference transfer is transactional, and failed cleanup retains the durable owner.

## 2a.18 in-memory/UI privacy lifetime amendment

E3's managed plaintext lifetime includes Rustnote-owned process-memory/UI derivatives of explicitly unlocked protected content. A single UI privacy epoch is advanced before every Vault visibility transition (Encrypt, Unlock, Lock, Remove Encryption). Async UI work that can publish decrypted or formerly-public data across that boundary captures the epoch and must reject stale publication; render/action paths must additionally suppress cached data whose publication epoch is no longer current. Existing list/selection generations remain independent complementary guards.

Cross-note caches such as Backlinks, hierarchy/properties, note pickers/search candidates, sources/citations, note-link registries and revision previews must participate in this revocation boundary. Lock/protection may not be followed by a stale title, metadata result, action target or revision preview that was dispatched under an older privacy epoch.

A protected resource whose decrypted data remains in active asynchronous use must retain its Vault authorization for that complete sensitive lifetime. Protected attachment file commands retain both the note plaintext-derivative permit and Vault read permit from metadata lookup through materialization, native Open/Reveal work and the complete Save-As dialog lifetime. Protected revision preview uses a permit-bearing snapshot resource: the Vault read permit remains alive through editor-projection/Markdown/HTML derivation and the final privacy-epoch publish/discard decision, and is released immediately afterward rather than being retained in the cache.

Errors that can outlive a protected resource permit must not embed decrypted protected filenames or other sensitive metadata. Lock first revokes future Vault reads and then waits for all permit-bearing operations to end, so it cannot report success while one of these Rustnote-owned sensitive asynchronous lifetimes is still active.

## 2a.19 independent adversarial closure amendment

The 2a.19 pass re-derives E0–E3 from the original contracts and treats all earlier remediation validators as regression aids rather than proof. It adds no new cryptographic format or general persistence architecture. The following rules are closure requirements discovered by adversarial source tracing.

**Exclusive Vault revocation transitions.** A note-level Lock, Remove-encryption, permanent-delete, portability rollback or equivalent destructive privacy mutation must publish Vault revocation and drain existing read permits before acquiring Storage's operation lock. The returned revocation transition remains alive through the serialized database mutation, blocking a concurrent Unlock in the drain-to-mutation gap. Conflicting revocation transitions for the same note are mutually exclusive, and an overlap is rejected fail-closed rather than queued. Whole-library revocation is likewise mutually exclusive with every note-level revocation and rejects overlap. This prevents Lock from reporting success and then allowing an already-started conflicting privacy mutation to wake and republish plaintext, and avoids waiting on a permit-bearing operation while holding the storage lock that operation may still need.

**Whole-library producer revocation.** Complete-archive and backup-repository restore must freeze the library-wide plaintext-derivative gate before Vault revocation and before the final database-replacement lock. New attachment/image/import producers are blocked and existing producers drain, so a library-resident plaintext staging owner cannot be committed only to the outgoing database and then lost at the SQLite ownership handoff. Before canonical library replacement, the UI privacy epoch is revoked and the live editor/autosave library state is discarded; Storage independently drains every old Vault session and removes its tracked decrypted materializations.

**Restore ownership handoff.** Whole-library restore must not use a generic `.copy.tmp` / `.previous` replacement sequence for canonical attachment plaintext. An incoming-only canonical path receives durable generic cleanup ownership before it can exist. Replacement bytes are copied into a same-directory temporary path whose durable owner is reserved before the first byte, verified there, then atomically renamed into an already-owned canonical path. The incoming database is seeded before the SQLite snapshot switch with cleanup ownership for old-library-only canonical blobs. The SQLite backup transaction is the ownership handoff: before completion the outgoing database or its generic staging rows own every touched plaintext path; after completion the restored canonical references and seeded cleanup rows do. Rollback may restore the outgoing database only after every incoming-only canonical plaintext path that the outgoing database would not own has been retired successfully.

**Protected revision publication lifetime.** A protected revision restore returns an opaque authorization retaining its Vault read permit. That authorization must survive the storage mutation, editor replacement/remount, projection/normalization work and the final UI publish/discard decision. The UI privacy epoch is checked after asynchronous boundaries and immediately before publication. Lock therefore either waits for the complete sensitive post-storage lifetime or causes the stale result to be discarded; it cannot complete and then be followed by publication of decrypted revision state from the revoked session.

**Independent writer/residue sweep.** E3 closure again requires direct inspection of SQLite constructors and filesystem writers rather than relying only on named tests. Production SQLite connection authority remains inside `src/storage`. Security-sensitive library plaintext unlink paths may not ignore deletion failures unless an independent durable/process owner remains authoritative and startup recovery is fail-closed. The inherited 2a.15–2a.18 requirements for attachment/image/import staging, orphan quarantine, offline backup obligations, emergency journal, WAL/FTS/revision residue, stale UI caches, attachment dialog lifetime and protected attachment materialization remain mandatory.

## Q6 independent closure amendment — backup snapshot/restore quarantine ownership

The independent Q6 writer/ownership audit found that ordinary attachment removal can leave a valid, durably owned `attachments/.orphaned` quarantine file while the current library retains that cleanup owner. Quarantine is deliberately excluded from managed backup/archive payloads, so its ownership has two additional whole-library invariants:

1. A database snapshot copy may omit only **settled quarantine ownership rows** that are positively identified as the generic library-blob owner, have no note scope, have no prepared transfer state/destination, and name a valid `.orphaned` path. Incoming attachment staging, portability/import staging, canonical cleanup ownership, prepared transfers and ambiguous rows remain snapshot blockers.
2. Before a whole-library restore replaces the canonical SQLite database, Rustnote must strictly retire settled old-library quarantine while the outgoing database still owns those files. Failure aborts Restore before the database ownership handoff. The incoming database must never become canonical while an excluded old-library quarantine file survives without its durable owner.

Executable acceptance requires both directions: a repository snapshot containing an attachment must restore the exact note/attachment bytes and reopen cleanly after the live copy removed that attachment; and an active managed plaintext staging owner must continue to make persistent snapshot creation fail closed.


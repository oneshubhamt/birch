# Rustnote 0.12.5 build 2a.14 — independent E0–E3 final closure audit

This pass is an independent source audit, not an extension of the Vault architecture. The 2a.13 validators are treated as untrusted evidence. E0–E3 are reconstructed from the original contracts and each plaintext-equivalent surface is traced to its authoritative implementation.

## Audit conclusion

The direct-source audit found three concrete E3 defects and fixed them narrowly using existing abstractions:

- PDF annotation-pages/index responses dropped their protected attachment/Vault lease before reading and serializing annotation plaintext. They now retain `ManagedAttachment` through response construction.
- Startup recovery treated failure to delete a valid stale plaintext emergency draft for a protected note as a warning. It now fails closed.
- Emergency-journal quarantine could retain a corrupt plaintext draft from the note's ordinary lifetime after later encryption. The existing note-specific autosave privacy barrier now removes matching quarantine records, and startup privacy recovery removes legacy matching quarantine records for already protected notes.

No new Vault architecture was added.

## Independent E0–E3 evidence matrix

| Original surface/property | Direct-source evidence and result |
| --- | --- |
| E0 threat boundary | Contracts enumerate database/WAL, attachments, backups, journal, indexes, revisions, caches/previews/staging and runtime plaintext. Application-level logical residue is distinguished from physical-media secure erasure. |
| Cipher/key format | `rustnote-vault`: AEGIS-256 with 32-byte key, nonce and full 32-byte tag; versioned authenticated object/envelope/blob formats. |
| Password KDF | Argon2id v1 envelope with bounded accepted parameters; password-derived and random scope keys use zeroizing buffers. Native representative-hardware acceptance remains an external empirical gate. |
| Context/authentication | Vault domain/purpose/object identity and format header are authenticated; wrong-object/context substitution is rejected. |
| Large attachments | Protected blobs use authenticated chunk streaming and unpublished temporary output before atomic publication. |
| Canonical note fields | Protect captures logical note then sanitizes title/title-style/body/editor-state/plain-text in ordinary rows. |
| Tags/filter state | Note-tag links are removed, orphan tags pruned, protected-only persisted filter labels scrubbed. |
| FTS5 | Protected row removed; E3 rebuilds FTS5 before WAL truncation/VACUUM so deleted virtual-table content is not merely logically hidden. |
| Revisions/history | Revision snapshots and attachment refs move into the authenticated protected payload and ordinary revision rows are removed. |
| Sources/citations/links | Note source links, citations and outgoing links move into protected payload; ordinary note-owned rows are removed/pruned. |
| PDF annotations | Annotation JSON moves into protected payload and ordinary rows are removed. Protected protocol reads retain a Vault resource guard through response construction; protected annotation mutation is rejected. |
| Attachment metadata | Protected attachment rows retain structural identity but replace filename/metadata/plaintext hash with locked/ciphertext state; full logical metadata lives in authenticated payload. |
| Attachment bytes | Shared ordinary blob is never encrypted in place. Protected ciphertext is separate; plaintext is retired only when unreferenced. |
| Attachment ingest/mutation | Durable ingest/attachment-operation rows block protection until settled; storage operation lock serializes mutation/protection publication. |
| Image/media caches | Note-scoped derivative transition drains producers before Encrypt; worker owns its permit; strict targeted cleanup is fail-closed. Protected resources do not enter persistent media caches. |
| WebView previews | Attachment/media/PDF data responses use `no-store`; protected preview/runtime resources retain Vault leases and are registered for revocation cleanup. |
| Image/PDF staging | Note/attachment-scoped derivative permits re-authorize after transition; protected edit/annotated-export staging is rejected; staged identities permit strict targeted cleanup. |
| Native handoff | Ordinary/pending handoffs have stable content/note ownership for Encrypt cleanup. Protected runtime files are opened from Vault runtime rather than copied into persistent handoff cache. |
| Emergency journal | Vault application barrier synchronously blocks future drafts and removes active note records before protection. 2a.14 also fails closed on protected stale-record deletion and removes target-scoped quarantine residue. |
| Automatic backups | Every snapshot is serialized by the storage operation lock and rejects unfinished `vault_operations`; backup repositories have durable library ownership and historical location tracking. |
| Portable/export paths | Plaintext Markdown/portable scopes reject protected notes; complete archives serialize with storage operations and preserve protected canonical state. Explicit user-selected external exports are outside Rustnote-managed residue after handoff. |
| SQLite/WAL | Protect uses immediate transaction, secure-delete/full-synchronous at privacy boundary, FTS rebuild, WAL TRUNCATE verification and VACUUM. |
| Crash recovery | Durable protect/unprotect journal has prepared/database-committed recovery for both directions; startup completes or rolls back before normal integrity checks. |
| Lock/runtime revocation | Per-note revocation blocks new read permits and waits active leased resources. UI generation invalidation/unmount prevents stale Dioxus publication; native protected protocol resources retain their permits for plaintext-use lifetime. |
| Temporary ownership | Sensitive v2 sessions use OS file-lock ownership and cleanup claims; markerless abandoned v2 sessions do not trust PID reuse. |
| Protect → unprotect → protect | Unprotect restores canonical rows/attachments/history/sources/annotations and refreshes FTS; repeated-cycle regressions cover restoration and re-scrub. |

## Scope limits that are not closure defects

- E3 is an application-level logical-residue guarantee. SSD wear levelling, COW snapshots, filesystem snapshots and inaccessible/unmounted historical media are not represented as physical secure erasure guarantees.
- User-directed external Save As/Open/reveal destinations are outside Rustnote-managed residue after the user deliberately transfers plaintext out of the Vault boundary.
- Native Rust compilation/tests and the E0 representative-hardware benchmark are not executed in this environment by prior user instruction; they remain external acceptance evidence.

## Final source-level verdict

After the three 2a.14 corrections, the independent source trace found no remaining Rustnote-managed plaintext-equivalent surface in the original E0–E3 scope that lacks a protection, revocation, serialization or fail-closed cleanup path. This is a source-level closure statement, subject to the native acceptance gates above.

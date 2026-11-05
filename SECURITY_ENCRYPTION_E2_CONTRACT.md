# Rustnote Security E2 Contract — Selective Note Encryption

Build: **0.12.5 / 2a.3**

## Objective

E2 makes one ordinary Rustnote note selectively encryptable without replacing SQLite, FTS5, ProseMirror, PDF.js, or the ordinary attachment architecture. The E1 `rustnote-vault` crate remains the only low-level cryptographic implementation. AEGIS-256 is the encrypted-format v1 cipher and always uses its full **256-bit authentication tag**; Argon2id protects the random per-note key from a human password.

E2 is intentionally conservative: an encrypted note may be unlocked for viewing, but it remains read-only until encryption is removed. Editing encrypted content is deferred until E3 closes broader plaintext-derivative paths.

## Canonical protected-note model

A protected note has one random 256-bit note key. At rest that key exists only in an Argon2id password envelope. The authenticated AEGIS note payload contains the sensitive logical state:

- note title, title style, document/editor state and plain-text projection;
- tags;
- note revision snapshots and their historical attachment references;
- sources and citations owned by the note;
- PDF annotation JSON for note-owned attachments;
- outgoing note links;
- original attachment metadata;
- plaintext-to-ciphertext protected-blob mappings.

The password never directly encrypts note content.

## SQLite boundary while locked

SQLite remains the database engine. While a note is protected, ordinary tables retain only routing/state information needed for Rustnote to manage the item, including note identity, notebook placement, pin/archive/trash state, timestamps, attachment identity/routing, and opaque ciphertext accounting.

Sensitive plaintext columns are blanked or removed. In particular:

- note title/body/editor/plain-text columns are sanitized;
- the note has no persistent `notes_fts` row;
- note tags are removed from ordinary tag membership;
- plaintext note revision rows are removed after being captured inside the protected payload;
- note-owned sources/citations are removed after capture;
- note-owned PDF annotations are removed after capture;
- outgoing note-link rows are removed after capture;
- protected attachment placeholder rows do not preserve original name, MIME/image metadata, edit lineage, embedded state, or original attachment timestamps.

Structural relationships that are necessary to locate/manage the encrypted item are not claimed to be hidden in E2.

## Attachment contract

Normal notes continue to use Rustnote's existing content-addressed SHA-256 blob store unchanged.

For a protected note, every current or revision-only attachment blob referenced by that note is streamed through the E1 AEGIS protected-blob format. Ciphertext is stored as a distinct content-addressed blob. Rustnote never encrypts a normal shared plaintext blob in place.

If a plaintext blob is exclusively referenced by the note being protected, it is atomically moved out of the canonical blob path before the SQLite protection commit and deleted after commit. If the database transition fails, the plaintext blob is restored and unreferenced ciphertext is cleaned up. If another unprotected note legitimately references the same plaintext blob, that plaintext must remain for the other note.

`protected_note_blobs` provides an opaque retention/reference index for every protected ciphertext blob, including revision-only blobs, so backup, orphan cleanup, hard deletion and integrity accounting cannot lose them.

Large attachments remain bounded-memory streaming operations.

## Unlock / lock contract

Unlocking verifies the Argon2id key envelope and authenticates the complete encrypted note payload before retaining the random note key in process memory. The key is never persisted in SQLite.

A locked note exposes a dedicated locked-note view. An unlocked protected note may be read, including its encrypted revisions, sources/citations, attachments and PDF annotations, but it remains read-only in E2.

Locking discards the in-memory note key and removes process-temporary decrypted attachment material. Permanent note deletion does the same before deleting storage rows.

## Search contract

Protected content is absent from persistent FTS5 whether locked or temporarily unlocked. This prevents decrypted private content from being written back to the on-disk search index.

Consequently, E2 protected notes do not participate in global full-text search. E4 will add RAM-only FTS for explicitly unlocked protected content.

## Export / backup contract

Ordinary plain-text, Markdown, portable and similar content exports must not silently export a protected note as blank/placeholder content. E2 blocks those scopes while protected.

The complete Rustnote archive remains lossless: it backs up the encrypted SQLite state and every referenced protected ciphertext blob without decrypting the note.

Richer explicit encrypted/decrypted export behavior is deferred to E3.

## Plaintext-derivative boundary and E3

E2 closes the live canonical database/FTS/revision/source/PDF-annotation and protected-blob paths required to make selective note encryption real. It does **not** claim forensic erasure of plaintext that may predate encryption.

E3 remains mandatory for a comprehensive audit of:

- SQLite WAL, rollback/free pages and vacuum/secure-delete behavior;
- emergency/crash-recovery history beyond the transition cleanup performed in E2;
- historical automatic backups and user-created exports;
- preview/rendition/media-compatibility caches;
- OS handoff/temp files and abnormal-crash residue;
- any other derived plaintext caches or indexes.

## Cryptographic boundary

Application/UI/storage code must use Rustnote's `src/security` facade. Direct imports of `aegis`, `argon2`, or other low-level primitive crates outside `crates/rustnote-vault` are forbidden.

No SQLCipher, WebCrypto, custom cipher, blockchain, cloud-sync crypto, or post-quantum key exchange is introduced in E2.

## Integrity / future blockchain-inspired work

AEGIS authentication already detects ciphertext modification for protected objects/blobs. Hash-linked revision history and optional device signatures remain future integrity features; E2 does not introduce an actual blockchain.

## E2 definition of done

E2 is complete when source and executable QA establish that:

1. one note can be encrypted, locked, unlocked and returned losslessly to ordinary form;
2. wrong passwords and tampered vault data fail closed;
3. sensitive current-note data is absent from ordinary SQLite/FTS while protected;
4. current and historical attachment bytes are protected without corrupting shared ordinary blobs;
5. protected ciphertext is retained by backup/orphan/deletion accounting;
6. encrypted-note mutation paths are blocked in E2;
7. protected revisions/sources/PDF annotations remain readable only after unlock;
8. unsupported plaintext exports are blocked;
9. normal unprotected-note behavior is unchanged;
10. E1 cryptographic implementation remains isolated in `rustnote-vault`.

## 2a.18 protected revision restore amendment

Protected revision history is not merely retained for preview: an explicitly unlocked protected note may restore one of its authenticated historical revisions without reintroducing the revision into ordinary plaintext persistence.

The restore path must authenticate/decrypt the protected payload under the current Vault session, materialize the selected encrypted revision in memory, preserve the note's current structural routing/state, validate that every historical attachment still has a consistent protected-blob mapping, checkpoint the pre-restore state, replace the protected logical note/attachment/link state, checkpoint the restored state, and reseal the canonical protected payload. SQLite may retain only the protected note stub and locked attachment routing placeholders. The restore must not repopulate plaintext `note_revisions`, `note_tags`, `notes_fts`, note body/title/plain-text columns, source/citation rows, or original protected attachment metadata.

Any browser-runtime document normalization after a protected restore must use the same Vault-aware payload mutation path; it may not fall back to ordinary autosave/note persistence. Ordinary-note revision restore remains unchanged.

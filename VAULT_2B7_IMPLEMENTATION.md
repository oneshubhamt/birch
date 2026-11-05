# Rustnote Vault 2b.7 — Closure Repair Implementation

Date: 2026-09-01

## Scope

Build 2b.7 is a closure repair over 2b.6. It adds no new Vault architecture. It corrects the places where production callers bypassed or re-entered the authorities already established by 2b.1–2b.6.

## 1. Privacy-transition re-entry removed

### Protect

`protect_note_with_password()` no longer calls public `get_note()` while its Protect mutation guard remains alive. After the protection transaction and finalization, it reads the public timestamp through the direct SQLite stub reader and returns the already-authorized decrypted note state.

### Lock

`lock_protected_note()` no longer calls public `get_note()` while Lock admission is active. It reads the public stub directly and marks the returned state Locked.

### Delete / purge / rollback

Transition-owned plaintext retirement now obtains stable attachment identities directly from SQLite instead of calling the admission-aware public `list_attachments()` protected reader.

This preserves one rule: public readers acquire authorization; code already inside a privacy mutation uses transition-safe internal data access.

## 2. Protected revision Restore now fails closed

`PrivacyCoordinator::begin_protected_note_restore()` now performs under one admission mutex:

1. reject active library or same-note transition;
2. capture the current Vault note access;
3. insert Restore admission;
4. advance privacy generation;
5. drain publication leases;
6. begin derivative transition;
7. acquire canonical storage mutation authority.

Restore no longer calls `wait_for_note_admission_clear()` and cannot silently queue behind Lock/Protect/Delete/Unprotect/Unlock.

## 3. Protected link read-side cutover completed

Protected sources now read outgoing note IDs from encrypted `MetadataState` through `protected_outgoing_note_links_if_unlocked()`.

Behavior:

- ordinary sources continue using plaintext `note_links`;
- protected locked sources expose no outgoing relationship set;
- protected unlocked sources return targets from encrypted metadata;
- backlinks merge ordinary plaintext-source rows with currently unlocked protected sources;
- ordinary-source links to a protected target resolve the target's private title only while the target is unlocked;
- locked target identity/state remains resolvable but title is `None`.

No protected relationship rows are restored to plaintext SQLite merely for UI convenience.

## 4. One transaction-boundary ownership handoff for ordinary blobs

`ensure_unreferenced_canonical_blob_plaintext_stage_in_transaction()` is the centralized database→cleanup-ledger handoff. It:

- validates the canonical blob identity;
- checks `attachments`, `revision_attachment_refs` and `vault_blob_refs` inside the caller's transaction;
- treats a pending Add operation as an existing authoritative owner;
- parses Add operation JSON fail-closed;
- creates/reuses a generic canonical durable owner only after all persistent references are gone.

The helper intentionally returns `StorageResult<()>`; callers need the invariant, not the stage ID.

It is used by every ordinary row-removal path found in the final writer sweep, including revision-only historical attachment references.

## 5. Typed quarantine ownership and transfer

Canonical ordinary plaintext is no longer moved into `.orphaned` through an unowned raw rename.

The quarantine path now:

1. requires a pre-existing canonical durable owner;
2. reserves a durable `.orphaned` successor owner;
3. writes a prepared predecessor→successor transfer record;
4. renames using `OwnedPlaintext::rename_durable_to_preowned()`;
5. syncs the parent directory;
6. proves the successor is owned;
7. retires predecessor ownership only after the source is conclusively absent.

Deletion similarly unlinks first and retires ownership afterward. An unlink failure retains the cleanup obligation.

Normal steady-state cleanup cannot manufacture ownership after discovering a file. Startup verifies that pre-existing canonical/quarantine files already have a legitimate database, Vault, pending-Add or durable cleanup owner.

## 6. Pending Add ownership is exclusive and corrupt ownership fails closed

A pending attachment Add already owns the staging predecessor and potential canonical successor. The generic ownership handoff therefore does not create a second cleanup owner for the same SHA-256.

If an Add operation's serialized `Attachment` cannot be decoded, ownership decisions return an integrity error. Corruption is not interpreted as permission to delete or quarantine the blob.

## 7. Revision and destructive-operation ownership closure

Revision removal was independently re-audited because `revision_attachment_refs` may be the final durable owner of an ordinary canonical blob.

The implementation now captures candidate SHA-256 identities before cascade/delete operations and establishes cleanup ownership before commit for:

- ordinary revision restore;
- per-note retention pruning;
- global age/count pruning;
- storage-budget pruning;
- hard deletion;
- expired-Trash batch purge;
- portability rollback.

Protected notes fail closed if impossible ordinary revision-attachment residue is discovered during destructive operations.

## 8. Protected ciphertext deletion is separate from plaintext quarantine

Hard-deleted protected attachments are ciphertext, so they do not enter `.orphaned` plaintext retention.

Single-note hard delete and expired-Trash batch purge now:

1. capture protected ciphertext hashes while `vault_blob_refs` still exist;
2. delete the note/ref rows inside the SQLite transaction;
3. establish a durable canonical cleanup obligation for any newly-unreferenced canonical blob before commit;
4. commit;
5. call `discard_canonical_blob_if_unreferenced_strict()` to delete ciphertext directly.

A restart after step 4 but before step 5 is safe: the committed cleanup owner remains, and startup generic-stage recovery deletes the canonical file and retires the owner.

This is exercised by `protected_ciphertext_cleanup_owner_recovers_after_database_commit_crash_boundary`.

## 9. Validator closure

`scripts/validate-vault-2b7-closure-repair.sh` explicitly prohibits the blind spots that invalidated the prior 2b.6 certification and then chains the inherited 2b.1–2b.6 guards.

Historical E0–E3 validators were updated only where build 2b.7 changes the authoritative mechanism. Assertions were strengthened to require typed ownership rather than relaxed to permit raw cleanup.

## 10. Architectural result

The final source still has one privacy architecture:

```text
PrivacyCoordinator
    ├─ transition admission/revocation/drain
    └─ storage mutation serialization

OwnedPlaintext + durable ownership ledger
    └─ managed sensitive plaintext lifetime / transfer

SensitiveNoteData
    └─ deny-by-default protected persistence classification

Vault object store
    ├─ NotePrivateState
    ├─ MetadataState
    ├─ AttachmentIndex
    ├─ RevisionObject
    └─ encrypted attachment blobs

PrivacyPublicationToken
    └─ asynchronous decrypted UI publication lifetime
```

No `ProtectedNotePayload` production representation or permanent dual protected-note writer remains.

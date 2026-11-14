# Batch 2C — Managed-backup key and wire-format authority map

**Purpose:** Reconcile the independent 2b.43.1 key-policy contract, the subordinate 2b.43.2 byte-level format, and 2b.43.3 source implementation report **without modifying any original**. Source line ranges below are fixed to the baseline blobs. This is not an encrypted-backup release claim.

## Normative document ordering

1. **[2b.43.1 — key authority](../../BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md)** defines threat model, ownership, device binding, fail-closed key-loss paths, legacy cleanup, restore and performance. It does not itself implement backups.
2. **[2b.43.2 — precise byte format](../../BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md)** is explicitly **subordinate to 2b.43.1**; it fixes canonical versions, wire bytes, lengths, domain strings, serialization, identity vectors, parser rules, crash-commit semantics and test obligations. It is a specification, not a deployed reader/writer.
3. **[2b.43.3 — implementation report](../../BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md)** reports staged key authority and platform protector source. It does not override either governing contract, and it expressly leaves backup storage integration to later passes.
4. **Current code:** `src/security/backup_key_authority.rs` declares `BACKUP_REPOSITORY_FORMAT_VERSION = 3` for staged key headers; `src/storage/backup_repository.rs` still declares **`REPOSITORY_FORMAT_VERSION = 2`** and `INDEX_FILE_NAME = "repository.json"`. These are **different layers**, not contradictory evidence of an active v3 backup writer. `src/security/mod.rs` and `src/platform/mod.rs` explicitly mark their staged modules with temporary `#[expect(dead_code)]`.

## Key management and threat boundaries (2b.43.1)

- **Managed only:** automatic and pre-restore safety snapshots, encrypted databases/blobs/index/manifest and ciphertext-only staging/quarantine. Manual user-selected portable `.rustnote.zip` archives are **separate ExternalUserOwned outputs** and are not silently encrypted by this contract.
- **Root key:** OS-protected device/installation root for *small keyset wrapping only*, not bulk encryption; never exported into ordinary filesystem/SQLite/config/logs or reused as a Vault NoteKey. No password/biometric prompt per automatic snapshot.
- **Per-repository independent random keyset:** four separately generated 256-bit keys: snapshot AEAD, blob AEAD, metadata AEAD, HMAC opaque-ID. Not derived deterministically from root; wrapped/authenticated under OS root; non-clone, redacted debug and zeroizing lifecycle, bounded-operation acquisition.
- **macOS:** device-local non-synchronizing data-protection Keychain, `AccessibleAfterFirstUnlockThisDeviceOnly`, application-specific service/account. **Android:** non-exportable Android Keystore AES-256-GCM root with non-per-use authentication and hardware backing when available. Unsupported platforms fail closed; adapters are not proof of a shipping mobile app.
- **Binding:** repository and library identity, format/envelope version and object class. Not bound to absolute path: an authenticated repository may move on same device, but cross-device restore or root loss is unsupported without separately designed recovery.
- **Key state machine:** root may be created *only* for conclusively fresh state with no unresolved cleanup obligation; temporary secure-store failure leaves library usable but disallows backup/restore/mutation; deleted/corrupt root/envelope or failed context authentication preserves existing encrypted bytes, forbids key regeneration/destructive repair and plaintext fallback.
- **Legacy development repository:** old plaintext v2 state is **never migrated**. Owned old repositories may be durably tracked and strictly purged; unknown/unowned directories may not be adopted or deleted; offline locations retain cleanup obligations. Protect-time repository purging remains until a fully proven encrypted-only v3 cutover.
- **Transient plaintext:** a consistent SQLite snapshot may exist only in an **already-owned process-private `OwnedPlaintext` stage**, then be stream-encrypted into ciphertext-only repository staging, authenticated and atomically published, with strict cleanup/owner retention and crash recovery. Restore authenticates/decrypts into owned stage *before* existing library-wide Restore authority.
- **Performance:** bounded memory, reusable deduplicated encrypted blobs, no per-snapshot rewrite of unchanged data, no bulk decrypt just to list status. Correctness and I1–I7/E0–E3 invariants dominate speed.

## Exact byte-level authority (2b.43.2)

**Do not reimplement from this summary**; the original contains exact byte offsets, AAD bytes, test vectors, canonical nested schemas and rejection rules. This register preserves high-risk parameters to prevent accidental cross-format substitution:

| Format element | Immutable v1 specification |
| --- | --- |
| Repository/header/keyset/object versions | Repository **3**; header **1**, keyset **1**, backup object **1**, index/manifest payload **1** |
| Repository/header file | `repository.header`: fixed **92-byte** `RNBHDR01` prefix + exact platform envelope, no trailing bytes |
| Wrapped keyset | `RNBKEY01`, exactly **204 bytes**, four independent 32-byte keys and fixed repository/library identities |
| macOS envelope | **268 bytes** = 32-byte random nonce + 204-byte ciphertext + 32-byte AEGIS-256 tag |
| Android envelope | **232 bytes** = 12-byte random IV + 204-byte ciphertext + 16-byte AES-256-GCM tag |
| Encrypted small-object header | **116 bytes**, `RNBIDX01` or `RNBMAN01`, metadata key, full AEGIS-256 tag |
| Small-object AAD domain | `rustnote-backup-small-object-aad-v1`, includes library binding, class byte and complete header |
| Encrypted bulk header | **112 bytes**, `RNBSNP01` or `RNBBLB01`, authenticated with an independent **32-byte header tag** |
| Bulk chunk size | Exactly **1,048,576 bytes (1 MiB)** for backup-v3 v1 objects; unlike the flexible note Vault-blob contract |
| Bulk nonce | Random 24-byte prefix + 8-byte big-endian counter; `0xffffffffffffffff` reserved for header authenticator |
| Bulk chunk AAD | `rustnote-backup-bulk-chunk-aad-v1`, library binding, class byte, fixed header, chunk index and plaintext length |
| Exact bulk container size | `112 + 32 + plaintext_len + 32 * ceil(plaintext_len / 1,048,576)` |
| Small payload bounds | Repository index **4 MiB** maximum; manifest **16 MiB** maximum |
| Bulk payload bounds | Snapshot DB **1..1 TiB**; attachment blob **0..1 TiB** |
| Identity derivation | `library_binding = SHA-256(domain || u32_be(library-id UTF-8 length) || library-id UTF-8)`; `opaque_blob_id = HMAC-SHA-256(object_id_hmac_key, SHA-256(plaintext attachment))` |
| Namespace grammar | 64 lowercase hex IDs with `.rnbd`, `.rnbm`, `.rnbb`; encrypted `index.rnbi`; ciphertext-only `.staging` and `quarantine` |
| Metadata schemas | Canonical fixed binary grammar `RNBIP001` and `RNBMP001`, sorted entries, no duplicate IDs or nonzero reserved bytes |
| Snapshot commit point | **Durable authenticated manifest publication**, after referenced blobs/database; index updated last, reconstructable from authenticated manifests |
| Trust caveats | Actual plaintext digest check after AEAD; strict path/ID cross-checking; overflow/truncation/trailing-byte rejection; **no trusted whole-repository monotonic anti-rollback** in v3 |

Critical domain strings are separately specified in 2b.43.2 §6.1 and §19: `rustnote-backup-key-envelope-aad-v1`, `rustnote-backup-small-object-aad-v1`, `rustnote-backup-bulk-header-aad-v1`, `rustnote-backup-bulk-chunk-aad-v1`. **Never reuse Vault NoteKey domains** for backup objects. MAC/test vectors in §20 are normative and must be verified by code and tests.

## 2b.43.3 implementation report: evidence hierarchy

The report identifies `DeviceBackupKeyProtector` and `BackupKeyAuthority` source files, macOS SecItem and Android mobile-shell boundary, Apple envelope crypto in `rustnote-vault`, strict keyset/header parsing, and source-level/mock regression cases. A later appended native-compile follow-up reports test/Clippy repairs and a native test-run observation. **Those are pass-local historical reports**, not new validation performed on this consolidation branch. Additional verification includes production Keychain on actual supported macOS/codesigned app identity.

The report's **explicit exclusions** remain authoritative for *the 2b.43.3 pass*: no repository.header writer, no encrypted snapshot/blob/index/manifest persistence, no v3 Restore, no removal of Protect-time legacy cleanup. The working branch has not altered those facts.

## Risks and explicit non-goals

- OS secure-store loss can make device-bound managed backups unrecoverable; do not promise cross-device transfer or recovery phrases.
- External user-created archives retain their old semantics until separately specified. Do not assume every backup file is encrypted.
- Even authenticated v3 objects are not protected from **whole-repository rollback to an older valid encrypted state** without separate trusted monotonic state.
- No plaintext fallback, unknown-repository adoption, root replacement, old repository migration, SQLCipher, cloud sync or independent secondary plaintext-lifetime registry is authorized.
- When implementation succeeds later, removing the two temporary module-level `#[expect(dead_code)]` annotations is a required integration follow-up, **not** a reason to add static-code pass checks for their continued presence.

## Complete clause-level source traceability

### 2b.43.1: [original source](../../BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md), Git blob `daa051416b223c3de07f7223b961d150e7fb0be7`

| Original clause or section | Source lines | Authority treatment |
| --- | ---: | --- |
| `Rustnote 2b.43.1 — Encrypted Managed Backup Key Authority Contract` | 1–2 | Governing key/security requirements |
| `Status` | 3–20 | Governing key/security requirements |
| `1. Scope` | 21–39 | Governing key/security requirements |
| `2. Current architecture audit` | 40–72 | Governing key/security requirements |
| `3. Threat model and security objective` | 73–113 | Governing key/security requirements |
| `4. Key hierarchy` | 114–115 | Governing key/security requirements |
| `4.1 Device Backup Root Key` | 116–143 | Governing key/security requirements |
| `4.2 Repository keysets are independent random keys` | 144–170 | Governing key/security requirements |
| `5. OS secure-storage contract` | 171–172 | Governing key/security requirements |
| `5.1 Platform-neutral authority` | 173–189 | Governing key/security requirements |
| `5.2 macOS profile` | 190–204 | Governing key/security requirements |
| `5.3 Android profile` | 205–220 | Governing key/security requirements |
| `5.4 Other platforms` | 221–228 | Governing key/security requirements |
| `6. Device binding` | 229–253 | Governing key/security requirements |
| `7. Cryptographic domain separation` | 254–288 | Governing key/security requirements |
| `8. Repository identity and opaque deduplication` | 289–316 | Governing key/security requirements |
| `9. Canonical encrypted repository format` | 317–359 | Governing key/security requirements |
| `10. Plaintext creation during backup` | 360–392 | Governing key/security requirements |
| `11. Key creation and retrieval state machine` | 393–396 | Governing key/security requirements |
| `A. Fresh state` | 397–420 | Governing key/security requirements |
| `B. Temporarily unavailable secure store` | 421–437 | Governing key/security requirements |
| `C. Root key absent/deleted while encrypted repository exists` | 438–455 | Governing key/security requirements |
| `D. Root key or key envelope corrupt` | 456–463 | Governing key/security requirements |
| `12. Repository initialization and corruption rules` | 464–481 | Governing key/security requirements |
| `13. Legacy development repository rule` | 482–524 | Governing key/security requirements |
| `14. Restore contract` | 525–528 | Governing key/security requirements |
| `Phase 1 — authenticate/decrypt into owned private staging` | 529–548 | Governing key/security requirements |
| `Phase 2 — existing library-wide Restore authority` | 549–568 | Governing key/security requirements |
| `15. Restore after key loss` | 569–583 | Governing key/security requirements |
| `16. Key lifetime and zeroization` | 584–585 | Governing key/security requirements |
| `Device root` | 586–592 | Governing key/security requirements |
| `Repository keyset` | 593–601 | Governing key/security requirements |
| `Derived plaintext/key material` | 602–607 | Governing key/security requirements |
| `17. Backup behavior before secure storage is available` | 608–628 | Governing key/security requirements |
| `18. Backup location changes` | 629–640 | Governing key/security requirements |
| `19. Protect-time repository purge end-state` | 641–665 | Governing key/security requirements |
| `20. Required failure semantics` | 666–696 | Governing key/security requirements |
| `21. Performance contract` | 697–715 | Governing key/security requirements |
| `22. Explicit non-goals` | 716–738 | Governing key/security requirements |
| `23. Implementation sequence governed by this contract` | 739–761 | Governing key/security requirements |
| `24. Mandatory acceptance tests for later implementation` | 762–791 | Governing key/security requirements |
| `25. Definition of Done for 2b.43.1` | 792–813 | Governing key/security requirements |

### 2b.43.2: [original source](../../BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md), Git blob `bdc8606d8b73acc53e91fa2a69e15b0c33c0ad47`

| Original clause or section | Source lines | Authority treatment |
| --- | ---: | --- |
| `Rustnote 2b.43.2 — Encrypted Managed Backup Byte-Format Contract` | 1–2 | Governing exact v3 format; subordinate to 2b.43.1 |
| `Status` | 3–20 | Governing exact v3 format; subordinate to 2b.43.1 |
| `1. Design goals` | 21–42 | Governing exact v3 format; subordinate to 2b.43.1 |
| `2. Binary conventions` | 43–75 | Governing exact v3 format; subordinate to 2b.43.1 |
| `3. Fixed version and algorithm identifiers` | 76–116 | Governing exact v3 format; subordinate to 2b.43.1 |
| `4. Canonical identities` | 117–118 | Governing exact v3 format; subordinate to 2b.43.1 |
| `4.1 Repository ID` | 119–124 | Governing exact v3 format; subordinate to 2b.43.1 |
| `4.2 Snapshot ID` | 125–132 | Governing exact v3 format; subordinate to 2b.43.1 |
| `4.3 Library binding` | 133–148 | Governing exact v3 format; subordinate to 2b.43.1 |
| `4.4 Opaque attachment object ID` | 149–163 | Governing exact v3 format; subordinate to 2b.43.1 |
| `5. Repository layout and filenames` | 164–188 | Governing exact v3 format; subordinate to 2b.43.1 |
| `6. `repository.header` exact encoding` | 189–227 | Governing exact v3 format; subordinate to 2b.43.1 |
| `6.1 Key-envelope AAD` | 228–242 | Governing exact v3 format; subordinate to 2b.43.1 |
| `7. Repository keyset plaintext — exact 204 bytes` | 243–264 | Governing exact v3 format; subordinate to 2b.43.1 |
| `8. Platform key-envelope encodings` | 265–266 | Governing exact v3 format; subordinate to 2b.43.1 |
| `8.1 macOS / Apple Keychain root` | 267–282 | Governing exact v3 format; subordinate to 2b.43.1 |
| `8.2 Android Keystore root` | 283–302 | Governing exact v3 format; subordinate to 2b.43.1 |
| `9. Small encrypted object container` | 303–353 | Governing exact v3 format; subordinate to 2b.43.1 |
| `9.1 Small-object AAD` | 354–370 | Governing exact v3 format; subordinate to 2b.43.1 |
| `10. Bulk encrypted object container` | 371–404 | Governing exact v3 format; subordinate to 2b.43.1 |
| `10.1 Nonces` | 405–425 | Governing exact v3 format; subordinate to 2b.43.1 |
| `10.2 Header authentication` | 426–440 | Governing exact v3 format; subordinate to 2b.43.1 |
| `10.3 Chunk records` | 441–464 | Governing exact v3 format; subordinate to 2b.43.1 |
| `10.4 Exact bulk file length` | 465–482 | Governing exact v3 format; subordinate to 2b.43.1 |
| `11. Repository index plaintext schema` | 483–518 | Governing exact v3 format; subordinate to 2b.43.1 |
| `12. Snapshot manifest plaintext schema` | 519–557 | Governing exact v3 format; subordinate to 2b.43.1 |
| `13. Digest semantics` | 558–573 | Governing exact v3 format; subordinate to 2b.43.1 |
| `14. Atomic publication and commit ordering` | 574–575 | Governing exact v3 format; subordinate to 2b.43.1 |
| `14.1 Attachment blob publication` | 576–602 | Governing exact v3 format; subordinate to 2b.43.1 |
| `14.2 Snapshot database publication` | 603–616 | Governing exact v3 format; subordinate to 2b.43.1 |
| `14.3 Manifest is the snapshot commit point` | 617–636 | Governing exact v3 format; subordinate to 2b.43.1 |
| `14.4 Index publication` | 637–644 | Governing exact v3 format; subordinate to 2b.43.1 |
| `15. Deletion/pruning semantics` | 645–668 | Governing exact v3 format; subordinate to 2b.43.1 |
| `16. Crash recovery rules` | 669–683 | Governing exact v3 format; subordinate to 2b.43.1 |
| `17. Parser and resource limits` | 684–708 | Governing exact v3 format; subordinate to 2b.43.1 |
| `18. Object/path cross-checking` | 709–732 | Governing exact v3 format; subordinate to 2b.43.1 |
| `19. Domain-separation summary` | 733–764 | Governing exact v3 format; subordinate to 2b.43.1 |
| `20. Canonical structural test vectors` | 765–768 | Governing exact v3 format; subordinate to 2b.43.1 |
| `20.1 Library binding vector` | 769–792 | Governing exact v3 format; subordinate to 2b.43.1 |
| `20.2 Opaque blob ID vector` | 793–815 | Governing exact v3 format; subordinate to 2b.43.1 |
| `21. Rollback/freshness scope` | 816–825 | Governing exact v3 format; subordinate to 2b.43.1 |
| `22. Explicitly forbidden representations` | 826–848 | Governing exact v3 format; subordinate to 2b.43.1 |
| `23. Relationship to existing `rustnote-vault`` | 849–878 | Governing exact v3 format; subordinate to 2b.43.1 |
| `24. Mandatory implementation tests derived from this format` | 879–916 | Governing exact v3 format; subordinate to 2b.43.1 |
| `25. Definition of Done for 2b.43.2` | 917–941 | Governing exact v3 format; subordinate to 2b.43.1 |

### 2b.43.3: [original source](../../BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md), Git blob `2d2acf1dfcdd5acdcb9e233ae6dd6d075f30631c`

| Original clause or section | Source lines | Authority treatment |
| --- | ---: | --- |
| `Rustnote 2b.43.3 — OS secure-key-store + BackupKeyAuthority` | 1–2 | Historical staged implementation/evidence; not deployed v3 |
| `Status` | 3–8 | Historical staged implementation/evidence; not deployed v3 |
| `Governing contracts` | 9–17 | Historical staged implementation/evidence; not deployed v3 |
| `Objective` | 18–36 | Historical staged implementation/evidence; not deployed v3 |
| `Implemented architecture` | 37–38 | Historical staged implementation/evidence; not deployed v3 |
| ``src/platform/secure_key_store.rs`` | 39–51 | Historical staged implementation/evidence; not deployed v3 |
| `macOS` | 52–82 | Historical staged implementation/evidence; not deployed v3 |
| `Android` | 83–97 | Historical staged implementation/evidence; not deployed v3 |
| `Unsupported platforms` | 98–103 | Historical staged implementation/evidence; not deployed v3 |
| `Canonical crypto boundary` | 104–133 | Historical staged implementation/evidence; not deployed v3 |
| `BackupKeyAuthority` | 134–152 | Historical staged implementation/evidence; not deployed v3 |
| `Fresh repository` | 153–172 | Historical staged implementation/evidence; not deployed v3 |
| `Existing repository` | 173–190 | Historical staged implementation/evidence; not deployed v3 |
| `Failure model` | 191–210 | Historical staged implementation/evidence; not deployed v3 |
| `Direct regression coverage added` | 211–242 | Historical staged implementation/evidence; not deployed v3 |
| `Deliberate staged boundary` | 243–257 | Historical staged implementation/evidence; not deployed v3 |
| `Explicitly not implemented in this pass` | 258–270 | Historical staged implementation/evidence; not deployed v3 |
| `macOS execution constraint` | 271–276 | Historical staged implementation/evidence; not deployed v3 |
| `Native compile follow-up` | 277–281 | Historical staged implementation/evidence; not deployed v3 |
| `Native compile follow-up — second macOS round` | 282–292 | Historical staged implementation/evidence; not deployed v3 |
| `Native QA source-contract hardening` | 293–296 | Historical staged implementation/evidence; not deployed v3 |

## Batch 2C QA boundary

All three original sources were read in full, and current repository source was checked for staged-v3 versus active-v2 separation, secure-store and key-envelope types, and the two documented dead-code expectations. The original contract files retain their exact paths and blob contents. No native build, parser fuzz, end-to-end ciphertext test, Keychain device test or crash-recovery test was run in this documentation pass; these remain mandatory before production encryption claims.

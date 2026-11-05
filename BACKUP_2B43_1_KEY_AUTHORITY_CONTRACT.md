# Rustnote 2b.43.1 — Encrypted Managed Backup Key Authority Contract

## Status

**Governing contract only. No encrypted-backup storage implementation begins in this pass.**

Baseline:

```text
Rustnote v0.12.6
build 2b.43
post-2b.42 E0–E3 / I1–I7 closure baseline
```

2b.43.1 defines the security and key-management contract for the next architecture series: always-encrypted Rustnote-managed automatic and pre-restore safety backups.

The contract exists before implementation so later storage work has one finite authority rather than evolving cryptographic semantics through patches.

---

# 1. Scope

2b.43 covers the Rustnote-managed backup repository only:

```text
automatic snapshots
pre-restore safety snapshots
managed snapshot database objects
managed attachment/blob objects
managed repository manifests/indexes
managed repository staging/quarantine
```

It does **not** silently change user-selected portable output such as manual `.rustnote.zip` complete archives. Those remain `ExternalUserOwned` outputs under their existing explicit export semantics until a separate portable encrypted-archive design is approved.

The managed repository must become encrypted at rest regardless of whether its notes are ordinary or individually protected by Vault.

---

# 2. Current architecture audit

The 2b.43 baseline currently stores the managed repository as:

```text
<Rustnote backup parent>/
├── .rustnote-backup-repository-owner
└── Rustnote Repository/
    ├── repository.json
    ├── snapshots/<snapshot_id>.sqlite3
    ├── blobs/<plaintext-sha256>
    ├── manifests/<snapshot_id>.json
    ├── .staging/
    └── quarantine/
```

Current properties confirmed by source audit:

- the external namespace owner survives recursive repository deletion;
- snapshot databases are ordinary SQLite snapshots;
- ordinary attachment blobs are copied as ordinary files;
- blob deduplication uses the plaintext SHA-256 as repository identity;
- repository index/manifests are plaintext JSON;
- automatic and safety snapshots share retention/deduplication;
- Restore copies a selected snapshot into process-private restore staging before the library-wide install transition;
- `OwnedPlaintext`, process-private staging cleanup, and `PrivacyCoordinator` already provide authoritative plaintext lifetime and Restore ordering;
- Protect currently resets every remembered managed backup repository because historical snapshots can contain the newly protected note in plaintext;
- Rustnote presently has no OS-backed secure-key-store abstraction.

These facts define the delta for 2b.43. They do not justify replacing the existing ownership, privacy-transition, snapshot-retention, deduplication, or Restore authorities.

---

# 3. Threat model and security objective

The managed-backup encryption layer protects backup storage against offline disclosure:

```text
copied/stolen backup directory
lost external drive
filesystem snapshot
another process reading backup files without Rustnote's OS key authority
```

The layer does not claim to protect plaintext from a fully compromised running OS/user session that can command Rustnote or the OS secure store to use its key.

The end-state invariant is:

> Every byte of Rustnote-managed note/database/attachment backup payload is ciphertext at rest outside a deliberately owned transient plaintext lifetime.

Allowed observable repository metadata is limited to what the filesystem or minimal bootstrap header inherently exposes:

```text
repository existence
format/version
random opaque repository/object identifiers
ciphertext file sizes
filesystem timestamps
```

The repository must not expose in plaintext:

```text
note/document content
ordinary SQLite pages
attachment bytes
raw attachment SHA-256 identities
note titles/tags/sources/citations/annotations
snapshot note/attachment/revision counts
repository manifest/index contents
```

---

# 4. Key hierarchy

## 4.1 Device Backup Root Key

Rustnote has one logical **Device Backup Root Key authority** per application installation/device security domain.

The root key:

- is created by or stored through the operating-system secure key facility;
- is never stored in the Rustnote database, backup repository, configuration files, logs, or ordinary filesystem;
- is never used directly to encrypt snapshot databases, attachment blobs, manifests, or indexes;
- exists only to authenticate and wrap/unwrap small repository keysets;
- is independent from Vault note passwords, NoteKeys, notebook keys, and any future SQLCipher/live-database key;
- must not require an interactive biometric/password prompt for every automatic backup;
- must never have a plaintext fallback.

Storage code must interact with a narrow platform/security authority, not with Keychain/Keystore APIs directly.

Conceptually:

```text
platform secure key store
        ↓
DeviceBackupRootKey authority
        ↓
wrap / unwrap only
        ↓
RepositoryKeyset envelope
```

## 4.2 Repository keysets are independent random keys

Repository keys are **not deterministically derived from the device root key**.

Every new managed repository receives a freshly random, independent keyset:

```text
BackupRepositoryKeysetV1 {
    snapshot_aead_key   : 256 bits
    blob_aead_key       : 256 bits
    metadata_aead_key   : 256 bits
    object_id_hmac_key  : 256 bits
}
```

Rationale:

- compromise/corruption is repository-scoped where possible;
- new backup locations naturally get new keys;
- key rotation/rewrapping can later replace only the small envelope;
- no deterministic cross-repository key relation exists;
- bulk data never needs re-encryption merely because the OS wrapping authority changes.

The complete keyset is authenticated-encrypted under the Device Backup Root Key authority and stored only as a **wrapped repository-key envelope**.

---

# 5. OS secure-storage contract

## 5.1 Platform-neutral authority

Introduce later, in implementation passes, one narrow platform boundary conceptually equivalent to:

```text
DeviceBackupKeyProtector

create_root_if_absent_for_fresh_state()
root_status()
seal_repository_keyset(context, keyset)
open_repository_keyset(context, envelope)
```

The API must not expose a raw OS root key to storage.

The exact Rust API is an implementation detail of a later pass, but these semantics are mandatory.

## 5.2 macOS profile

The intended macOS profile is:

```text
SecItem / data-protection Keychain
non-synchronizable
kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
application-specific service/account identity
```

The choice is device-local and background-capable after the user has unlocked the Mac once after boot. No iCloud synchronization is requested by Rustnote.

If the implementation must materialize symmetric root bytes in the Rust process, that occurs only inside the narrow platform/security authority and the bytes are zeroized immediately after use.

## 5.3 Android profile

The intended Android profile is:

```text
AndroidKeyStore
AES-256-GCM wrapping key
PURPOSE_ENCRYPT | PURPOSE_DECRYPT
non-exportable key material
no per-use user-authentication requirement
randomized encryption required
hardware-backed when the device provides it
```

StrongBox is not a mandatory availability requirement. A supported Android device may use the Android Keystore's non-exportable software/TEE-backed implementation, but Rustnote must never fall back to storing the root key as a plaintext application file.

## 5.4 Other platforms

Any platform without an approved secure-key-store implementation has **managed encrypted backup unavailable**.

It must not fall back to plaintext managed backups.

---

# 6. Device binding

Managed repositories in 2b.43 are device-bound.

The repository-key envelope is authenticated against at least:

```text
Rustnote backup domain identifier
repository format version
key-envelope version
library_id
repository_id
```

It is deliberately **not bound to an absolute filesystem path**, so a repository may be relocated on the same device through Rustnote's location-management flow without re-encrypting all payloads.

Consequences:

- copying a managed repository to another device is not a supported restore mechanism in 2b.43;
- loss of the OS root key makes that device-bound repository unreadable;
- automatic managed backups are therefore distinct from portable manual archives;
- cross-device encrypted recovery/key escrow is a future explicit design, not an accidental side effect of this pass.

---

# 7. Cryptographic domain separation

Managed-backup cryptography is a separate Rustnote security domain from Vault-note encryption even when it reuses reviewed AEGIS-256 streaming/object primitives.

The backup object model must have explicit backup-specific authenticated context.

At minimum every encrypted object binds:

```text
backup format/version
repository_id
library_id
object class
opaque object/snapshot identity
plaintext length where applicable
chunk index/record identity for streaming objects
```

Required object classes include at least:

```text
RepositoryMetadata
SnapshotManifest
SnapshotDatabase
AttachmentBlob
RepositoryIndex
RepositoryKeyEnvelope
```

Keys are also purpose-separated by the independent repository keyset, so a ciphertext valid as one object class cannot be accepted under another class/key.

No Vault NoteKey, password-derived key, or protected-note key envelope is reused for managed backup encryption.

---

# 8. Repository identity and opaque deduplication

Each repository receives a cryptographically random opaque `repository_id`.

Snapshot identifiers remain random opaque identifiers.

Attachment deduplication remains content-based, but raw plaintext SHA-256 values must no longer be exposed as repository filenames.

Canonical logical identity:

```text
plaintext_digest = SHA-256(attachment bytes)
opaque_blob_id   = HMAC-SHA-256(object_id_hmac_key, plaintext_digest)
```

The encrypted manifest may retain the plaintext digest and byte size for internal verification because the manifest itself is encrypted.

This preserves:

```text
one attachment used by many snapshots
→ one encrypted repository blob
```

without exposing known-file SHA-256 names to an offline observer.

---

# 9. Canonical encrypted repository format

The next repository format is a clean break:

```text
REPOSITORY_FORMAT_VERSION = 3
KEY_ENVELOPE_VERSION      = 1
BACKUP_OBJECT_VERSION     = 1
```

Logical layout:

```text
<Rustnote backup parent>/
├── .rustnote-backup-repository-owner          # durable ownership only
└── Rustnote Repository/
    ├── repository.header                      # minimal bootstrap metadata + wrapped keyset
    ├── index.rnbi                             # encrypted
    ├── snapshots/<opaque_snapshot_id>.rnbd    # encrypted SQLite snapshot bytes
    ├── manifests/<opaque_snapshot_id>.rnbm    # encrypted
    ├── blobs/<opaque_blob_id>.rnbb             # encrypted, deduplicated
    ├── .staging/                               # ciphertext-only repository staging
    └── quarantine/                             # ciphertext-only repository artifacts
```

`repository.header` may expose only the minimum required to locate/authenticate the repository key envelope, such as:

```text
magic
repository format version
key-envelope version
repository_id
library_id or equivalent authenticated library binding
platform protector/envelope metadata
wrapped repository keyset
```

It must contain no note content, snapshot counts, attachment names, raw content hashes, or other user-derived backup metadata.

The precise byte encoding is defined in 2b.43.2, but it may not contradict this logical contract.

---

# 10. Plaintext creation during backup

Always-encrypted backups do **not** imply that SQLite can magically emit an encrypted database stream without a consistent plaintext snapshot boundary.

The permitted pattern is:

```text
reserve authoritative process-private OwnedPlaintext snapshot
        ↓
create consistent SQLite snapshot there
        ↓
stream-encrypt snapshot to ciphertext-only repository staging
        ↓
authenticate + atomically publish encrypted object
        ↓
strictly delete transient plaintext snapshot
        ↓
retire plaintext ownership
```

Rules:

- no plaintext SQLite snapshot may ever be created inside the managed backup repository;
- plaintext snapshot ownership must exist before the first byte is written (I1);
- backup success cannot be reported until the transient plaintext snapshot is deleted or its cleanup obligation remains authoritatively owned (I2);
- crash/restart cleanup must reuse the existing process-private/OwnedPlaintext authority rather than introduce a backup-specific cleanup registry;
- ordinary attachment sources may be streamed directly from their canonical files into encrypted ciphertext staging; no extra plaintext copy is required;
- protected attachment ciphertext may be wrapped again by repository encryption without exposing decrypted attachment bytes.

Repository `.staging` must contain ciphertext only.

---

# 11. Key creation and retrieval state machine

The root/key authority distinguishes four states.

## A. Fresh state

```text
no Device Backup Root Key
AND
no supported encrypted repository
AND
no unresolved legacy plaintext-repository cleanup obligation
```

On first managed-backup initialization/use:

```text
create Device Backup Root Key atomically in OS secure store
↓
generate independent RepositoryKeysetV1
↓
seal keyset
↓
create v3 repository
```

Root creation should be lazy to the managed-backup subsystem; merely opening/editing notes must not require creation of backup key material.

## B. Temporarily unavailable secure store

Examples include OS key store locked/unavailable or platform adapter unavailable.

Behavior:

```text
library remains usable
managed automatic backup = unavailable/retryable error
managed restore = unavailable
repository mutation = forbidden
plaintext fallback = forbidden
new replacement root = forbidden
```

Existing repository bytes are left untouched.

## C. Root key absent/deleted while encrypted repository exists

Behavior:

```text
FAIL CLOSED
```

Rustnote must:

- report backup-key loss/unavailability distinctly;
- preserve the encrypted repository unchanged;
- refuse backup creation, pruning, reconciliation that destroys data, inspection requiring decryption, and Restore;
- never silently create a new root key and overwrite/reinitialize the repository;
- never claim the old snapshots are valid under a replacement key.

A future explicit user-confirmed "discard unreadable managed backups and start fresh" operation may be designed separately.

## D. Root key or key envelope corrupt

Wrong length, invalid secure-store data, authentication failure, malformed envelope, or context mismatch is an integrity failure.

Behavior is the same fail-closed preservation rule as key loss. No automatic deletion, adoption, regeneration, or plaintext fallback.

---

# 12. Repository initialization and corruption rules

A v3 repository is initialized only when the repository is conclusively absent/fresh under the existing durable namespace ownership authority.

If any v3 bootstrap/header/key envelope exists but cannot be authenticated:

```text
preserve
report integrity failure
perform no destructive repair
```

A repository with snapshots must never receive a newly generated keyset merely because its key envelope cannot be opened.

Quarantine may move ciphertext artifacts only when normal repository integrity logic positively proves they are unreferenced/invalid under a successfully opened repository. Key unavailability alone is never authority to quarantine or delete data.

---

# 13. Legacy development repository rule

Backward compatibility and data migration are explicit non-goals.

Rustnote does **not** implement:

```text
v2 plaintext repository → v3 encrypted repository conversion
legacy manifest reader compatibility
legacy key adoption
old encrypted-repository key migration framework
```

The only permitted treatment of obsolete Rustnote-managed plaintext repositories is **destructive privacy cleanup**, not migration:

```text
prove existing Rustnote durable namespace ownership
↓
publish/retain cleanup obligation
↓
strictly purge obsolete managed repository
↓
prove absence
↓
create fresh v3 encrypted repository only if secure key authority is available
```

Unknown/unowned directories are never adopted or deleted.

Offline/removable historical backup locations retain a durable pending cleanup obligation until the actual owned repository returns and is positively purged.

The existing Protect-time plaintext-backup purge must not be removed until 2b.43 proves that every active/remembered Rustnote-managed repository is either:

```text
v3 encrypted
or
retired/purged with no unresolved plaintext obligation
```

This is a cutover cleanup rule, not a compatibility layer.

---

# 14. Restore contract

Restore from an encrypted managed repository occurs in two distinct phases.

## Phase 1 — authenticate/decrypt into owned private staging

```text
open repository keyset
↓
authenticate/decrypt manifest/index
↓
reserve process-private OwnedPlaintext restore stage
↓
stream-decrypt database/blobs into owned staging
↓
validate complete staged snapshot
↓
drop repository key lease when no longer needed
```

No plaintext restore file may be created before ownership exists.

Authentication/decryption failure must clean the partial stage or leave its cleanup obligation alive.

## Phase 2 — existing library-wide Restore authority

Only after a complete validated stage exists:

```text
begin LibraryPrivacyTransition::Restore
↓
install staged database/blobs through existing recoverable ownership handoff
↓
validate installed library
↓
rollback using existing rules if required
↓
strictly clean restore staging
```

2b.43 must reuse the existing `PrivacyCoordinator`, `OwnedPlaintext`, process-private restore staging, and canonical restore ownership transfer. It must not invent a separate backup privacy state machine.

---

# 15. Restore after key loss

If the Device Backup Root Key or repository keyset is unavailable/lost:

- managed repository Restore is impossible by design;
- Rustnote fails before creating any plaintext restore stage;
- encrypted repository files remain untouched;
- Rustnote must not guess, brute-force, regenerate, or substitute keys;
- current library data remains usable if otherwise healthy;
- user-selected portable/manual archives remain a separate recovery channel under their existing semantics.

Cross-device/key-escrow recovery is outside 2b.43 and requires a future explicit recovery-key design.

---

# 16. Key lifetime and zeroization

## Device root

- storage never owns a raw root key;
- Android root key remains non-exportable in Android Keystore;
- macOS root secret, if materialized by the platform implementation, stays inside the narrow secure-store module and is zeroized immediately after wrapping/unwrapping;
- no root key is cached in `Storage`, UI state, static globals, logs, debug output, or serialized structures.

## Repository keyset

- non-`Clone` key owner;
- redacted `Debug`;
- zeroized on drop;
- acquired only for the bounded repository operation that needs it;
- not cached for the process lifetime;
- not stored in SQLite, application settings, repository plaintext metadata, or telemetry/profiling.

## Derived plaintext/key material

Any temporary plaintext metadata/decryption buffer containing repository secrets must use zeroizing owners where practical and must never be logged.

---

# 17. Backup behavior before secure storage is available

The main Rustnote library must remain usable when the OS secure-key facility is temporarily unavailable.

Managed backup behavior is instead:

```text
automatic backup request
→ fail/skip with explicit secure-key-unavailable status
→ no repository mutation
→ no plaintext fallback
```

Startup/deferred maintenance must not destructively reconcile an encrypted repository without a valid repository keyset.

Protect/Lock/Unlock/Remove Encryption must not weaken their guarantees merely because backups are unavailable.

After the v3 cutover is fully closed, Protect should not require the backup root key because already-encrypted managed repositories cannot contain Rustnote-managed plaintext backup copies.

---

# 18. Backup location changes

Changing the configured backup location creates a new repository with a new independent random keyset.

An old v3 repository remains decryptable on the same device if reconnected because its own wrapped keyset travels with the repository and the Device Backup Root Key can authenticate/unseal it.

Cryptographic binding is to repository/library identity, not absolute path.

Existing path ownership/history rules remain responsible for preventing one library from silently adopting another library's repository.

---

# 19. Protect-time repository purge end-state

The current E3 mechanism:

```text
Protect note B
↓
purge/reset all managed historical backup repositories
```

remains valid during transition.

It may be removed only in a later 2b.43 pass after executable proof that:

1. every newly created automatic/safety repository object is encrypted;
2. no plaintext database/blob/manifest/index is ever published to the repository;
3. obsolete plaintext repositories are durably purged or remain tracked as unresolved cleanup obligations;
4. backup-location change/reconnect cannot reintroduce an old plaintext repository as active;
5. Restore works from v3 encrypted objects using owned plaintext staging;
6. key-unavailable paths never fall back to plaintext.

Only then may Protect stop resetting encrypted historical repositories.

---

# 20. Required failure semantics

The following are always fail-closed:

```text
OS secure store unavailable
root key missing while repository exists
root key corrupt
repository key envelope authentication failure
wrong library/repository context
unsupported repository format
ciphertext authentication failure
truncated encrypted object
unexpected plaintext file in v3 payload namespace
owned plaintext cleanup failure
decrypted restore staging cleanup failure
```

No failure may cause Rustnote to:

```text
write a plaintext managed backup
replace an existing repository key automatically
adopt unknown repository data
delete unreadable encrypted backups automatically
publish an unverified Restore
retire plaintext ownership before conclusive absence
```

---

# 21. Performance contract

Always-encrypted managed backups must preserve Rustnote's lightweight character.

Required properties:

- database encryption is streaming after the consistent SQLite snapshot boundary;
- attachment encryption/decryption is bounded-memory;
- attachment deduplication remains repository-wide;
- unchanged attachment blobs are not re-encrypted per snapshot;
- creating one snapshot does not decrypt or re-encrypt previous snapshots;
- repository key unwrap is O(1) and small;
- status/pruning may decrypt small metadata objects but must not decrypt bulk snapshot/blob payloads merely to list repository state;
- Protect no longer performs whole-repository purge once the encrypted-backup cutover is proven.

No optimization may weaken I1–I7 or E0–E3.

---

# 22. Explicit non-goals

2b.43 does not introduce:

```text
user password prompts for routine automatic backups
reuse of Vault passwords/NoteKeys
cross-device managed-backup restore
encrypted portable archive format
cloud backup/sync
SQLCipher/live-database encryption
backup key escrow/recovery phrases
repository migration from obsolete development formats
plaintext fallback when secure storage fails
per-snapshot re-encryption of unchanged attachment blobs
new plaintext cleanup registries
new privacy-transition authorities
```

Future work may add portable encrypted recovery or live-database encryption, but it must derive from separate domain-separated keys/authorities rather than reusing backup payload keys.

---

# 23. Implementation sequence governed by this contract

No storage implementation occurs in 2b.43.1.

Recommended subsequent passes:

```text
2b.43.2  byte-level encrypted repository/object format
2b.43.3  OS secure-key-store + BackupKeyAuthority
2b.43.4  encrypted SQLite snapshot writer
2b.43.5  encrypted deduplicated attachment/blob repository
2b.43.6  encrypted restore/staging path
2b.43.7  cut over Protect from repository purge to encrypted-backup invariant
2b.43.8  crash/restart/key-loss/adversarial closure
2b.43.9  performance + dead-code/legacy cleanup
2b.43.10 independent de-novo security audit
2b.43.11 final QA/package
```

A later pass may refine this decomposition if the audit reveals a cleaner boundary, but it may not weaken this contract silently.

---

# 24. Mandatory acceptance tests for later implementation

At minimum implementation must eventually prove:

```text
root key absent + fresh state → safe first initialization
root key temporarily unavailable → no backup write, no plaintext fallback
root key deleted + existing repo → fail closed, preserve repo
corrupt root/key envelope → fail closed, preserve repo
wrong library_id/repository_id → authentication failure
same device + moved repository path → repository remains usable
other device/root key → repository cannot be opened
new backup location → independent random repository keyset
same plaintext attachment in many snapshots → one opaque encrypted blob
known SHA-256 cannot be inferred from repository filename
snapshot DB bytes contain no plaintext secret markers
manifest/index bytes contain no plaintext note metadata
repository .staging contains ciphertext only
crash during snapshot encryption → no unowned plaintext survives
crash after ciphertext write before manifest publish → recover/prune safely
crash during Restore decrypt → process-private plaintext remains owned/recoverable
ciphertext corruption → authentication failure before install
key loss Restore → no plaintext stage created
legacy owned v2 repo → purged, never migrated
unknown/unowned repo → never adopted/deleted
Protect after v3 closure → does not purge valid encrypted backup history
```

---

# 25. Definition of Done for 2b.43.1

2b.43.1 is complete when:

1. the current backup architecture has been mapped from source;
2. Device Backup Root Key purpose is finite and narrow;
3. repository keys are explicitly independent random keysets;
4. device-binding semantics are explicit;
5. macOS and Android secure-store profiles are specified;
6. root/key unavailable, deleted and corrupt states have fail-closed behavior;
7. repository/key/object format versions are fixed conceptually;
8. cryptographic domain separation is explicit;
9. opaque deduplication identity is specified;
10. key lifetime/zeroization rules are explicit;
11. backup behavior before secure storage is available is explicit;
12. Restore and key-loss behavior are explicit;
13. legacy-development repository migration is explicitly prohibited;
14. the existing E0–E3/I1–I7 authorities are reused rather than duplicated;
15. no production backup/storage behavior has been implemented or changed in this pass.

Only after all fifteen are true may 2b.43.2 begin.

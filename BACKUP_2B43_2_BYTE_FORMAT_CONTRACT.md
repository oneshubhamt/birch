# Rustnote 2b.43.2 — Encrypted Managed Backup Byte-Format Contract

## Status

**Governing format contract only. No encrypted-backup production reader/writer is implemented in this pass.**

Baseline:

```text
Rustnote v0.12.6
build 2b.43
2b.43.1 key-authority contract closed
```

This contract fixes the exact v3 managed-backup repository encoding before OS-key integration or encrypted storage implementation begins.

It is subordinate to `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`. If an implementation choice conflicts with 2b.43.1, the implementation is wrong; this document may only refine, not weaken, that contract.

---

# 1. Design goals

The format must simultaneously provide:

```text
confidentiality of every managed backup payload
strong authentication before publication/use
repository/library/object domain binding
bounded-memory streaming for large objects
immutable deduplicated attachment objects
recoverable atomic publication
strict parser bounds for untrusted repository bytes
one canonical encoding per logical value
no plaintext JSON/index/manifest payloads
no raw attachment SHA-256 filenames
no migration compatibility with obsolete development repositories
```

The format deliberately does **not** introduce a general serialization framework. v1 metadata payloads use a small fixed Rustnote binary grammar defined below.

---

# 2. Binary conventions

Unless a field explicitly says otherwise:

- byte offsets are zero-based and the end offset is exclusive;
- all integers are unsigned big-endian;
- `i64` timestamps use two's-complement big-endian representation;
- booleans are exactly `0x00` or `0x01`;
- reserved bytes/fields **must be zero on write and zero on read**;
- UTF-8 strings are canonical UTF-8, never NUL-terminated;
- a parser rejects invalid UTF-8, overlong fields, unknown enum values, nonzero reserved fields, truncation, integer overflow, and trailing bytes;
- counts are validated before allocation/multiplication;
- ciphertext files are parsed from their actual length, never from filesystem extension alone;
- the maximum managed repository budget remains 1 TiB, so no individual bulk plaintext object may exceed 1 TiB in v1;
- all cryptographic random identifiers/nonces use the platform CSPRNG and must fail closed if randomness is unavailable.

Canonical scalar encodings used inside encrypted metadata payloads:

```text
u8       1 byte
u16      2 bytes, big-endian
u32      4 bytes, big-endian
u64      8 bytes, big-endian
i64      8 bytes, two's-complement big-endian
bytes32  exactly 32 bytes
str16    u16 byte_length + exactly byte_length UTF-8 bytes
str32    u32 byte_length + exactly byte_length UTF-8 bytes
```

`str16`/`str32` lengths are byte lengths, not Unicode scalar counts.

---

# 3. Fixed version and algorithm identifiers

The first encrypted managed repository format is:

```text
REPOSITORY_FORMAT_VERSION = 3
REPOSITORY_HEADER_VERSION = 1
KEYSET_VERSION             = 1
BACKUP_OBJECT_VERSION      = 1
INDEX_PAYLOAD_VERSION      = 1
MANIFEST_PAYLOAD_VERSION   = 1
```

Cryptographic identifiers:

```text
AEGIS_256_FULL_TAG = 1   # 256-bit key, 256-bit nonce, 256-bit tag
AES_256_GCM        = 2   # Android Keystore envelope only, 256-bit key, 96-bit IV, 128-bit tag
```

Protector identifiers:

```text
APPLE_KEYCHAIN_ROOT_V1  = 1
ANDROID_KEYSTORE_ROOT_V1 = 2
```

Object-class identifiers:

```text
REPOSITORY_INDEX   = 1
SNAPSHOT_MANIFEST  = 2
SNAPSHOT_DATABASE  = 3
ATTACHMENT_BLOB    = 4
REPOSITORY_KEYSET  = 5
```

Unknown versions, algorithms, protector IDs or object classes are rejected. There is no compatibility parser that guesses an older representation.

---

# 4. Canonical identities

## 4.1 Repository ID

`repository_id` is 32 random bytes generated once when a fresh v3 repository is created.

Filesystem/text rendering is exactly 64 lowercase hexadecimal ASCII characters.

## 4.2 Snapshot ID

`snapshot_id` is 32 fresh random bytes per snapshot.

Filesystem rendering is exactly 64 lowercase hexadecimal ASCII characters.

The old timestamp/counter `auto_*` / `safety_*` snapshot IDs are not valid in v3.

## 4.3 Library binding

The raw Rustnote `library_id` is not stored in the plaintext repository header.

The 32-byte `library_binding` is:

```text
SHA-256(
    ASCII("rustnote-backup-library-binding-v1")
    || u32_be(len(library_id_utf8))
    || library_id_utf8
)
```

The actual `library_id` remains available inside encrypted metadata where useful for cross-checking.

## 4.4 Opaque attachment object ID

For an attachment plaintext byte stream:

```text
plaintext_digest = SHA-256(plaintext_bytes)
opaque_blob_id   = HMAC-SHA-256(object_id_hmac_key, plaintext_digest)
```

The filename uses only `opaque_blob_id`; the raw digest is retained only inside the encrypted snapshot manifest.

The same repository/keyset therefore deduplicates identical plaintext attachments, while two repositories produce unrelated object IDs.

---

# 5. Repository layout and filenames

The only canonical v3 layout is:

```text
<Rustnote backup parent>/
├── .rustnote-backup-repository-owner
└── Rustnote Repository/
    ├── repository.header
    ├── index.rnbi
    ├── snapshots/<64-lowercase-hex-snapshot-id>.rnbd
    ├── manifests/<64-lowercase-hex-snapshot-id>.rnbm
    ├── blobs/<64-lowercase-hex-opaque-blob-id>.rnbb
    ├── .staging/
    └── quarantine/
```

Final payload namespaces accept no alternate extension/casing/name grammar.

`.staging` and `quarantine` may use random implementation-private filenames, but every artifact inside the repository tree must still be ciphertext or bootstrap metadata. No plaintext SQLite/database/manifest/blob is ever allowed there.

The external `.rustnote-backup-repository-owner` remains the existing durable ownership marker and is not part of the cryptographic repository format.

---

# 6. `repository.header` exact encoding

`repository.header` is the only bootstrap file that is not itself encrypted as a repository object. Its wrapped keyset remains authenticated-encrypted.

The fixed prefix is exactly **92 bytes**:

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | magic = ASCII `RNBHDR01` |
| 8 | 2 | repository format = `3` |
| 10 | 2 | header version = `1` |
| 12 | 1 | protector kind |
| 13 | 1 | wrap algorithm |
| 14 | 2 | reserved = `0` |
| 16 | 32 | `repository_id` |
| 48 | 32 | `library_binding` |
| 80 | 2 | keyset version = `1` |
| 82 | 2 | protector key generation/slot = `1` |
| 84 | 4 | `envelope_len` |
| 88 | 4 | reserved = `0` |

Immediately after byte 92 is exactly `envelope_len` bytes of platform envelope. No trailing bytes are permitted.

Global limits:

```text
1 <= envelope_len <= 4096
file_len == 92 + envelope_len
```

The tuple `(protector kind, wrap algorithm)` has exactly two v1 legal combinations:

```text
(APPLE_KEYCHAIN_ROOT_V1, AEGIS_256_FULL_TAG)
(ANDROID_KEYSTORE_ROOT_V1, AES_256_GCM)
```

Any other combination is invalid.

## 6.1 Key-envelope AAD

Both platform envelope schemes authenticate exactly:

```text
ASCII("rustnote-backup-key-envelope-aad-v1")
|| repository_header_fixed_prefix_92_bytes
```

The prefix therefore authenticates repository format/version, protector identity, repository identity, library binding, keyset version, protector slot and envelope length.

After successful unsealing, the duplicated repository/library identities inside the keyset plaintext must also match the prefix.

---

# 7. Repository keyset plaintext — exact 204 bytes

Before OS-root wrapping, `RepositoryKeysetV1` is exactly:

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | magic = ASCII `RNBKEY01` |
| 8 | 2 | keyset version = `1` |
| 10 | 2 | reserved = `0` |
| 12 | 32 | `repository_id` |
| 44 | 32 | `library_binding` |
| 76 | 32 | `snapshot_aead_key` |
| 108 | 32 | `blob_aead_key` |
| 140 | 32 | `metadata_aead_key` |
| 172 | 32 | `object_id_hmac_key` |

No trailing bytes/padding are allowed.

All four keys are independently CSPRNG-generated 256-bit values. They are not derived from one another.

---

# 8. Platform key-envelope encodings

## 8.1 macOS / Apple Keychain root

The Keychain stores the 256-bit `DeviceBackupRootKey` bytes under the 2b.43.1 accessibility profile.

Envelope bytes are exactly **268 bytes**:

```text
[0..32)    random AEGIS-256 nonce
[32..236)  204-byte ciphertext of RepositoryKeysetV1
[236..268) 32-byte AEGIS-256 authentication tag
```

AEGIS uses the full 256-bit tag. The AAD is §6.1 exactly.

`envelope_len` must equal `268` for this protector.

## 8.2 Android Keystore root

Android Keystore holds the non-exportable AES-256-GCM wrapping key.

Envelope bytes are exactly **232 bytes**:

```text
[0..12)    random 96-bit GCM IV
[12..216)  204-byte ciphertext of RepositoryKeysetV1
[216..232) 16-byte GCM authentication tag
```

The AAD is §6.1 exactly.

`envelope_len` must equal `232` for this protector.

No platform may reinterpret or silently transcode another platform's envelope format.

---

# 9. Small encrypted object container

`index.rnbi` and snapshot manifests are bounded encrypted in-memory objects using `metadata_aead_key`.

The fixed header is exactly **116 bytes**:

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | class magic |
| 8 | 2 | object version = `1` |
| 10 | 1 | algorithm = `AEGIS_256_FULL_TAG` |
| 11 | 1 | flags = `0` |
| 12 | 32 | `repository_id` |
| 44 | 32 | `object_id` |
| 76 | 8 | plaintext length |
| 84 | 32 | random AEGIS-256 nonce |

Class magic:

```text
RepositoryIndex  = ASCII("RNBIDX01")
SnapshotManifest = ASCII("RNBMAN01")
```

On-disk bytes are exactly:

```text
116-byte header
|| plaintext_len bytes ciphertext
|| 32-byte AEGIS tag
```

Therefore:

```text
file_len = 116 + plaintext_len + 32
```

No trailing bytes are allowed.

Class bounds:

```text
RepositoryIndex plaintext_len <= 4 MiB
SnapshotManifest plaintext_len <= 16 MiB
```

`index.rnbi` receives a fresh random 32-byte `object_id` on every atomic rewrite.

A manifest's `object_id` is exactly its `snapshot_id`.

## 9.1 Small-object AAD

AAD is exactly:

```text
ASCII("rustnote-backup-small-object-aad-v1")
|| library_binding[32]
|| object_class_u8
|| header[116]
```

The class byte is `1` for index and `2` for manifest.

Because class magic, class byte, repository identity, object identity, length and nonce are all authenticated, moving ciphertext between repositories/classes/IDs cannot produce an accepted object.

---

# 10. Bulk encrypted object container

SQLite snapshot databases and attachment blobs use independently authenticated fixed-size records and bounded memory.

The fixed header is exactly **112 bytes**:

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | class magic |
| 8 | 2 | object version = `1` |
| 10 | 1 | algorithm = `AEGIS_256_FULL_TAG` |
| 11 | 1 | flags = `0` |
| 12 | 32 | `repository_id` |
| 44 | 32 | `object_id` |
| 76 | 4 | chunk plaintext bytes = `1,048,576` |
| 80 | 8 | total plaintext length |
| 88 | 24 | random nonce prefix |

Class magic:

```text
SnapshotDatabase = ASCII("RNBSNP01")
AttachmentBlob   = ASCII("RNBBLB01")
```

Canonical v1 chunk size is exactly **1 MiB**. A v1 reader rejects any other value; the field exists so a future object version can evolve without ambiguous parsing.

Plaintext length rules:

```text
SnapshotDatabase: 1 .. 1 TiB
AttachmentBlob:   0 .. 1 TiB
```

## 10.1 Nonces

Each object gets a fresh random 24-byte `nonce_prefix`.

The 32-byte AEGIS nonce is:

```text
nonce_prefix[24] || u64_be(counter)
```

Counter values:

```text
header authenticator = 0xffffffffffffffff
chunk 0              = 0
chunk 1              = 1
...
```

Given the v1 size bound and 1 MiB chunks, no valid object can approach the reserved header counter.

## 10.2 Header authentication

Immediately after the 112-byte header is a 32-byte AEGIS tag authenticating an empty plaintext under the header nonce.

Header AAD is exactly:

```text
ASCII("rustnote-backup-bulk-header-aad-v1")
|| library_binding[32]
|| object_class_u8
|| header[112]
```

The header tag must authenticate before payload allocation/decryption begins.

## 10.3 Chunk records

For each chunk `i`, the ciphertext length equals that chunk's plaintext length and is followed immediately by one 32-byte tag.

Chunk AAD is exactly:

```text
ASCII("rustnote-backup-bulk-chunk-aad-v1")
|| library_binding[32]
|| object_class_u8
|| header[112]
|| u64_be(chunk_index)
|| u32_be(chunk_plaintext_len)
```

Bulk keys:

```text
SnapshotDatabase → snapshot_aead_key
AttachmentBlob   → blob_aead_key
```

A chunk is written to a plaintext Restore/output stage **only after that chunk authenticates**. A later chunk failure still requires the integration layer to discard or retain cleanup ownership of the already-written authenticated prefix.

## 10.4 Exact bulk file length

Let:

```text
C = ceil(plaintext_len / 1,048,576)   # zero when plaintext_len == 0
```

Then:

```text
file_len = 112 + 32 + plaintext_len + (C * 32)
```

Any shorter or longer file is invalid, including trailing data.

---

# 11. Repository index plaintext schema

After decrypting `index.rnbi`, the plaintext is exactly `RepositoryIndexPayloadV1`:

```text
8 bytes   magic = ASCII("RNBIP001")
u16       payload version = 1
u16       reserved = 0
str16     library_id                  # 16..128 bytes under current library contract
u8        has_last_attempt            # 0 or 1
7 bytes   reserved = 0
i64       last_attempt_at             # must be 0 when has_last_attempt == 0
str32     last_result                 # <= 4096 bytes UTF-8
u32       snapshot_count              # hard parser limit <= 1024; current writer policy <= 10
repeated snapshot_count times:
    bytes32 snapshot_id
    u8      kind                      # 1=Automatic, 2=Safety
    7 bytes reserved = 0
    i64     created_at
    u64     database_bytes
    u32     blob_count
    u32     reserved = 0
```

Snapshot entries are encoded canonically sorted by:

```text
(created_at ascending, snapshot_id bytes lexicographically ascending)
```

Duplicate `snapshot_id` values are invalid. The current retention policy still writes at most 7 automatic + 3 safety snapshots; the larger 1024 parser ceiling is only a hard untrusted-input bound and does not change retention behavior.

The index is **operational/reconstructable metadata, not snapshot authority**. If index authentication/parsing fails while the keyset is available, Rustnote may rebuild it from successfully authenticated manifests. Key unavailability alone never authorizes rebuild/destruction.

---

# 12. Snapshot manifest plaintext schema

After decrypting `<snapshot_id>.rnbm`, plaintext is exactly `SnapshotManifestPayloadV1`:

```text
8 bytes   magic = ASCII("RNBMP001")
u16       payload version = 1
u16       reserved = 0
bytes32   snapshot_id                 # must equal filename + outer object_id
u8        kind                        # 1=Automatic, 2=Safety
7 bytes   reserved = 0
i64       created_at
str16     build_identifier            # 1..256 UTF-8 bytes
bytes32   database_sha256             # digest of decrypted SQLite snapshot bytes
u64       database_bytes              # > 0
u64       notebook_count
u64       note_count
u64       attachment_count
u64       revision_count
u32       blob_count
u32       reserved = 0
repeated blob_count times:
    bytes32 opaque_blob_id
    bytes32 plaintext_sha256
    u64     plaintext_bytes
```

Blob entries are encoded strictly sorted by `opaque_blob_id` lexicographically ascending. Duplicate opaque IDs or duplicate plaintext digests are invalid. Canonical writers emit each referenced blob exactly once.

For every blob entry:

```text
opaque_blob_id == HMAC-SHA-256(object_id_hmac_key, plaintext_sha256)
```

The manifest is the **snapshot publication authority**. A snapshot is not committed until its authenticated manifest is durably published.

---

# 13. Digest semantics

`database_sha256` and `plaintext_sha256` always hash the plaintext payload bytes, not ciphertext/container bytes.

Their purposes are:

- deterministic dedup identity input for attachments;
- end-to-end verification after successful AEAD decryption;
- detection of implementation/storage mismatches independent from container framing.

AEAD authentication remains mandatory; a matching SHA-256 never substitutes for authentication.

Ciphertext digests are not stored in the v1 manifest because AEAD plus exact-length checks already authenticate container contents.

---

# 14. Atomic publication and commit ordering

## 14.1 Attachment blob publication

For a new opaque blob:

```text
stream source → encrypted file in repository .staging
↓
flush + sync ciphertext file
↓
validate fixed header / repository ID / object ID / exact encoded length
↓
atomic rename to blobs/<opaque_blob_id>.rnbb
↓
sync blobs directory
```

If the final immutable blob already exists, normal automatic backup may reuse it only after:

```text
filename ID matches expected opaque_blob_id
class/version/repository/object header fields match
header authenticator verifies with blob key
exact encoded length matches expected plaintext size
```

A normal backup does not have to decrypt every existing bulk chunk merely to reuse an immutable deduplicated blob. Full repository inspection, Restore, and safety-backup verification authenticate all referenced chunks and verify plaintext digests.

## 14.2 Snapshot database publication

A newly produced process-private SQLite snapshot is streamed to an encrypted `.staging` object and never copied plaintext into the repository.

After encryption:

```text
sync encrypted staged object
↓
atomic rename to snapshots/<snapshot_id>.rnbd
↓
sync snapshots directory
```

## 14.3 Manifest is the snapshot commit point

All referenced encrypted blobs and the encrypted snapshot database must already be durably published before the manifest.

Then:

```text
build canonical manifest plaintext
↓
encrypt manifest to temporary ciphertext
↓
sync
↓
atomic rename to manifests/<snapshot_id>.rnbm
↓
sync manifests directory
```

Only after that rename+directory sync is the snapshot committed.

## 14.4 Index publication

`index.rnbi` is written last through atomic replace and directory sync.

Crash after manifest commit but before index replacement is recoverable by scanning/decrypting manifests and rebuilding the index.

---

# 15. Deletion/pruning semantics

Pruning is metadata-authority first:

```text
authenticate all manifests needed for the decision
↓
atomically remove/retire target manifest
↓
sync manifests directory
↓
remove its snapshot database ciphertext
↓
garbage-collect encrypted blobs no longer referenced by any authenticated manifest
↓
rewrite index
```

If cleanup is interrupted, leftover database/blob ciphertext is safe residue and can be reconciled later. It must never be mistaken for a committed snapshot without a valid manifest.

No pruning or garbage collection is allowed when the repository keyset cannot be opened.

---

# 16. Crash recovery rules

With an available/authenticated keyset:

- ciphertext in `.staging` is never committed state and may be removed after structural/authentication checks appropriate to recovery;
- a snapshot database with no authenticated manifest is uncommitted and may be removed/quarantined;
- a manifest whose database object is missing/invalid is an integrity failure, not a usable snapshot;
- unreferenced encrypted blobs may be garbage-collected only after scanning all authenticated manifests;
- a missing/corrupt index is reconstructable from authenticated manifests;
- no crash-recovery path decrypts bulk payload merely to enumerate repository state unless verification/Restore requires it.

Without an available keyset, repository bytes are preserved unchanged except for non-destructive read-only bootstrap inspection.

---

# 17. Parser and resource limits

Before allocating from untrusted lengths/counts, readers enforce:

```text
repository.header envelope_len <= 4096
index plaintext <= 4 MiB
manifest plaintext <= 16 MiB
last_result <= 4096 bytes
library_id <= 128 bytes
build_identifier <= 256 bytes
snapshot_count <= 1024 hard parser limit (current writer policy remains <= 10)
bulk plaintext <= 1 TiB
bulk chunk size == 1 MiB
bulk chunk count arithmetic checked for overflow
```

Manifest `blob_count` is additionally bounded by whether the complete encoded entries fit within the authenticated 16 MiB plaintext limit.

Every parser rejects trailing bytes after consuming the canonical schema.

No size/count from repository bytes may be converted to `usize`, multiplied, added, or used to allocate before checked bounds/overflow validation.

---

# 18. Object/path cross-checking

Authentication is necessary but not the only acceptance check.

After authentication:

```text
repository.header repository_id == opened keyset repository_id
repository.header library_binding == current library binding
all object headers repository_id == opened repository
manifest outer object_id == manifest plaintext snapshot_id
manifest snapshot_id == filename snapshot ID
snapshot database object_id == manifest snapshot_id
blob outer object_id == blob filename opaque ID
manifest opaque_blob_id == HMAC(object-id key, manifest plaintext digest)
manifest database_bytes == decrypted snapshot DB length
manifest blob byte lengths == decrypted blob lengths
manifest SHA-256 values == decrypted plaintext digests
```

A failure is integrity failure. No object is adopted under a different identity.

---

# 19. Domain-separation summary

The v1 backup crypto domains are deliberately disjoint:

```text
Device root wrapping
  AAD domain: rustnote-backup-key-envelope-aad-v1

Small metadata object
  AAD domain: rustnote-backup-small-object-aad-v1
  key: metadata_aead_key

Bulk header
  AAD domain: rustnote-backup-bulk-header-aad-v1

Bulk chunk
  AAD domain: rustnote-backup-bulk-chunk-aad-v1

Snapshot database bulk key
  key: snapshot_aead_key

Attachment blob bulk key
  key: blob_aead_key

Opaque dedup identity
  HMAC key: object_id_hmac_key
```

Vault note keys/domains are never accepted as backup keys/domains and vice versa.

---

# 20. Canonical structural test vectors

These vectors fix non-secret identity encodings independently of any later Rust implementation.

## 20.1 Library binding vector

Input:

```text
library_id UTF-8 = "library_example_001"
```

The hashed preimage is exactly:

```text
ASCII("rustnote-backup-library-binding-v1")
|| 00000013
|| ASCII("library_example_001")
```

Expected SHA-256:

```text
f0363a8464e6a722237fbdfdb98a0f869ef60c0bce8a5049c82267525ff54fdb
```

The 2b.43.2 QA validator independently recomputes this value; later implementation tests must reproduce it.

## 20.2 Opaque blob ID vector

Input:

```text
object_id_hmac_key = 00 01 02 ... 1f
plaintext_digest   = SHA-256(ASCII("abc"))
opaque_blob_id     = HMAC-SHA-256(key, plaintext_digest)
```

Expected values:

```text
SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
opaque_blob_id = 910ec338f64892dfc0288e343162e996b71c39c67dd1cad13923f2d67a79cff6
```

The 2b.43.2 QA validator independently recomputes these values; later implementation tests must reproduce them.

Ciphertext test vectors are intentionally deferred until the backup crypto implementation exists in `rustnote-vault`; that implementation must then add deterministic test-only nonce/IV fixtures without exposing deterministic production nonce APIs.

---

# 21. Rollback/freshness scope

v3 authenticates object contents and identities but does not claim a trusted monotonic anti-rollback counter against an attacker who replaces the **entire** repository with an older, previously valid encrypted repository state.

Such rollback can remove newer backup history but does not expose plaintext or allow forged ciphertext to authenticate.

Adding secure monotonic freshness would require a separate trusted-state design and is not justified for the local managed-backup threat model in 2b.43.

---

# 22. Explicitly forbidden representations

The v3 implementation must never publish:

```text
repository.json
plaintext manifest JSON
plaintext SQLite snapshot in repository/.staging
raw SHA-256 blob filename
base64/hex key material in metadata
serde/bincode/native-ABI-dependent metadata encoding
variable v1 bulk chunk size
truncated-tag AEGIS
unauthenticated encryption
one shared bulk key for snapshots + blobs + metadata
object acceptance based only on extension/filename
silent parser fallback to v2
```

No implementation may serialize Rust structs directly with a representation whose byte encoding can drift with compiler/library version. The binary grammar in this contract is the format.

---

# 23. Relationship to existing `rustnote-vault`

The existing Vault crate already provides the reviewed cryptographic pattern required here:

```text
AEGIS-256
32-byte key
32-byte nonce
32-byte full tag
random nonces
header authentication
1 MiB bounded streaming
per-chunk authentication
zeroizing key/plaintext buffers
```

2b.43 implementation should reuse/refactor those **primitives**, not reuse Vault-note file magic or Vault note object domains.

The correct architectural outcome is one cryptographic crate with distinct typed domains/formats:

```text
rustnote-vault
├── Vault note/object formats
└── Managed backup formats
```

Storage remains responsible for repository lifecycle; cryptographic byte encoding/authentication belongs behind the security/crypto boundary.

---

# 24. Mandatory implementation tests derived from this format

Later implementation must include executable tests for at least:

```text
exact repository.header byte offsets and length
reserved bytes rejected
unknown algorithm/protector/class rejected
wrong envelope length rejected
keyset exact 204-byte round trip
wrong repository_id fails envelope/object authentication
wrong library binding fails envelope/object authentication
wrong object class fails authentication
small-object truncation/trailing bytes rejected
small-object plaintext bounds enforced
bulk header tag corruption rejected before payload output
bulk chunk corruption rejected before that chunk is output
bulk truncation/trailing bytes rejected
zero-byte attachment blob round trip
1-byte / 1MiB / 1MiB+1 boundary round trips
1 TiB length metadata rejected/accepted only within arithmetic bounds without allocation
snapshot DB zero length rejected
manifest/filename/object-id mismatch rejected
blob HMAC identity mismatch rejected
noncanonical manifest ordering rejected
noncanonical index ordering rejected
index can rebuild from authenticated manifests
manifest commit before index crash is recoverable
unmanifested snapshot DB is never treated as committed
staging contains ciphertext only
same plaintext attachment → same opaque ID in one repository
same plaintext attachment → unrelated opaque ID in independent repositories
known SHA-256 is not used as filename
all ciphertext nonce/IV generation is random in production
```

---

# 25. Definition of Done for 2b.43.2

2b.43.2 is complete when all of the following are true:

1. repository/header/keyset bytes have exact offsets and sizes;
2. Apple and Android wrapped-key envelope bytes are exact;
3. repository/library/object identity encoding is exact;
4. metadata/bulk AAD byte construction is exact;
5. nonce/IV and streaming-counter rules are exact;
6. small-object container bytes are exact;
7. bulk-object container bytes/chunking are exact;
8. index plaintext schema is canonical and bounded;
9. manifest plaintext schema is canonical and bounded;
10. dedup object-ID derivation and filename grammar are exact;
11. atomic publication/manifest commit ordering is explicit;
12. pruning/crash-recovery authority is explicit;
13. parser/resource limits and trailing-byte rejection are explicit;
14. cross-object/path verification rules are explicit;
15. rollback/freshness scope is explicitly bounded;
16. the format does not rely on serde/native ABI/compiler-specific layout;
17. implementation tests required by the format are enumerated;
18. no production backup/storage/crypto behavior has been implemented or modified in this pass.

Only after all eighteen are true may 2b.43.3 begin.

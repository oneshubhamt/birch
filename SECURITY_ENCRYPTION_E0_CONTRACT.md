# Rustnote encryption E0 contract — build 2a.1

## Purpose

E0 fixes Rustnote's privacy target and creates a reproducible measurement harness before any production encryption code is selected or introduced.

This pass does **not** encrypt notes, notebooks, SQLite, attachments, backups, or caches. It does **not** add cryptography to the shipping Rustnote dependency graph.

## 2b.12 product decision override — AEGIS-256 fixed profile

The comparative cipher/KDF benchmark entry criterion below is **superseded by an explicit product decision** as of build 2b.12. Rustnote v1 uses **AEGIS-256** as its sole production bulk-encryption algorithm. Comparative AEGIS/XChaCha/AES benchmarking remains an optional engineering diagnostic and is no longer a security-release prerequisite.

The fixed v1 cryptographic profile is:

- AEGIS-256 with a 256-bit key, 256-bit nonce and full 256-bit authentication tag;
- OS-provided cryptographic randomness for keys, object nonces, salts and blob nonce prefixes;
- random 256-bit NoteKeys; passwords only wrap NoteKeys and never encrypt note/attachment payloads directly;
- Argon2id v=19 with 64 MiB memory, 3 passes, 4 lanes, 128-bit random salt and a 256-bit derived wrapping key;
- `RNVBLB01` protected blobs use a 192-bit random per-blob nonce prefix plus a 64-bit record counter, yielding a unique 256-bit nonce per authenticated record;
- all format headers, domains, object identities and record metadata remain authenticated as associated data.

The Argon2id profile is RFC 9106's second recommended configuration for memory-constrained environments. The v1 parser also caps untrusted envelope resource requests at exactly this profile so a corrupted envelope cannot request more memory/CPU than Rustnote itself intentionally uses. Future stronger KDF profiles require an explicit policy/version change.

This override changes the **selection evidence requirement**, not the E0 threat model, plaintext-derivative boundary, algorithm agility requirement, or E1–E3 security invariants.

## Product boundary

Rustnote's own cryptography will protect deliberately encrypted **notes** and **notebooks**, including their sensitive dependent data. Whole-database encryption is not an E0 objective. Native app access control (Touch ID, Windows Hello, Android biometrics or equivalent) is a separate platform-access feature and is not a substitute for content encryption.

## Threat model

Rustnote protected content is intended to remain confidential and tamper-detectable against an adversary who obtains one or more **locked, at-rest** artifacts, including:

- the SQLite database;
- attachment/blob storage;
- automatic backups and portable backups;
- emergency/autosave recovery files;
- search indexes and derived text;
- preview/rendition/media compatibility caches;
- exported or synchronized encrypted Rustnote objects in future cloud storage;
- historical/revision material that belongs to the protected scope.

The design must assume the attacker can copy those artifacts and perform unlimited offline analysis and password guessing subject only to the cost imposed by the password KDF.

The design must also authenticate encrypted data so unauthorized modification is detected before plaintext is accepted.

## Explicit non-goals / limits

Content encryption cannot promise confidentiality after protected content has been legitimately unlocked on a device whose operating system, process memory, kernel, input stack, screen capture path, or hardware is already compromised. It also cannot make a weak human password equivalent to a uniformly random key.

E0 therefore does not claim protection against:

- malware with access to Rustnote's unlocked process or plaintext memory;
- a compromised operating-system kernel or hostile hardware/firmware;
- keylogging or screen capture while the content is unlocked;
- coercion of the user to reveal a password;
- denial of service or deletion of encrypted data when no independent backup exists.

These limits must never be obscured by UI or marketing language.

## Cryptographic roles

### Data encryption

The historical E0 candidate set was:

1. **AEGIS-256** — 256-bit key, 256-bit nonce, with Rustnote evaluating the 256-bit authentication-tag form.
2. **XChaCha20-Poly1305** — 256-bit key, 192-bit nonce, 128-bit authentication tag.
3. **AES-256-GCM** — 256-bit key, 96-bit nonce, 128-bit authentication tag; retained as the mature interoperability/reference baseline.

**Current governing decision (2b.12): AEGIS-256 is selected for Rustnote encrypted format v1 with the full 256-bit authentication tag.** The benchmark harness remains available for diagnostics, but comparative cipher benchmarking no longer gates the selected production algorithm.

### Password hardening

Human passwords must not directly encrypt note data. Rustnote will generate random data-encryption keys. A password-derived key will only unlock/wrap those random keys.

**Argon2id** is the production password KDF. Rustnote v1 fixes the RFC 9106 second-recommended profile: 64 MiB memory, 3 passes, 4 lanes, 128-bit random salt and a 256-bit derived wrapping key. Parameters remain versioned and stored with the encrypted envelope so a future explicit profile can raise the cost without re-encrypting note/attachment payloads.

### Key hierarchy

E1 must prefer small, explicit envelope encryption rather than re-encrypting large data when a password changes. At minimum:

- each protected scope receives random cryptographic key material;
- password-derived material protects key material rather than becoming the long-lived content key;
- key separation/context binding is mandatory between logically different uses;
- nonce uniqueness rules are part of the format contract, not an informal caller responsibility;
- secret key material must have a bounded lifetime in memory and be zeroized where practical and meaningful.

## Privacy boundary: plaintext derivatives

A note/notebook is not considered encrypted if equivalent sensitive plaintext survives elsewhere.

Before E2/E3 can be considered complete, the protected-content path must account for at least:

- `notes.title`, `notes.body`, `notes.editor_state`, `notes.plain_text`, and sensitive title styling/metadata;
- `notes_fts` title/body/tag/attachment-name material;
- note tags and other revealing relationships where included in the protected scope;
- `note_revisions.snapshot_json` and revision attachment references where revealing;
- emergency journal drafts;
- attachment filenames, metadata and bytes;
- PDF annotations and extracted/searchable PDF text;
- image renditions and thumbnails;
- media compatibility outputs;
- native handoff/export staging;
- automatic backup SQLite snapshots and backup attachment blobs;
- portability/export artifacts when the user requests an encrypted-preserving export;
- future cloud/sync objects.

Ordinary unencrypted Rustnote content may continue using the current persistent SQLite/FTS5 paths.

## Search contract

Locked protected content must not be represented as plaintext in persistent FTS5 or any other persistent search cache.

A later search pass may make **currently unlocked** protected content searchable through an in-memory/session-only index. Locking the scope must remove that decrypted search material. Whole-database encryption is not required for this model.

Global PDF search is a separate search improvement: ordinary PDFs may have extracted page text in persistent search; protected PDFs may expose extracted text only while their scope is unlocked and only through non-persistent search state.

## Attachment contract

Rustnote's current ordinary attachment store is content-addressed by SHA-256. A protected note must not encrypt or mutate a shared ordinary blob in place.

E1/E2 must introduce an explicit protected-blob format/identity. Large attachments must use independently authenticated records/chunks so encryption and decryption remain bounded-memory and no unauthenticated plaintext is exposed to consumers.

## Integrity and blockchain-inspired ideas

AEAD authentication is mandatory and is the primary integrity protection for each encrypted object.

Rustnote may later add a **hash-linked revision history** and device signatures to make historical deletion/reordering/forgery detectable. This is the useful blockchain-inspired property. Rustnote will not introduce a blockchain, cryptocurrency, mining, consensus network, or public ledger for note protection.

Any later signature scheme must be evaluated independently for post-quantum migration; present-day blockchain signatures are not assumed to be post-quantum secure.

## Quantum-resistance posture

Rustnote will not invent a novel "post-quantum bulk cipher". The bulk-encryption candidates use 256-bit symmetric keys, which are the appropriate class for long-term stored-data protection under current post-quantum guidance.

Post-quantum public-key cryptography (for example ML-KEM or successor standards) is relevant later to cross-device key establishment, not to replacing the symmetric cipher used for every note byte.

The encrypted-object format must be algorithm/version agile so a future Rustnote release can introduce a new approved cipher/KDF and migrate existing protected content.

## Optional E0 benchmark harness (non-gating as of 2b.12)

`tools/e0-crypto-bench` is deliberately isolated from the shipping application workspace and dependency graph. Its runner copies the benchmark into `target/`, resolves the exact direct version pins there, and runs the resulting temporary benchmark with `--locked`, so E0 never mutates Rustnote's production lockfile or source tree.

It measures:

- AEGIS-256;
- XChaCha20-Poly1305;
- AES-256-GCM;
- Argon2id unlock-cost probes;
- representative 4 KiB note, 1 MiB document, and 16 MiB attachment payloads.

The harness includes allocation cost and is intended for **Rustnote candidate comparison on the same machine**, not publication-quality cross-project benchmarking.

Results from CI/container hardware must never be presented as device-performance evidence. As of 2b.12, however, benchmark output is optional and does not determine the already-fixed AEGIS-256/Argon2id v1 profile.

## E1 entry criteria (current 2b.12 interpretation)

E1 is governed by:

1. E0 threat-model/privacy validation passes.
2. The recorded product decision fixes AEGIS-256 for Rustnote encrypted format v1.
3. The fixed Argon2id v1 profile is 64 MiB / 3 passes / 4 lanes with a 128-bit salt and 256-bit derived key.
4. Benchmarking is optional engineering evidence, not a release prerequisite.
5. The encrypted-object envelope defines version, algorithm, KDF parameters, salts/nonces, associated-data binding, and migration semantics before production ciphertext is created.

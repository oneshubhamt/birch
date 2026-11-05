# Rustnote encryption E1 contract — build 2a.2

## Objective

E1 establishes one production-grade cryptographic boundary without changing how Rustnote stores or displays any real note yet.

The canonical implementation is the isolated internal crate:

```text
crates/rustnote-vault
```

The application crate only re-exports its public capability through `src/security`.

## Fixed E1 decisions

- SQLite remains ordinary SQLite. No SQLCipher and no database schema migration.
- FTS5, editor code, note persistence, attachment persistence, PDF.js, backups, and UI behaviour are unchanged in E1.
- Rustnote encrypted format v1 uses **AEGIS-256** with a 256-bit key, 256-bit nonce, and 256-bit authentication tag.
- Human passwords never encrypt note/attachment data directly.
- Rustnote generates random 256-bit scope keys from the operating-system random source.
- Passwords are processed with **Argon2id** only to derive a temporary key-encryption key that wraps/unwraps a random Rustnote scope key.
- The serialized Argon2 parameters are stored in each key envelope so parameters can be strengthened/tuned later without changing old ciphertext.
- Owned secret keys and derived password keys are zeroized on drop where Rust permits reliable zeroization.
- Encryption formats are explicitly versioned and algorithm-tagged. AEGIS-256 is v1, not an irreversible architecture lock-in.
- Cryptographic context (domain + object ID) is authenticated so ciphertext for one note/notebook/attachment cannot be silently transplanted into another scope.

## Cryptographic formats

E1 defines three independent opaque formats:

1. `RNVOBJ01` — bounded encrypted objects.
2. `RNVKEY01` — Argon2id password-wrapped random scope keys.
3. `RNVBLB01` — large protected blobs encrypted as independently authenticated chunks.

All format headers are authenticated as associated data. Reserved fields must remain zero. Unsupported versions/algorithms/KDF identifiers fail closed.

## AEGIS-256 policy

AEGIS-256 is the only production bulk cipher in E1.

The vault does not dynamically choose a different cipher per machine. That keeps encrypted Rustnote data deterministic in format and portable between devices.

Future Rustnote versions may add another algorithm identifier and a migration path. Existing v1 data remains self-describing.

## Password / Argon2id policy

The flow is:

```text
password
   ↓
Argon2id + random salt + stored parameters
   ↓
temporary 256-bit wrapping key
   ↓
AEGIS-256 key envelope
   ↓
random Rustnote scope key
```

The random scope key is what later E2/E5 will use for note/notebook payloads.

`Argon2idParams::V1_INTERACTIVE` is the fixed Rustnote v1 profile: 64 MiB, time cost 3, lanes 4, with a 128-bit random salt and 256-bit derived wrapping key. This is RFC 9106's second recommended Argon2id configuration for memory-constrained environments. Parameters remain serialized per envelope so a later explicit profile/version can migrate policy without re-encrypting note/attachment payloads.

As of 2b.12, untrusted key envelopes are hard-capped at the **same v1 resource profile**: 64 MiB memory, time cost 3, and 4 lanes. This prevents corrupted/malicious envelopes from demanding more memory/CPU than Rustnote intentionally uses. Stronger future policies require an explicit policy/version revision rather than silently expanding this denial-of-service surface. The public `Argon2idParams` type exposes only the canonical `V1_INTERACTIVE` value; its raw fields are private, and the obsolete candidate alias has been removed, so application/storage callers cannot casually construct a weaker wrapping profile.

Password policy (minimum length, passphrase guidance, retries, recovery UX) is intentionally outside the crypto crate and belongs to application/UI integration.

## Large protected blob contract

Attachments may be very large, so E1 must never require whole-file buffering.

`RNVBLB01` uses:

- a small authenticated header;
- a random 192-bit nonce prefix;
- a unique 64-bit record number appended to create each AEGIS-256 256-bit nonce;
- independently authenticated chunks;
- default chunk size 1 MiB;
- accepted chunk sizes bounded to 4 KiB–8 MiB;
- authenticated total plaintext length and chunk size.

The header itself has an authentication tag, so an empty protected file is still authenticated.

On decryption, each chunk is authenticated before being written. A later failing chunk can leave an already-authenticated plaintext prefix in the supplied output sink. Therefore E2 integration MUST decrypt to an unpublished temporary file/buffer and publish it only after the entire operation returns success.

## Memory/key handling contract

- `VaultKey` is intentionally non-`Clone`.
- `VaultKey` debug output is always redacted.
- `VaultKey` and derived Argon2id wrapping keys use `zeroize::Zeroizing`.
- Decrypted bounded-object plaintext is returned as a zeroizing owner.
- Streaming buffers are zeroizing owners.
- The vault does not own caller password memory. E2 must collect passwords into an owned zeroizing application buffer and keep them for the shortest practical lifetime.
- Rust cannot guarantee erasure of every historical compiler/runtime copy. The objective is bounded ownership and no avoidable long-lived plaintext/key copies.
- Third-party cryptographic primitives can make short-lived internal key copies while operating. Rustnote does not expose or persist those copies and must not claim perfect process-memory erasure; the contract is to minimize ownership, zeroize Rustnote-owned secret buffers, and avoid long-lived/logged/persistent secret copies.

## Explicit E1 non-goals

E1 does NOT:

- encrypt a real note;
- encrypt a real notebook;
- alter attachment storage;
- alter SQLite;
- alter FTS5/search;
- alter revision history;
- alter autosave/emergency journals;
- alter backups/exports/caches/previews;
- add password or lock UI;
- add native biometric app locking;
- add cloud sync/PWA support;
- add blockchain/hash-chain revision integrity;
- add post-quantum device key exchange.

Those remain later passes.

## Blockchain-inspired integrity

The E0 decision remains unchanged: later Rustnote may hash-link synchronized/historical revisions and optionally sign revisions with per-device keys. That is an integrity/history feature, not part of E1 confidentiality and not a blockchain.

## E2 entry criteria

E2 may begin only after:

- the `rustnote-vault` crate builds with the locked dependency graph;
- all vault unit tests pass;
- wrong key/password/context and modified ciphertext are rejected;
- protected-blob truncation/trailing-data/tamper tests pass;
- `cargo clippy` is clean for the new crate;
- the production app still builds with no note/schema/UI/search behaviour changes.

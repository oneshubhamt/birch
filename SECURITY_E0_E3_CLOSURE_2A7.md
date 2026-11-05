# Rustnote 2a.7 — Vault Revocation & Final E0–E3 Security Closure

Build: **0.12.5 / 2a.7**

## Purpose

2a.7 closes the residual issues found by the de novo audit of build 2a.6. It does not redesign the vault format, SQLite model, editor, PDF.js, or ordinary attachment architecture. The v1 cryptographic format remains AEGIS-256 with a 256-bit key, 256-bit nonce and the full 256-bit / 32-byte authentication tag; Argon2id protects random per-note keys.

The E0 real-device performance acceptance is treated as externally accepted for this closure, as directed by the project owner. No benchmark result is fabricated or embedded by this pass.

## 1. Atomic Lock revocation contract

Each unlocked protected note has one process-local vault session containing its key, generation, revocation state, active protected-read count, and registered decrypted materializations.

Every Rustnote-owned operation that may use protected plaintext outside the immediate key mutex must hold a `VaultReadPermit` for its complete plaintext-use lifetime. The permit is propagated through the opaque `AttachmentTransportResource`; application, media and PDF transport must retain that resource/lease rather than extracting an unguarded protected `PathBuf`.

Lock is the exclusive revocation boundary:

1. publish `revoking = true` under the session mutex;
2. refuse all new permits/resources immediately;
3. wait for already-issued permits to drain;
4. remove the session so the `VaultKey` drops/zeroizes;
5. privacy-remove all registered decrypted materializations;
6. only then return success to the caller.

A materialization that finishes concurrently with revocation may not register itself after revocation; its just-created plaintext file must be privacy-removed before returning an error.

This makes a completed Lock a strong Rustnote-internal guarantee: no protected attachment/PDF/media operation from the revoked session remains active, and no Rustnote-owned decrypted materialization from that session remains registered on disk.

## 2. PDF/media/application lease propagation

The storage transport resource is the single plaintext-lifetime capability for protected attachments. PDF custom-protocol requests retain it inside `ManagedAttachment`; media transport retains it for the request/stream scope; application file commands and runtime probes retain the full resource for the complete native/probe operation.

No application capability may convert a protected transport resource into a bare path and drop the lease early.

Cached PDF file handles are performance artifacts, not authorization capabilities. A request must first obtain a fresh current-session transport lease. Lock waits for active leased requests; after Lock, a cached handle cannot be reached through a newly authorized protected request until the note is explicitly unlocked again.

## 3. Sensitive staging contract

Rustnote uses one native privacy abstraction for sensitive staging leaves:

- private directory leaf: Unix mode `0700`;
- private file: Unix mode `0600` at creation;
- non-Unix targets rely on the application/user sandbox/ACL boundary exposed by the platform;
- sensitive directory leaves that resolve to symlinks are rejected.

The abstraction is used for vault runtime materialization, protect/unprotect operation staging, image staging and PDF staging where plaintext may be present.

## 4. Unprotect confidentiality-release contract

Removing encryption necessarily reconstructs plaintext. Rustnote defines the confidentiality-release request as beginning when the user explicitly authorizes **Remove encryption** with the correct password, but it still minimizes exposure before database commit:

- reconstructed plaintext is written only into a private Rustnote-owned operation directory;
- the durable vault-operation journal makes the transition recoverable;
- if the database has not committed unprotection, startup recovery removes unpublished staging;
- after database commit, recovery may publish the authenticated plaintext into ordinary storage.

Therefore an interrupted pre-commit unprotect operation may leave private Rustnote-owned plaintext staging until recovery, but it must not publish that plaintext into ordinary canonical attachment storage while the database still represents the note as protected.

## 5. Protected-payload structural validation contract

AEGIS authenticates bytes, but authenticated malformed historical/imported state must still fail closed. After decryption Rustnote validates one canonical protected payload graph, including:

- note identity and embedded protection state;
- unique current attachment identities and note ownership;
- valid blob hashes/sizes and one-to-one ciphertext mappings;
- attachment source-chain existence and acyclicity;
- unique positive revision numbers/IDs, snapshot checksum/size/format/note identity;
- revision attachment graph validity and exact agreement with durable revision references;
- source/citation ownership and identity uniqueness;
- PDF annotation attachment/page validity, JSON validity and page uniqueness;
- outgoing target note-ID validity and uniqueness.

Invalid protected state is rejected rather than partially restored.

## 6. Metadata and external-egress boundary

Vault v1 protects note content and sensitive note-owned metadata but does not claim complete existence-, timing- or size-hiding. A locked library can still reveal structural facts required by Rustnote, and ciphertext length can reveal approximate plaintext size. Explicit padding/traffic-analysis resistance is outside E0–E3.

When the user deliberately saves, opens or reveals decrypted protected content to another application/location, that external plaintext leaves Rustnote's revocable vault boundary. Lock can revoke Rustnote-owned internal resources; it cannot recall copies already handed to the OS or another application.

## 7. Dependency security contract

The transitive `webbrowser` dependency is locked to **1.2.4**, eliminating the Unix `BROWSER` argument-injection issue affecting versions through 1.2.1. Security closure QA includes `cargo audit` in addition to Cargo check, Clippy and tests.

Dependency updates for this closure are intentionally narrow: only the vulnerable `webbrowser` lock entry is advanced; Rustnote's direct Dioxus 0.7.10 pins remain unchanged.

## 8. Behavioral/adversarial QA contract

Tests must cover at minimum:

- Lock waits for an existing protected read permit;
- revocation refuses new permits before Lock completes;
- late materialization registration is rejected after revocation;
- common attachment transport leases keep Lock waiting while PDF/media-style reads are active;
- PDF `ManagedAttachment` retains the transport lease for the whole request lifetime;
- Lock removes process-temporary plaintext before returning;
- sensitive Unix staging directories/files use 0700/0600 permissions;
- prior E2/E3 crash recovery, autosave barrier, backup ownership and plaintext-residue tests continue to pass.

## Definition of E0–E3 closure

With the project owner's E0 real-device acceptance assumption, E0–E3 are closed only when:

1. E0 threat model/KDF/format policy remains intact;
2. E1 cryptographic isolation and full-tag AEGIS format remain intact;
3. E2 note/attachment encryption and crash-safe transitions remain intact;
4. E3 managed plaintext-residue controls remain intact;
5. Lock is an atomic Rustnote-internal revocation barrier;
6. no protected path escapes application/transport lifetime authority;
7. sensitive plaintext staging is private by construction;
8. protected payload restoration validates its complete structural graph;
9. the vulnerable transitive dependency is absent and `cargo audit` is clean;
10. source/static architecture/security QA and native Cargo/Clippy/test QA pass.

The E0 benchmark measurement itself is not re-run or fabricated by 2a.7; it is an accepted external premise for this closure.

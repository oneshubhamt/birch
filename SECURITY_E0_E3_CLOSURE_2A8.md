# Rustnote 2a.8 — E0–E3 Descriptor-Lifetime Final Closure

Build: **0.12.5 / 2a.8**

## Scope

2a.8 is a narrow closure pass over 2a.7. It does not change the vault wire format, Argon2id policy, SQLite schema, editor, PDF.js runtime, attachment storage model, or protected-payload format. E0 real-device acceptance is treated as resolved by project decision.

The pass closes three residual E3/QA findings from the de novo 2a.7 audit:

1. a cached PDF `Arc<File>` could physically outlive Lock even though new requests were re-authorized;
2. endpoint-level media file-handle lifetime needed deterministic revocation coverage;
3. converter-created protected image renditions needed explicit Unix `0600` normalization.

## 1. PDF descriptor-lifetime contract

PDF read-session caching may retain only non-secret coordination metadata such as attachment identity, expected path, length and last-use time. It must never retain an open `File`, `Arc<File>`, `PdfReadHandle`, memory mapping, or equivalent plaintext-bearing OS capability.

Every PDF data request:

1. obtains a current `ManagedAttachment`, which owns the complete `AttachmentTransportResource` and its `VaultReadPermit`;
2. validates any cached session metadata against the current path;
3. opens a fresh request-scoped `PdfReadHandle` only after authorization;
4. performs the bounded read while the `ManagedAttachment` lease remains alive;
5. drops the file handle before the request returns.

Therefore Lock can wait only for currently authorized PDF requests; after the final request lease drains there is no cached PDF file descriptor that can continue to expose unlinked plaintext.

This also closes the pre-encryption variant of the same problem: ordinary PDF session metadata may survive briefly, but no open ordinary plaintext descriptor is retained by the PDF session cache across Encrypt.

## 2. Media descriptor-lifetime contract

Media transport binds the OS `File` and the protected-note permit into one `LeasedMediaFile`. The permit is acquired before the file is opened and remains owned by the file wrapper for the complete stream/read lifetime.

A completed Lock therefore implies that Rustnote owns no still-usable protected media file descriptor from the revoked session.

## 3. Converter-created private-file contract

`native::private_temp` remains the canonical sensitive staging authority and now also owns `harden_existing_file()` for outputs that an external converter must create itself.

Before Rustnote publishes or serves such an output it must:

- reject symlink/non-regular output;
- normalize the Unix file mode to `0600`;
- retain the existing private-parent-directory boundary (`0700` for sensitive staging roots).

Image rendition output is hardened immediately after the platform converter returns and before metadata validation/publication.

## 4. Behavioral QA contract

The cumulative E0–E3 suite must retain the 2a.6/2a.7 adversarial guarantees and additionally prove:

- the PDF session cache contains metadata only and cannot retain a file descriptor after unlink;
- protected PDF transport resources block Lock while the request authority is alive;
- an opened protected media file itself retains the vault permit and keeps Lock waiting until the OS handle is dropped;
- converter-created sensitive files are normalized to `0600` on Unix;
- the 2a.7 late-materialization, read-permit, crash-recovery, autosave, backup-ownership and payload-validation tests remain present.

## 5. Dependency and release QA

`webbrowser` remains pinned to patched **1.2.4**. `cargo audit` remains part of native acceptance. No broad dependency update is introduced by 2a.8.

The non-security macOS/Dioxus validation scripts no longer hard-code an obsolete source build number; they validate the actual `build-id.txt` without preventing subsequent builds from using the established packaging/runtime pipeline.

## Definition of E0–E3 source closure

With E0 real-device acceptance treated as resolved, source-level E0–E3 closure requires all of the following:

1. E0 threat model, benchmark isolation and canonical Argon2id policy remain intact;
2. E1 AEGIS-256 isolation, random note-key hierarchy, versioned formats and full 256-bit tag remain intact;
3. E2 protected note/attachment coverage and crash-safe filesystem↔SQLite transitions remain intact;
4. E3 autosave, backups, SQLite/WAL, caches, temporary materialization, editor/PDF authorization and cleanup guarantees remain intact;
5. Lock is a counted, synchronous per-note revocation barrier;
6. protected PDF sessions retain no open file handle beyond a request;
7. protected media OS handles are inseparable from a live vault permit;
8. converter-created sensitive outputs are explicitly private before publication;
9. protected payload graph validation remains fail-closed;
10. the patched dependency baseline remains in the lockfile;
11. cumulative source/static security and architecture QA passes.

Native Cargo/rustfmt/Clippy/Rust tests and `cargo audit` remain a separate executable acceptance gate and must not be reported as run where the Rust toolchain is unavailable.

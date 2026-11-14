# Batch 2A — E0/E1 cryptographic authority map

**Status:** Documentation reconciliation, not a new cryptographic design, implementation or test result. Full source documents remain unchanged and authoritative. This map covers **all headings and sections** of the original E0 and E1 texts; numbered source line ranges refer to the frozen 2b.43.3 baseline.

## Governing conclusions and precedence

- **E0 threat model:** Confidentiality and authentication of locked at-rest encrypted content and derivatives against offline artifact theft; not protection from compromised OS/process, unlocked memory, screen capture, denial of service, or weak passwords.
- **E0 2b.12 explicit override:** Comparative AEGIS/XChaCha/AES cipher benchmarking is **no longer an entry or security-release gate**. Production Rustnote Vault v1 fixes AEGIS-256, 256-bit key/nonce and **full 256-bit (32-byte) tag**. The isolated benchmarking harness remains optional.
- **Argon2id fixed v1:** v=19, 64 MiB, 3 passes, 4 lanes, 128-bit random salt, 256-bit derived wrapping key. Untrusted envelope resource requests are capped to that profile; later changes require explicit version/policy revision.
- **Key hierarchy:** A CSPRNG-generated independent random 256-bit scope/note key encrypts content. Password-derived material is a *temporary wrapping key*, not the bulk data-encryption key. Sensitive keys/buffers are zeroized where Rust ownership allows; this is not a guarantee that every historic process-memory copy is erased.
- **Isolated crypto authority:** `crates/rustnote-vault/` owns primitives and format parsing; the application uses its `src/security` facade. v1 format families are `RNVOBJ01` (bounded object), `RNVKEY01` (key envelope), `RNVBLB01` (streamed blobs); headers, context/domain, object identities, length/nonce/chunk metadata are authenticated; unknown algorithm/version/reserved fields reject.
- **Protected blobs:** Random 192-bit prefix + 64-bit chunk counter, 1 MiB default and 4 KiB–8 MiB accepted chunks in *Vault-note blob* format. Every chunk authenticates before writing; failure of a late chunk can leave already-authenticated prefix in a private sink, so final publication requires complete success.
- **Boundaries:** SQLite, FTS5, PDF.js and editor remain ordinary systems; selective protection must cover all plaintext equivalents (revision, FTS, attachment, autosave, preview, export, backup). Protected content remains absent from persistent FTS. In-memory unlocked search and global PDF search were **planned future work** in E0, not E0/E1 deliverables.
- **E1 explicitly excludes** real note encryption or any source-storage/UI mutation. Completion claims must be evaluated against the E1 test gates and subsequent implementation, not deduced from existence of this document.
- **Search/notebooks/cloud/biometrics/blockchain/PQ:** Historical design ideas and explicitly excluded future capabilities, not scope silently imported into E0/E1.

## Clause-level source traceability

The row status **Governing** means the heading is part of the original contract as amended; **Historical/overridden** means a particular choice is retained as provenance, but an explicit later decision prevails. All exact technical details remain in the source itself.

### E0: [original contract](../../SECURITY_ENCRYPTION_E0_CONTRACT.md) (Git blob `1daa3d82d081e4c6b35d72b4be85e55acd0d621a`)

| Clause / original heading | Lines | Classification |
| --- | ---: | --- |
| `Rustnote encryption E0 contract — build 2a.1` | 1–2 | Governing within stated E0/E1 scope |
| `Purpose` | 3–8 | Governing within stated E0/E1 scope |
| `2b.12 product decision override — AEGIS-256 fixed profile` | 9–25 | Governing within stated E0/E1 scope |
| `Product boundary` | 26–29 | Governing within stated E0/E1 scope |
| `Threat model` | 30–46 | Governing within stated E0/E1 scope |
| `Explicit non-goals / limits` | 47–60 | Scope constraint / non-goal |
| `Cryptographic roles` | 61–62 | Read together with 2b.12 override |
| `Data encryption` | 63–72 | Governing within stated E0/E1 scope |
| `Password hardening` | 73–78 | Governing within stated E0/E1 scope |
| `Key hierarchy` | 79–88 | Governing within stated E0/E1 scope |
| `Privacy boundary: plaintext derivatives` | 89–110 | Governing within stated E0/E1 scope |
| `Search contract` | 111–118 | Governing within stated E0/E1 scope |
| `Attachment contract` | 119–124 | Governing within stated E0/E1 scope |
| `Integrity and blockchain-inspired ideas` | 125–132 | Governing within stated E0/E1 scope |
| `Quantum-resistance posture` | 133–140 | Governing within stated E0/E1 scope |
| `Optional E0 benchmark harness (non-gating as of 2b.12)` | 141–156 | Read together with 2b.12 override |
| `E1 entry criteria (current 2b.12 interpretation)` | 157–166 | Read together with 2b.12 override |

### E1: [original contract](../../SECURITY_ENCRYPTION_E1_CONTRACT.md) (Git blob `739c2b248f00614054dad9e305ca276585cd496e`)

| Clause / original heading | Lines | Classification |
| --- | ---: | --- |
| `Rustnote encryption E1 contract — build 2a.2` | 1–2 | Governing within stated E0/E1 scope |
| `Objective` | 3–14 | Governing within stated E0/E1 scope |
| `Fixed E1 decisions` | 15–27 | Governing within stated E0/E1 scope |
| `Cryptographic formats` | 28–37 | Governing within stated E0/E1 scope |
| `AEGIS-256 policy` | 38–45 | Governing within stated E0/E1 scope |
| `Password / Argon2id policy` | 46–69 | Governing within stated E0/E1 scope |
| `Large protected blob contract` | 70–87 | Governing within stated E0/E1 scope |
| `Memory/key handling contract` | 88–98 | Governing within stated E0/E1 scope |
| `Explicit E1 non-goals` | 99–118 | Scope constraint / non-goal |
| `Blockchain-inspired integrity` | 119–122 | Governing within stated E0/E1 scope |
| `E2 entry criteria` | 123–133 | Governing within stated E0/E1 scope |

## Documentary QA and unresolved work

- Original E0/E1 contracts were read in full, including the internal 2b.12 override. No original contract was edited.
- This is a **traceability map**, not an independently executable AEGIS or Argon2id audit. Deeper matching of `RNVOBJ01`/`RNVKEY01`/`RNVBLB01` implementation against every byte-level clause is a separate source/code-test review.
- A later BackupKeyAuthority uses a distinct repository-key hierarchy and its own domains, **not** the Vault-note key envelope. Do not conflate E1 Vault blob chunk-size flexibility with the **fixed 1 MiB** v3 managed-backup format.
- The original [1a.34 design context](../../E0_E3_ORIGINAL_ARCHITECTURE_CONTEXT.md) is historic design provenance, not authority to reverse the explicit 2b.12 AEGIS selection.

**Preservation gate:** Source file paths and blob SHAs must remain exactly those listed above; all original sections remain addressable.

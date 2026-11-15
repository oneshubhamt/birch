# Security and managed-backup contracts — authority index

This folder is a **navigation and reconciliation layer**, not a substitute for the original precise contracts. Always use the untouched source file for exact cryptographic bytes, field offsets, domain strings, failure cases, and required tests.

## Read in dependency order

| Review scope | Reconciliation map | Source authority |
| --- | --- | --- |
| E0 threat model, v1 cipher/KDF override; E1 crypto boundary | [E0–E1 clause map](E0_E1_AUTHORITY.md) | [E0](../../SECURITY_ENCRYPTION_E0_CONTRACT.md), [E1](../../SECURITY_ENCRYPTION_E1_CONTRACT.md) |
| E2 selective notes, E3 managed plaintext/in-memory residue and amendments | [E2–E3 clause map](E2_E3_AUTHORITY.md) | [E2](../../SECURITY_ENCRYPTION_E2_CONTRACT.md), [E3](../../SECURITY_ENCRYPTION_E3_CONTRACT.md) |
| Device-bound managed backup key model, exact v3 wire format and staged 2b.43.3 source | [2b.43 backup clause map](BACKUP_AUTHORITY.md) | [2b.43.1](../../BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md), [2b.43.2](../../BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md), plus current source |

The original E0/E1 documents contain the explicit 2b.12 product decision selecting AEGIS-256; the original 1a.34 architecture narrative predates it. The E3 original contract has subsequent amendments through 2a.19; later 2b.42 source and reports refine some mechanisms. The 2b.43 backup key/format contracts are **future v3 requirements**, while the active managed repository writer remains v2 in this source snapshot.

**No implicit precedence by filename or timestamp.** Explicit amendments and governing-contract hierarchy matter; current code describes the state implemented, not necessarily compliance; old PASS reports are time-bounded evidence.

## Original 1a.34 context — classified as provenance

[Original architecture context](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/E0_E3_ORIGINAL_ARCHITECTURE_CONTEXT.md), Git blob `1ce0df7a5e5ba4acfdae0438cec5245c1bf08a13`, is retained byte-for-byte in the frozen Git baseline. It is a design discussion and roadmap: selective note encryption, plaintext derivative cleanup, later S1/S2 search, E4 RAM-only search, E5 encrypted notebooks, E6 biometric app lock, and much later sync/PWA/device-key exchange. The original preferred a **benchmark-dependent** choice between AEGIS and XChaCha; that was later expressly replaced by the 2b.12 fixed AEGIS-256 production decision. Its proposed features are **not evidence they were shipped**.

| Original design-context heading | Original lines | Classification |
| --- | ---: | --- |
| `What the audit tells me` | 18–48 | Historical design provenance, not current normative implementation |
| `What I would implement first` | 49–52 | Historical design provenance, not current normative implementation |
| `Pass E1 — Rustnote Vault foundation` | 53–100 | Historical design provenance, not current normative implementation |
| `Pass E2 — one genuinely encrypted note` | 101–145 | Historical design provenance, not current normative implementation |
| `Attachments` | 146–164 | Historical design provenance, not current normative implementation |
| `Pass E3 — close every plaintext leak` | 165–188 | Historical design provenance, not current normative implementation |
| `Then I would improve search` | 189–200 | Historical design provenance, not current normative implementation |
| `Persistent search` | 201–210 | Historical design provenance, not current normative implementation |
| `Private-session search` | 211–235 | Historical design provenance, not current normative implementation |
| `Global PDF search` | 236–302 | Historical design provenance, not current normative implementation |
| `Then encrypted notebooks` | 303–327 | Historical design provenance, not current normative implementation |
| `Only after that: native app lock` | 328–353 | Historical design provenance, not current normative implementation |
| `And only much later: cloud sync` | 354–374 | Historical design provenance, not current normative implementation |
| `So my actual implementation sequence would be` | 375–502 | Historical design provenance, not current normative implementation |
| `The key architectural decision` | 503–522 | Historical design provenance, not current normative implementation |

## Review and future work

- Map/check current claims against [status](../PROJECT_STATUS.md), not just text similarity.
- Batch 3 reconciled the later 2b/2b.42 implementation evolution into [Vault architecture](../security/VAULT_ARCHITECTURE.md), [managed plaintext lifetime](../security/PLAINTEXT_LIFETIME.md), and [Vault evolution](../security/VAULT_EVOLUTION.md). The E2–E3 map preserves the older contract wording while those documents describe the current source-level mechanism and supersession history.
- Later QA consolidation must classify scripts as behavioral/property/integration/static-source checks and verify the *actual* sensitive-content invariants rather than merely the continued presence of words or phrase snippets.
- Batch 2 carries **no source/crypto/storage changes** and does not certify security. Exact original contracts remain at their original paths to preserve validator and Git-history references.

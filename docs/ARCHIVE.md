# Documentation archive and no-loss map

**Application:** Rustnote · **repository:** Birch  
**Frozen source/document baseline:** `main@514b797fb0a6a128050cf75ac2428fc12095106a`  
**Purpose:** final storage map for the documentation-consolidation project. This is provenance and navigation, not runtime/security certification.

The repository originally contained **239 Markdown files, 226 at root**. Most were pass-numbered implementation, QA or native-compile records. The consolidation preserves their durable information in stable purpose-based documents while Git remains the immutable byte-for-byte archive.

## Final storage model

Use [README.md](README.md) as the documentation landing page. The intended storage classes are:

| Class | Final handling |
| --- | --- |
| Living project guides | Stable purpose-based paths under `docs/`, plus root README / CONTRIBUTING / SECURITY / THIRD_PARTY_NOTICES |
| Exact normative security/backup contracts | Retained byte-for-byte at their original root paths because exact cryptographic/format text matters and validators consume them |
| Historical implementation/QA/native history | Consolidated into three purpose-based ledgers; exact originals remain at the frozen Git baseline |
| Exact-path validator fixtures | Remain at root only because current validators read their filenames/content directly; not preferred documentation |
| Functional Markdown | GitHub templates, component READMEs and bundled welcome-note asset remain with their consumers |
| Temporary consolidation machinery | Removed after Batch 6; its exact state remains in Git history |

### Preferred reader-facing entry points

- [Architecture](ARCHITECTURE.md)
- [Development](DEVELOPMENT.md)
- [QA and release gates](QA.md)
- [Project status](PROJECT_STATUS.md)
- [Security/backup contract index](contracts/README.md)
- [Vault architecture](security/VAULT_ARCHITECTURE.md)
- [Managed plaintext lifetime](security/PLAINTEXT_LIFETIME.md)
- [Vault/security evolution](security/VAULT_EVOLUTION.md)
- [Historical QA evidence](qa/HISTORICAL_EVIDENCE.md)
- [Native toolchain history](development/NATIVE_TOOLCHAIN_HISTORY.md)

Component-specific authorities remain adjacent to their implementation: `editor-spec/README.md`, `editor-src/README.md`, `pdf-src/README.md`, and `tools/e0-crypto-bench/README.md`.

## Complete no-loss mapping for the 239-file baseline

The baseline is immutable at:

`https://github.com/oneshubhamt/birch/tree/514b797fb0a6a128050cf75ac2428fc12095106a`

Every deleted original can be retrieved by replacing `<path>` in:

`https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/<path>`

The 239 files are accounted for as follows.

| Baseline family | Count | Durable destination / exact-history authority |
| --- | ---: | --- |
| `QA_*.md` | **104** | [Historical QA evidence](qa/HISTORICAL_EVIDENCE.md) contains all 104 exact immutable links, commands/evidence limitations and disposition |
| `NATIVE_COMPILE_*.md` | **58** | [Native toolchain history](development/NATIVE_TOOLCHAIN_HISTORY.md) contains all 58 exact immutable links and chronological architectural lessons |
| noncontract `VAULT_*.md` + `SECURITY_E0_E3_*.md` | **51** | [Vault evolution](security/VAULT_EVOLUTION.md) contains the chronological decision/supersession record and complete 51-file immutable ledger |
| foundational E0–E3 / backup contracts and original architecture context | **8** | Six exact normative contracts remain at root; [contract maps](contracts/README.md) provide section-level reconciliation. The historical 1a.34 context and staged 2b.43.3 report are Git-only after Batch 6 |
| root living guides/legal + version promotion | **5** | README, CONTRIBUTING, SECURITY and THIRD_PARTY_NOTICES remain current; the old 2b.43 promotion report is Git-only and its durable facts are in project/native history |
| original `docs/` guides | **6** | Architecture, Development, Project Status and QA remain current; bootstrap QA and initial repository-setup instructions are Git-only historical evidence |
| component READMEs | **4** | editor-spec, editor-src, pdf-src and E0 benchmark guides remain beside their consumers |
| GitHub templates | **2** | Retained under `.github/` as functional repository surfaces |
| bundled welcome-note Markdown | **1** | Retained under `assets/` as application asset content |
| **Total** | **239** | **239/239 accounted for** |

### Historical singleton links removed from the working tree

These records remain directly inspectable at the frozen baseline:

- [Original 1a.34 E0–E3 architecture context](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/E0_E3_ORIGINAL_ARCHITECTURE_CONTEXT.md)
- [2b.43.3 secure-key-store implementation report](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md)
- [Initial repository setup instructions](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/REPOSITORY_SETUP.md)
- [2026-11-05 bootstrap QA report](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/docs/BOOTSTRAP_QA.md)
- [v0.12.6 / build 2b.43 promotion record](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VERSION_PROMOTION_V0.12.6_BUILD_2B43.md)

## Exact contracts retained in the working tree

The following remain byte-identical to the baseline and are intentionally **not** replaced by summaries:

- `SECURITY_ENCRYPTION_E0_CONTRACT.md`
- `SECURITY_ENCRYPTION_E1_CONTRACT.md`
- `SECURITY_ENCRYPTION_E2_CONTRACT.md`
- `SECURITY_ENCRYPTION_E3_CONTRACT.md`
- `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`
- `BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md`

Use the [contract index](contracts/README.md) for precedence and current-source reconciliation. Byte offsets, AAD strings, sizes, algorithms, nonce rules, parser limits and fail-closed behavior must be taken from the exact contract, not a shortened overview.

## Exact-path compatibility fixtures

Exactly **100 historical root Markdown files** remain because current validator scripts consume them directly:

- **54** `NATIVE_COMPILE_*.md` plan/implementation files;
- **35** `QA_*.md` reports;
- **11** `VAULT_*.md` / `SECURITY_E0_E3_*.md` implementation-history files.

They remain byte-identical to the baseline. Their presence is **not** a recommendation to read them as current documentation. Their durable content is already consolidated into the three historical ledgers above.

Moving/deleting these 100 files requires the separate QA-refinement effort to remove validators' dependence on historical prose. Batch 6 deliberately does not change scripts/tests/CI merely to improve file layout.

## Governing conflicts and open verification findings

The consolidation does not erase disagreements between old contracts, later implementation mechanisms and current verification state:

1. **Backup v3 is not deployed.** `src/security/backup_key_authority.rs` stages repository format 3 key/header authority, while `src/storage/backup_repository.rs` still writes repository format **2**. 2b.43.4+ remains future integration work.
2. **E3 VACUUM wording is historically superseded at the implementation-method level.** The original E3 contract specifies VACUUM; later 2b.42.8 forensic work and current source use `secure_delete + FTS5 rebuild + verified WAL TRUNCATE` without per-Protect VACUUM. The privacy outcome remains the governing invariant; native evidence remains time-bounded.
3. **Global UI epoch became scoped publication.** E3 2a.18's single-epoch mechanism was refined into library + multi-note + per-note generations. Current source implements the scoped model.
4. **Native CI feature coverage is incomplete.** Current CI Clippy/tests omit `--all-features`, while some protected PDF lifetime tests require the `internal-test-support` feature. This is a coverage gap, not evidence of an implementation defect.
5. **Historical PASS is not current certification.** Static/source guards, compiled Rust tests, native artifact checks and manual OS integration are distinct evidence classes.
6. **Encrypted backup/key-store platform behavior still needs real-device adversarial QA.** Source/API presence is not Keychain/Keystore runtime certification.

## Independent high-risk clause re-check performed in Batch 6

Batch 6 independently re-read and cross-checked the high-risk clauses rather than relying only on prior summaries:

- E0 fixed AEGIS-256 profile: 256-bit key, nonce and full tag; random NoteKeys; Argon2id v=19, 64 MiB, 3 passes, 4 lanes, 128-bit salt, 256-bit wrapping key; authenticated context/domain metadata.
- E1 exact Vault-format roles: `RNVOBJ01`, `RNVKEY01`, `RNVBLB01`; authenticated headers; bounded blob chunks; zeroizing Rustnote-owned secret/plaintext buffers with no claim of perfect process-memory erasure.
- 2b.43.1 device root used only for repository-keyset wrapping; independent random per-repository keys; device-local secure-store profiles; no plaintext fallback; root loss/corruption preserves encrypted repository and fails closed; legacy v2 plaintext repository migration explicitly prohibited.
- 2b.43.2 exact v3 format: 92-byte header prefix, 204-byte keyset, 268-byte macOS envelope, 232-byte Android envelope, 116-byte small-object header, 112-byte bulk header, exact 1 MiB v1 bulk chunks, trailing-byte/overflow rejection and explicit lack of whole-repository anti-rollback freshness.
- Current source still separates staged v3 key authority from active v2 backup persistence.
- Current privacy source uses note/multi-note/library generations and current Protect residue source omits per-Protect VACUUM.

These are documentary/source checks only; no native crypto, Keychain, crash/restart or integrated v3 backup test was executed by the consolidation project.

## Consolidation audit artifacts

The temporary Batch 0–5 inventory/plan/register are intentionally not permanent living documentation. Their exact final pre-Batch-6 state remains available at commit `7cd6aa9833acc9af9605b85cc160813c60703f35`:

- [Consolidation plan](https://github.com/oneshubhamt/birch/blob/7cd6aa9833acc9af9605b85cc160813c60703f35/docs/CONSOLIDATION_PLAN.md)
- [239-file frozen inventory](https://github.com/oneshubhamt/birch/blob/7cd6aa9833acc9af9605b85cc160813c60703f35/docs/DOCUMENT_INVENTORY.md)
- [Batch 1–5 consolidation register](https://github.com/oneshubhamt/birch/blob/7cd6aa9833acc9af9605b85cc160813c60703f35/docs/CONSOLIDATION_REGISTER.md)

This archive index is the durable replacement for those working documents.

## Discoverability and duplication benchmark

| Metric | Baseline | Final Batch 6 target |
| --- | ---: | ---: |
| Total Markdown files | 239 | **132** |
| Root Markdown files | 226 | **110** |
| Pass-numbered QA/native/Vault-security files in baseline | 213 | **100 compatibility fixtures + 3 consolidated histories** |
| Preferred pass-history reader entry points | dozens/hundreds of pass files | **3** |
| Exact normative security/backup contracts | 6 | **6 unchanged** |
| Temporary consolidation tracking files | 0 initially / 3 during work | **0** |

The raw file count remains inflated by the 100 exact-path validator fixtures. Excluding those compatibility inputs, the final tree has **32 Markdown files with actual living, normative, functional or consolidated-history purpose**. This is the meaningful discoverability measure until QA is refactored.

## Evidence boundary

This project reorganizes documentation only. It does not claim that current CI passed, that a signed app was produced, that cryptography was independently certified, or that backup v3 is implemented. For current verification requirements use [QA](QA.md) and [Project status](PROJECT_STATUS.md).

# Consolidation change and conflict register

**Frozen baseline:** `main@514b797fb0a6a128050cf75ac2428fc12095106a`. See the [complete inventory](DOCUMENT_INVENTORY.md) for every original file path, size and Git blob SHA; see the [preservation protocol](CONSOLIDATION_PLAN.md) for loss-prevention rules.

## Batch 1 source-to-destination mapping

| Original document | Material handled | Canonical location / disposition |
| --- | --- | --- |
| `README.md` | Identity, setup, source map, QA and historical-record cautions | Short landing page; detailed ownership in [architecture](ARCHITECTURE.md), commands in [development](DEVELOPMENT.md), stage in [status](PROJECT_STATUS.md) |
| `docs/ARCHITECTURE.md` | Dioxus/editor/PDF, storage, Vault, platform and backup boundaries | [Architecture](ARCHITECTURE.md), with explicit active-v2/staged-v3 separation |
| `docs/DEVELOPMENT.md` | Toolchain, preparation, execution, native and JS QA commands | [Development](DEVELOPMENT.md); same commands retained |
| `docs/QA.md` | CI matrix, manual security/release gates, evidence limits | [QA](QA.md); actual CI runner scope clarified |
| `docs/PROJECT_STATUS.md` | Version, test limits, unimplemented v3 backup, no app rename | [Status](PROJECT_STATUS.md), checked against code |
| `CONTRIBUTING.md` | Audit/plan/implementation/re-audit/QA procedure | **Unchanged**, authority preserved |
| `SECURITY.md` | Vulnerability policy, threat model, baseline warnings | **Unchanged**, security authority preserved |
| `THIRD_PARTY_NOTICES.md` | Notices, package versions, PDF.js vulnerability caveat | **Unchanged**, pending separate dependency review |
| `docs/REPOSITORY_SETUP.md` | Initial empty-repository bootstrap and administration recommendations | **Unchanged historical record**; labelled in index |
| `docs/BOOTSTRAP_QA.md` | 2026-11-05 environment-specific checks and gaps | **Unchanged historical evidence**; labelled in index/status |
| `editor-spec/README.md` | Editor document schema and validation authority | **Unchanged** |
| `editor-src/README.md` | Editor module dependency/ownership map | **Unchanged** |
| `pdf-src/README.md` | PDF viewer module/layout/runtime rules | **Unchanged** |
| `tools/e0-crypto-bench/README.md` | Research-only benchmark workflow | **Unchanged** |
| `.github/ISSUE_TEMPLATE/bug_report.md` | Non-sensitive issue reporting | **Unchanged** |
| `.github/pull_request_template.md` | Audit/review checklist | **Unchanged** |

The [documentation index](README.md) is new. Existing Vault, security, backup, QA, and native-compile pass records remain **untouched at their exact paths**, including all normative byte-level specifications and historical QA evidence.

## Findings and deferrals

| ID | Verified distinction or risk | Outcome |
| --- | --- | --- |
| D1-01 | `src/storage/backup_repository.rs` declares repository **v2**, while the proposed encrypted format is **v3** | Clarified; implementation deferred to 2b.43.4 |
| D1-02 | `src/security/mod.rs` labels backup key authority staged | Recorded, not incorrectly represented as integrated |
| D1-03 | `scripts/ci-source-checks.sh` runs four version/backup shell validators, not all historic validators | Clarified in [QA](QA.md) |
| D1-04 | Workflow triggers are `main` push, PR and manual dispatch, not every feature-branch push | Clarified in [QA](QA.md) |
| D1-05 | Historical initial-publication instructions assume a previously empty repository | Retained as provenance; not routine current instructions |
| D1-06 | Bootstrap QA PASS counts refer to an earlier preparation environment | Retained as historical; not replayed |
| D1-07 | Third-party PDF.js baseline has a stated vulnerability and workaround | Original notice retained; separate dependency/security review required |
| D1-08 | Some static validators rely on exact historical paths | No historical document moved or deleted |
| D1-09 | Native Keychain/privacy and end-to-end backup behavior need dedicated verification | Preserved as open release gates |

**Quality standard:** a document's shorter size or newer timestamp is not evidence of better accuracy. Every removed requirement must have a traceable preserved destination; any unresolved contradiction must remain visible. No source, script, test or CI change is authorized in Batch 1.

## Batch 1 post-change audit and closure (2026-11-14)

- **Baseline preservation:** All **239** original Markdown document paths remain present; **234** are byte-identical to baseline Git blob identities, while **five living guides** were intentionally rewritten and mapped above.
- **Scope discipline:** Exactly **four** new Markdown files exist on this branch compared with `main`: the Batch 0 plan/inventory and Batch 1 index/register. There were **zero deletions** and **zero non-Markdown changes**; source, scripts, tests, CI, contract files and historical QA remain unchanged.
- **Reference integrity:** Checked **317 relative Markdown links** across the nine added/revised and consolidation-tracking Markdown documents; **zero unresolved paths or checked heading fragments**.
- **Manual semantic re-audit:** The original guides' build commands, platform/toolchain requirements, app identity, data/source ownership, security and backup limitations, verification caveats and manual release gates remain covered in the new purpose-based references.
- **No executed runtime QA:** No Rust compilation, Keychain/device test, browser test, complete validator suite, CI-run inspection or release packaging was performed in Batch 1. Those gates are neither claimed nor waived.

**Batch 1 closure: documentation structure and preservation checks PASS; functional and security-runtime verification NOT RUN.** Next is Batch 2: full clause-level security/backup contract audit, preserving byte-precise specifications without premature consolidation.

## Batch 2 — foundational security and backup contract reconciliation (2026-11-14)

**Method:** Read eight original source documents in full in three independently staged groups (E0/E1, E2/E3 including amendments through 2a.19, and 2b.43.1/2b.43.2/2b.43.3). Use [the contract index](contracts/README.md) for section-by-section original line references, original Git blob SHA identities, exact source links and precedence.

| Original source document | Classification | Preservation / new map |
| --- | --- | --- |
| `SECURITY_ENCRYPTION_E0_CONTRACT.md` | Normative threat and crypto policy with 2b.12 selection override | **Unchanged**, [E0/E1 map](contracts/E0_E1_AUTHORITY.md) |
| `SECURITY_ENCRYPTION_E1_CONTRACT.md` | Normative Vault crypto component and format policy | **Unchanged**, [E0/E1 map](contracts/E0_E1_AUTHORITY.md) |
| `SECURITY_ENCRYPTION_E2_CONTRACT.md` | E2 phase contract with 2a.18 protected revision restore amendment | **Unchanged**, [E2/E3 map](contracts/E2_E3_AUTHORITY.md) |
| `SECURITY_ENCRYPTION_E3_CONTRACT.md` | Normative residue/privacy goals and cumulative 2a.7–2a.19 amendments | **Unchanged**, [E2/E3 map](contracts/E2_E3_AUTHORITY.md) |
| `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md` | Normative device-root/key/state/cleanup contract | **Unchanged**, [backup map](contracts/BACKUP_AUTHORITY.md) |
| `BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md` | Normative, subordinate exact v3 binary specification | **Unchanged**, [backup map](contracts/BACKUP_AUTHORITY.md) |
| `BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md` | Historical staged implementation + compile QA narrative, not v3 storage integration | **Unchanged**, [backup map](contracts/BACKUP_AUTHORITY.md) |
| `E0_E3_ORIGINAL_ARCHITECTURE_CONTEXT.md` | Historical 1a.34 planning discussion (including later-planned search/notebooks) | **Unchanged**, [historical context map](contracts/README.md) |

**Traceability result:** All eight originals remain the precise, unabridged source of technical clauses. Three purpose-specific authority maps plus one index were added; none substitutes a shortened prose summary for canonical bytes or changes the format. All 2b.43.2 sections, all 2b.43.1 sections, the 2b.43.3 report, and all E3 2a.7–2a.19 amendments are linked by heading and original line interval.

### Source/code cross-checks and documentary discrepancies

| ID | Issue | Status |
| --- | --- | --- |
| D2-01 | E0 initial optional/candidate cipher selection versus explicit 2b.12 fixed AEGIS-256/Argon2id v1 policy | **Resolved for documentary authority:** 2b.12 is the governing override; historical comparison remains |
| D2-02 | E2 original read-only note model versus later protected revision and granular mutation work | **Phase-specific old scope**; full later historical/implementation reconciliation deferred to Batch 3 |
| D2-03 | E3 initially mandates `VACUUM`; 2b.42.8 later adopts no-VACUUM scrub | **Later method documented and source checked**; final native/forensic acceptance not certified by this pass |
| D2-04 | E3 2a.18's global UI privacy epoch versus 2b.42's per-note/multi-note/library generations | **Later scoped authority documented and source checked**; full end-to-end behavioral audit deferred to Batch 3 |
| D2-05 | Planned encrypted managed repository version 3 versus currently active v2 repository persistence | **Clearly separated**; 2b.43.4+ implementation and QA remain outstanding |
| D2-06 | Key unavailable/corrupt, unknown repository, offline legacy cleanup, key-loss Restore, external user-owned exports | **Required fail-closed or explicit out-of-scope behavior preserved**, not silently simplified |
| D2-07 | 2b.43.3 native compile/test follow-up reports and static tests | **Historical evidence only**; not a new run or release proof |
| D2-08 | Byte-level v3 canonical parser, AAD, identity, nonce, publication and no-rollback rules | **Original normative text intact**; actual integrated reader/writer and adverse-input QA remain future gates |

### Targeted independent mathematical checks

The 2b.43.2 §20 structural vectors were **independently recomputed** using SHA-256 and HMAC-SHA-256:
- `library_example_001` encodes as 19 UTF-8 bytes and matches the prescribed SHA-256 library binding.
- `SHA-256("abc")`, combined with a 32-byte key `00..1f`, matches the prescribed HMAC opaque blob ID.
- The exact bulk-container formula gives encoded lengths **144** (zero-byte blob), **177** (1 byte), **1,048,752** (1 MiB), and **1,048,785** (1 MiB + 1). These are arithmetic cross-checks, **not** v3 implementation round-trip tests.

**Batch 2 security discipline:** No code changes, no existing security contract replacements, no Keychain/device/cryptographic runtime certification, no assumption that future requirements are operational. Refer to each map for all detailed clauses.

### Batch 2 post-implementation targeted documentary QA — closure

- **Completeness of structural trace:** Section-heading counts independently compared against originals: E0=17, E1=11, E2=13, E3=32, 2b.43.1=42, 2b.43.2=45, 2b.43.3=21, original 1a.34 context=15. **196/196 heading-level entries mapped** across the four new contract-folder documents.
- **Source integrity:** All **eight Batch 2 original documents remain byte-identical** to their baseline Git blobs, including every field table, AAD string, exact-size rule, amendment, original narrative and test requirement. No document was moved or deleted.
- **Repository scope:** Against `main@514b797`, all **566 baseline tracked files still exist**. The branch holds **574** tracked files (eight additional Markdown references across Batches 0–2), with only **five deliberately edited existing living guides**. **Zero non-Markdown changes**; CI/scripts/tests/source remain identical.
- **Links:** The six Batch 2 newly added/affected navigation, authority and register documents were checked for **73 internal relative links**, with **zero unresolved paths or checked heading fragments**.
- **Mathematics:** Canonical 2b.43.2 structural SHA-256/HMAC vectors matched independent calculations; bulk record file-length boundary arithmetic agreed with the specified formula.
- **Behavioral QA:** **Not run.** No Cargo/Rust, JavaScript runner, macOS Keychain device checks, restore integration or forensic disk verification executed for this documentation-only batch. Historical source-audit/CI PASS claims were not imported as a new test result.
- **Open reconciliation:** D2-02 (historical E2 mutation model), D2-03 (later no-VACUUM implementation), and D2-04 (scoped UI privacy) require the later Batch 3 full pass-history source and test audit. D2-05 remains a future 2b.43.4+ implementation task, not a documentation defect.

**Batch 2 result:** **PASS** for preservation, clause-level discoverability, structural completeness and internal-link QA; **NOT CERTIFIED** for cryptographic/runtime security or later-version behavioral completeness. Continue to Batch 3 without altering the governing originals.


## Batch 3 — Vault/security implementation evolution consolidation (2026-11-15)

Batch 3 changes strategy from temporary preservation to **actual consolidation**. All 51 targeted root-level `VAULT_*.md` and noncontract `SECURITY_E0_E3_*.md` records were reviewed chronologically and reduced into three current-purpose documents:

- [Current Vault architecture](security/VAULT_ARCHITECTURE.md)
- [Managed plaintext lifetime](security/PLAINTEXT_LIFETIME.md)
- [Vault implementation evolution](security/VAULT_EVOLUTION.md)

The three documents preserve the durable architecture, I1–I7, important defect discoveries/repairs, supersession history, security boundaries and evidence limitations. The exact originals remain recoverable from the frozen baseline Git commit; [DOCUMENT_INVENTORY.md](DOCUMENT_INVENTORY.md) now links each baseline document to that immutable commit rather than assuming the file remains in the current tree.

### Deletion policy

**40 of the 51 Batch 3 pass-specific documents are superseded and scheduled for deletion in this batch.** Git history is the archive; the working tree should not duplicate it.

Exactly **11** Batch 3 files remain temporarily because current executable validator scripts read them by exact path/content:

```text
SECURITY_E0_E3_REMEDIATION_2A15.md
SECURITY_E0_E3_REMEDIATION_2A16.md
SECURITY_E0_E3_REMEDIATION_2A17.md
SECURITY_E0_E3_REMEDIATION_2A18.md
SECURITY_E0_E3_CLOSURE_2A19.md
VAULT_2B1_AUDIT_PLAN.md
VAULT_2B8_PLAINTEXT_AUTHORITY_CLOSURE_PLAN.md
VAULT_2B11_ATOMIC_PRIVACY_AUTHORITY_CLOSURE.md
VAULT_2B12_AEGIS_PRIVACY_DISCLOSURE_CLOSURE.md
VAULT_2B42_8_NO_VACUUM_FORENSIC.md
VAULT_2B42_9_CLEANUP.md
```

These are **QA compatibility dependencies, not preferred documentation**. Batch 4 should redesign the relevant validators around behavioral/source invariants or stable contracts, remove their dependence on historical report prose, and then delete the remaining 11.

### Durable conclusions carried forward

- 2a.6–2a.19 established crash-safe transitions, canonical plaintext ownership, backup privacy obligations, UI lifetime and independent adversarial closure.
- 2b.1–2b.4 replaced monolithic protected persistence with granular authenticated Vault objects and coordinator/token authorities.
- 2b.5–2b.12 consolidated ownership, removed caller-controlled security semantics, introduced strict absence proof, durable namespaces, external backup ownership, compound privacy authorization, RevisionIndex and fixed AEGIS-256 policy.
- 2b.42 replaced global publication invalidation with note/multi-note/library scopes, separated whole-library snapshot authority, measured the system, removed per-Protect VACUUM after forensic evidence, deleted obsolete development compatibility and independently repaired residual ambiguous-path ownership defects.
- Historical source/static PASS statements remain time-bounded evidence; they are not promoted into current native-runtime certification.

**No production source change is authorized by Batch 3.**


### Batch 3 post-consolidation closure

- **51/51 targeted Batch 3 source records were reviewed chronologically** across the 2a.6–2a.19 E0–E3 closure/remediation sequence, 2b.1–2b.12 Vault architecture sequence, and 2b.42.1–2b.42.11 scoped-publication/final-closure sequence.
- **40 superseded pass-specific Markdown files were deleted** from the working branch after their durable architecture, defect/fix history, supersession rules and evidence limits were consolidated.
- **11 historical files remain unchanged only because current validator scripts read them directly.** Their baseline Git blob identities are unchanged. They are scheduled for removal during Batch 4 after QA no longer depends on report prose.
- Three canonical documents now own the information: [Vault architecture](security/VAULT_ARCHITECTURE.md), [managed plaintext lifetime](security/PLAINTEXT_LIFETIME.md), and [Vault evolution](security/VAULT_EVOLUTION.md).
- The frozen [document inventory](DOCUMENT_INVENTORY.md) now links **all 239 baseline Markdown documents** to the immutable baseline commit, so deleted originals remain directly inspectable without remaining in the current working tree.
- Current source was cross-checked for schema **21**, Vault manifest format **2**, Vault note format **3**, RevisionIndex format **1**, granular object persistence, scoped note/multi-note publication authority, typed plaintext ownership, strict NotFound-only absence semantics and the current no-VACUUM SQLite scrub sequence.
- All **98 repository scripts** were inspected for direct Batch 3 filename dependencies: only the 11 retained files are required by current scripts. All **17 files under `tests/`** were also checked and contain no direct Batch 3 historical-document dependency.
- The 16 current reader-facing/consolidation/contract/security documents were checked for **132 internal relative links** after deletion; **zero missing relative targets** remain in that current documentation set.
- There are **zero non-Markdown changes** in the branch versus `main`; application source, tests, CI and validator behavior are unchanged by Batch 3.
- Markdown count has fallen from the **239-file baseline to 210** even after adding the current-purpose consolidation documents. Root-level Markdown fell from **226 to 186**. The remaining root bloat is now dominated by **104 QA records + 58 native-compile records**, which belong to Batches 4 and 5 rather than Vault documentation.
- No Rust/native/browser/Keychain/runtime QA was executed by this documentation pass. Historical execution claims remain historical evidence only.

**Batch 3 result: PASS for content consolidation, current-architecture reconstruction, deletion discipline, validator-dependency preservation and targeted link QA.** The remaining 11 historical Vault/security files are an explicit temporary debt owned by Batch 4, not the intended final documentation architecture.

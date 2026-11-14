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

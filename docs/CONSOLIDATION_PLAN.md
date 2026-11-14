# Documentation consolidation — preservation-first plan

**Working branch:** `docs-consolidation`  
**Source baseline:** `main@514b797fb0a6a128050cf75ac2428fc12095106a`  
**Initial inventory:** [DOCUMENT_INVENTORY.md](DOCUMENT_INVENTORY.md), 239 existing Markdown files (226 at root).  
**Scope of Batch 0:** Documentation inventory, authority rules, sequencing and loss-prevention protocol. No existing documents, application code, tests, or QA scripts are changed.

## Why consolidation must be staged

The baseline repository contains historical design decisions, byte-level security and backup format contracts, implementation and remediation narratives, compile plans, QA claims, developer guides, CI templates and component documentation. They differ in purpose and authority. The baseline README explicitly states that some validators depend on historical documents **at exact paths**; indiscriminate relocation could disable or break QA. Several reports are historical evidence, not proof that the current commit has been tested.

Therefore a smaller file count is **not** a success criterion by itself. The aim is to reduce reader-facing duplication and ambiguity while preserving hard technical information and traceability. Physical archive/removal is the *last* step, not the first.

## Preservation invariants

1. **Losslessness:** Until the relevant batch has been audited and its successor independently checked, retain each original file's exact bytes, path and Git blob SHA. Record all modifications and their rationale.
2. **No silent semantic changes:** Preserve cryptographic primitives, byte offsets, field layouts, algorithms, AAD, versioning, platform assumptions, failure behavior, lifecycle and privacy requirements, numeric limits, test acceptance criteria and known caveats verbatim where those precise facts matter.
3. **Traceable consolidation:** Every merged/reformulated requirement must map to original document(s), relevant sections, and—when possible—the implementing source/tests. Preserve negative requirements and unimplemented plans, not just successful stories.
4. **Evidence separation:** Distinguish **proposed**, **contractually required**, **implemented**, **verified on specified environment/commit**, **unverified**, **obsolete**, and **superseded**. A historical PASS is not contemporary evidence.
5. **Conflict resolution:** Never silently choose the newest or most confident-sounding statement. Compare date, scope, approved contract, explicit amendment, code behavior, and verification. Put unresolved contradictions in a conflict register rather than arbitrarily resolving them.
6. **Path and link safety:** No move/delete until references from scripts, CI, test files, source, existing Markdown links and external workflows have been inventoried and updated. Maintain redirects/stubs only when truly necessary; avoid an indefinite second documentation system.
7. **No code or QA refactoring in this project stage:** Documentation findings that imply implementation defects go into a follow-up issue/register. Do not "fix" application behavior while consolidating its documentation.
8. **Commit isolation and reversibility:** Each batch closes with a narrow commit/diff, unchanged-source check, reviewable mapping and identified residual risks. Changes remain on this working branch until accepted.

## Authority model (must be checked per assertion)

| Evidence kind | Meaning | Handling |
| --- | --- | --- |
| Current source and executable tests | What this exact checkout implements/tests | Inspect directly; runtime test execution still required to claim observed behavior |
| Governing contracts and accepted amendments | Intended normative rules and exact formats | Preserve detailed clauses; document adoption and supersession evidence |
| Later design or implementation report | Proposed or reported change | Confirm whether incorporated in contract and code |
| Historical QA/closure report | Time-bounded verification claim | Preserve original commit, OS, commands and limitations; do not transfer PASS to new branch |
| Original plans/discussions | Provenance and abandoned/alternative concepts | Retain as historic, not present-tense app specification |
| Living guides and status | Current reader entry points | Correct against the source and contracts; link to detailed authorities |

Code/contract disagreement is an **open finding**, not a license to rewrite the contract as though behavior were compliant.

## Ordered batches

### Batch 0 — Inventory and protective groundwork (this pass)

- Collect all Markdown paths, Git blob identities, sizes and provisional families; do not infer authority from filenames alone.
- Read existing root/working guides and key-contract introductions.
- Identify exact-path validator risk and establish this preservation protocol.
- Produce the frozen inventory plus this plan; verify that both are additive-only.

**Exit:** The 239 original Markdown files remain byte-identical and unmoved; two new `docs/` files are the only committed changes.

### Batch 1 — Reader-facing documentation and present state

**Scope:** `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `THIRD_PARTY_NOTICES.md`, the six pre-existing `docs/*.md` guides, and relevant source/component READMEs.

- Reconstruct the source baseline and current feature status from code/CI rather than historical completion claims.
- Establish one clear documentation landing page, architecture overview, developer workflow, security limitations, and QA reference.
- Mark repository-bootstrap instructions/reports as historical if no longer applicable.
- Preserve all baseline documents until linked and verified; record discrepancies separately.

### Batch 2 — Foundational security and backup format contracts

**Scope:** `SECURITY_ENCRYPTION_E0_CONTRACT.md` through `E3_CONTRACT.md`; `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`, `BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md`, `BACKUP_2B43_3_SECURE_KEY_STORE_IMPLEMENTATION.md`, and the original E0–E3 architecture context.

- Read contracts **in full**, especially all byte-level formats, cryptographic domain separation, key ownership, fail-closed paths and migration rules.
- Create a clause-by-clause authority map. Mark unimplemented backup cutover (2b.43.4) distinctly from completed design or source-level setup.
- Do not merge unrelated security contracts into a shorter prose summary that drops precision. Keep canonical byte-level specifications intact.

### Batch 3 — Vault and security implementation evolution

**Scope:** 37 `VAULT_*.md` and 14 noncontract `SECURITY_*.md` files, divided into chronological sub-batches (2b.1–2b.12, 2b.42, E0–E3 remediations).

- Build a supersession and decision register: what changed, why, governing invariant, current corresponding code, and whether later audits reversed an earlier conclusion.
- Separate durable architecture decisions from transient bug-fix narratives and abandoned proposals.
- Cross-check later findings against earlier reports before asserting closure.

### Batch 4 — QA evidence and test scope

**Scope:** 104 `QA_*.md` records, in thematic sub-batches (architecture, encryption/security, Vault, native compile, UI/editor/PDF, backup).

- Preserve the actual commands/results/limitations of each report while deduplicating navigation and repeated explanations.
- Create a compact test/requirement traceability matrix; note static phrase checks versus executable behavioral tests without changing the QA suite in this documentation pass.
- Treat a recorded PASS as a historical claim requiring independent reproduction for current closure.

### Batch 5 — Native compile history and supplementary documents

**Scope:** 58 `NATIVE_COMPILE_*.md` plan/implementation files, version-promotion records, component README files, templates and bundled content.

- Build a concise timeline of architectural lessons, persistent compatibility constraints and current toolchain gates.
- Keep compile bug histories accessible via an indexed archive/ledger even if living guides no longer repeat each incident.
- Do not classify asset content or repository templates as redundant documentation without a consumer-reference audit.

### Batch 6 — Rationalize storage and independently verify

- Produce an authoritative landing/index + minimal living documentation; use stable, purpose-based paths instead of pass-numbered entry points.
- Decide whether old historical documents should remain at root (because scripts depend on them), be grouped after dependencies change in a later QA pass, or be retained solely in a structured archive.
- Apply a **no-loss mapping**: original path/section → retained authoritative content or immutable historical location → evidence.
- Independently re-read representative sections and all high-risk cryptographic clauses; verify unresolved conflicts explicitly.
- Review cross-references, relative links, scripts, CI and docs-related paths. Compare all non-document tree entries with baseline.
- Benchmark documentation discoverability and duplication before calling consolidation complete. File-count reduction without traceability does not count.

## Required register format for later batches

| Source path + section | Requirement / historical fact | Classification | Intended canonical destination | Superseded by / evidence | Status |
| --- | --- | --- | --- | --- | --- |
| Filled when the relevant batch is inspected | Exact, not paraphrased where byte accuracy matters | Contract / architecture / observed / proposal / caveat | Stable path + heading | Source + implementation/test references | Preserved / open conflict / verified |

## QA and closure criteria for each batch

1. **Audit:** Independently read all targeted source documents (not merely titles or keyword matches). Inventory original and inbound/outbound references.
2. **Plan:** State exact consolidations, non-goals, acceptance criteria and preservation map.
3. **Implement:** Change only approved documents and indexes, with the smallest coherent diff.
4. **Re-audit:** Check the target text against originals, current code and governing constraints; flag any ambiguous authority.
5. **Targeted QA:** Verify path/link integrity, intact detailed clauses, complete mapping, no removed critical caveats, and source/test/CI unchanged unless explicitly planned.
6. **Full closure:** Before a bulk removal or final branch merge, compare against the baseline document manifest, review all conflicts and perform a complete document-reference audit.

## Known initial risks (not yet resolved)

- Historical root documents are input to existing exact-path source validators.
- Some source-level QA reports record passing static checks while explicitly lacking native macOS/Keychain evidence.
- Current backup key-store implementation and an actually encrypted backup persistence cutover are **different milestones**; documentation must not conflate them.
- The repository name Birch and the Rustnote application identity remain distinct.
- A filename or a newer pass number alone is insufficient to establish governing authority.

**Batch 0 delivers inventory and a safe operating procedure, not a semantic review of every document.** Proceed to Batch 1 only after accepting this preservation boundary.

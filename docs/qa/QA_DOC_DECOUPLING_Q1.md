# Q1 — QA/document decoupling audit and redesign

## Scope and baseline

This is an audit-only pass. It makes no production, validator, test, CI, format, security, privacy or backup behavior change and deletes no historical evidence.

Audited branch baseline: `working` at `d96fe9c722561fc46b79e552583b151f5b095442`. At that baseline, `main` pointed to the same commit and `legacy` remained isolated at the repository root commit.

Repository inventory at the audited baseline:

- 458 tracked files.
- 132 Markdown files, 110 of them at repository root.
- 86 `scripts/validate-*.sh` validators, totalling 551,530 bytes.
- 98 files under `scripts/`, totalling 603,606 bytes.
- 17 files under `tests/`, totalling 156,269 bytes.
- 43 validators directly read or require Markdown.
- No JS test, workflow file, or non-validator build/prepare/run script directly reads Markdown.
- 107 distinct Markdown files are executable validator inputs:
  - 100 historical compatibility fixtures;
  - 6 normative security/backup contracts;
  - `pdf-src/README.md`.

The 100 compatibility fixtures are exactly:

| Family | Count | Notes |
| --- | ---: | --- |
| `NATIVE_COMPILE_2B*_PLAN.md` + `NATIVE_COMPILE_2B*_IMPLEMENTATION.md` | 54 | 27 pass pairs |
| `QA_*.md` | 35 | 27 native QA records + 8 other QA records |
| historical `SECURITY_E0_E3_*` + `VAULT_*` records | 11 | remediation/closure/plan evidence |
| **Total** | **100** | Removing these eventually leaves 32 Markdown files |

The six normative contracts are intentionally outside that 100:

- `SECURITY_ENCRYPTION_E0_CONTRACT.md`
- `SECURITY_ENCRYPTION_E1_CONTRACT.md`
- `SECURITY_ENCRYPTION_E2_CONTRACT.md`
- `SECURITY_ENCRYPTION_E3_CONTRACT.md`
- `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md`
- `BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md`

They may remain useful human normative documents even after executable QA stops scraping them.

## Exact direct dependency map

### Architecture

| Validator | Markdown input | Exact assertion derived from Markdown | Classification | Q2+ disposition |
| --- | --- | --- | --- | --- |
| `validate-architecture-b5.sh` | `pdf-src/README.md` | README must contain `errors.mjs`, `preview_cache.mjs`, `render_scheduler.mjs`, `sidebar.mjs`, `transport_policy.mjs` | documentation phrase/presence | Remove from executable QA. If documentation completeness is desired, keep it as a non-release documentation lint, not an architecture invariant. |

### Backup 2b.43

| Validator | Markdown input | Exact assertion derived from Markdown | Classification | Replacement source of truth |
| --- | --- | --- | --- | --- |
| `validate-backup-2b43-1-key-contract.sh` | `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md` | Requires prose markers for DBRK/keyset fields, secure-store failure states, Apple/Android profiles, v3 format names, HMAC object IDs, plaintext authority, key-loss restore behavior, no migration, and transitional Protect purge | legitimate requirements; weak prose oracle | Rust constants/types + unit/integration tests for key generation/retrieval/failure state machine/context binding; thin source rule only for platform boundary where runtime testing is unavailable |
| same | `QA_BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md` | Requires PASS wording, “All fifteen…”, and `contract-only` | historical/provenance phrase-presence | Remove from QA. Git + consolidated evidence ledger retain provenance. |
| `validate-backup-2b43-2-byte-format.sh` | `BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md` | Requires version/magic/size/AAD/HMAC/publication-authority prose markers; re-derives 92/204/116/112/268/232-byte arithmetic; computes canonical library-binding and opaque-blob-ID vectors and requires the expected hex vectors to appear in prose | legitimate format/vector requirements; partially strong computation but wrong oracle | Canonical encoder/parser tests, golden byte vectors, negative parser tests and constants in executable code/fixtures; contract remains explanatory |
| same | `BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md` | File existence only | historical coupling to a normative document | Remove dependency; encode version/authority relationship in code/tests |
| `validate-backup-2b43-3-secure-key-store.sh` | indirect via 2b43.1 + 2b43.2 | Invokes both validators before its direct source checks | chained/duplicated | Make secure-key-store suite depend on executable primitives, not predecessor scripts |

Important defect in QA design, not production behavior: 2b43.1 reads `src/storage/backup_repository.rs` and `src/platform/mod.rs` but its contract-only assertions are against prose/QA records, not those production sources. 2b43.2 is materially stronger because it recomputes arithmetic/vectors independently, but it still compares the result to prose instead of an encoder/parser implementation.

### Security E0–E3

| Validator | Markdown input | Exact assertion derived from Markdown | Classification | Replacement |
| --- | --- | --- | --- | --- |
| `validate-security-e0.sh` | E0 contract | Requires sections/markers: Threat model; non-goals; Argon2id; AEGIS-256; XChaCha20-Poly1305; AES-256-GCM; plaintext-derivative boundary; search; attachments; integrity/blockchain ideas; quantum posture; E1 entry criteria | normative content, phrase-presence mechanism | Keep contract human-readable; test primitive isolation, pins, vectors/policies and boundaries directly |
| `validate-security-e1.sh` | E1 contract | Requires AEGIS policy, Argon2id policy, large-blob contract, memory/key handling, non-goals, integrity, E2 entry criteria | normative content, phrase-presence | Direct crypto-format/KDF/key-lifetime tests |
| `validate-security-e2.sh` | E2 contract | Requires full 256-bit tag statement plus SQLite/attachment/search/export-backup/plaintext-derivative/integrity/DoD headings | normative content, phrase-presence | Direct storage/locked-state/search/export/attachment tests; retain minimal structural guards only where behavior cannot be isolated |
| `validate-security-e3.sh` | E3 contract | Requires managed-residue guarantee, E3 DoD, and 2a.12–2a.17 amendment phrases | normative content mixed with historical amendment presence | Reconstruct invariants from current requirements; executable residue/crash/ownership tests must not depend on historical amendment wording |
| `validate-security-e0-e3-ownership-2a16.sh` | E3 contract | Requires “2a.16 canonical plaintext ownership amendment” and “No Rustnote-managed plaintext file may be created, copied, renamed, or materialized…” | legitimate invariant, historical phrase oracle | Property/integration tests around ownership authority and filesystem lifecycle |
| `validate-security-e0-e3-residue-2a17.sh` | E3 contract | Requires 2a.17 amendment, durable backup privacy obligations and attachment orphan quarantine | legitimate invariants, phrase oracle | Crash/restart/residue/backup/quarantine tests |
| same | `SECURITY_E0_E3_REMEDIATION_2A17.md` | Requires report headings: Backup privacy-obligation model, Attachment quarantine, Portability/import staging, Scope boundary | historical phrase-presence | Remove |
| `validate-security-e0-e3-closure.sh` | `SECURITY_E0_E3_REMEDIATION_2A15.md` | Requires Lock cleanup ownership; protected attachment materialization ownership; emergency-journal pre-quarantine privacy; Targeted QA | historical phrase-presence | Remove; source/runtime tests already protect the substantive invariants |
| same | `SECURITY_E0_E3_REMEDIATION_2A16.md` | Requires attachment `.incoming`, image `.ready -> .claimed`, portability/import, Protect/backup/startup/deletion barriers, namespace audit, targeted regressions headings | historical phrase-presence | Remove |
| same | `SECURITY_E0_E3_REMEDIATION_2A17.md` | Requires backup privacy model, removable-volume disappearance, attachment quarantine, portability/import staging, targeted regressions, scope boundary | historical phrase-presence | Remove |
| same | `SECURITY_E0_E3_REMEDIATION_2A18.md`, `SECURITY_E0_E3_CLOSURE_2A19.md`, `QA_SECURITY_E0_E3_REMEDIATION_2A17.md`, `QA_SECURITY_E0_E3_REMEDIATION_2A18.md`, `QA_SECURITY_E0_E3_CLOSURE_2A19.md` | File existence only | historical/provenance-only | Remove after equivalent current tests are proven |

The closure script also invokes E0–E3 and the 2a.16–2a.19 validators, so prose coupling propagates transitively.

### Vault/privacy historical records

| Validator | Markdown input | Exact assertion | Classification | Replacement |
| --- | --- | --- | --- | --- |
| `validate-vault-2b1-foundation.sh` | `VAULT_2B1_AUDIT_PLAN.md` | Requires Current note private state; Private metadata; Current attachment metadata; Revision history; Revision attachment references; Blob mapping; Public structural state; Explicit pass boundary | historical audit-plan phrase-presence | Current object-format/model tests + structural type/field assertions where necessary |
| `validate-vault-2b8-plaintext-authority-closure.sh` | `VAULT_2B8_PLAINTEXT_AUTHORITY_CLOSURE_PLAN.md` | File existence only | historical/provenance-only | Remove |
| `validate-vault-2b11-atomic-privacy-authority-closure.sh` | `VAULT_2B11_ATOMIC_PRIVACY_AUTHORITY_CLOSURE.md`, `QA_VAULT_2B11_CLOSURE.md` | File existence only | historical/provenance-only | Remove |
| `validate-vault-2b12-aegis-privacy-disclosure-closure.sh` | `VAULT_2B12_AEGIS_PRIVACY_DISCLOSURE_CLOSURE.md`, `QA_VAULT_2B12_CLOSURE.md` | File existence only | historical/provenance-only | Remove |
| same | E0 contract | Requires the 2b.12 AEGIS-256 product-decision override, full 256-bit AEGIS profile, benchmark-as-diagnostic wording, and fixed 64 MiB / 3 pass / 4 lane Argon2id policy | legitimate policy, phrase oracle | Test fixed constants/types and reject alternative production primitives |
| same | E1 contract | Requires `Argon2idParams::V1_INTERACTIVE`, “64 MiB, time cost 3, lanes 4”, “same v1 resource profile” | legitimate policy, phrase oracle | Direct KDF profile tests |
| `validate-vault-2b42-8-no-vacuum-forensic.sh` | `VAULT_2B42_8_NO_VACUUM_FORENSIC.md` | Requires “Native decision result: PASS”, production removal of per-Protect VACUUM, secure_delete + FTS5 rebuild + verified WAL truncate | historical decision phrase-presence | Keep actual database/WAL/backup forensic regression; remove report oracle |
| same | `QA_VAULT_2B42_8_NO_VACUUM_FORENSIC.md` | Requires native macOS PASS, VACUUM removed, final native closure gate | historical evidence phrase-presence | Remove; Git/CI evidence is provenance |
| `validate-vault-2b42-9-cleanup.sh` | `VAULT_2B42_9_CLEANUP.md` | Requires behavior-preserving cleanup, no backward-compat migration, crash recovery preserved | historical decision phrase-presence | Existing source/tests protect current architecture; remove prose check |
| same | `QA_VAULT_2B42_9_CLEANUP.md` | Requires dependency-free QA, native gate pending, 2b.42.10 sequencing phrase | historical/provenance-only | Remove |

### Native-compile family: exact 81-file dependency

The following 27 validators each require exactly three same-pass historical Markdown files:

`2b.13, 2b.14, 2b.15, 2b.16, 2b.17, 2b.18, 2b.19, 2b.20, 2b.21, 2b.24, 2b.25, 2b.26, 2b.27, 2b.28, 2b.29, 2b.30, 2b.31, 2b.32, 2b.33, 2b.34, 2b.35, 2b.36, 2b.37, 2b.38, 2b.39, 2b.40, 2b.41`.

For each pass `X`, the direct dependency is:

- `NATIVE_COMPILE_X_PLAN.md` — existence only;
- `NATIVE_COMPILE_X_IMPLEMENTATION.md` — existence only;
- `QA_NATIVE_COMPILE_X.md` — existence only.

No content from those 81 files is used to establish the pass-specific source assertions. Their presence is therefore pure compatibility/provenance coupling.

One exception adds a separate normative prose dependency: `validate-vault-2b20-startup-test-isolation-privacy-order-closure.sh` also reads E3 and requires the phrases “Lock first revokes future Vault reads and then waits for all permit-bearing operations to end” and “Whole-library producer revocation.” The ordering requirement is legitimate; prose is not the correct executable oracle.

## Indirect dependency and chaining graph

Direct document readers are only part of the problem. Chaining spreads those dependencies through otherwise document-free validators.

### Native chain

The native historical series is predecessor-linked from 2b.41 back through 2b.13. The early part is especially duplicative:

- 2b.13 invokes 2b.12, 2b.11 and four inherited security closure validators.
- 2b.14 invokes 2b.13 and then directly invokes that same six-validator inherited set again.
- 2b.15 invokes 2b.14, 2b.13 and the same inherited set again.
- 2b.16 onward mostly invokes the immediately preceding native validator.
- 2b.29/2b.30 also invoke deployment-target validators.
- Selected aggregate runners suppress some chaining with `RUSTNOTE_VALIDATOR_NO_CHAIN=1`, but standalone historical validators retain the recursive graph.

By the static call graph, a default standalone 2b.41 run expands to at least 54 historical closure-validator process invocations before ancillary deployment validators. Several inherited security/Vault validators are reached four times through the 2b.13–2b.15 fan-in. This is duplicated work, not stronger coverage.

### Architecture chain

The A0–A7 and B1–B6 validators form another pass-numbered inheritance graph. B2 explicitly invokes A1, A2, A3, A4, A1–A4 closure, A5, A6, A7 and B1 in static modes; later B validators invoke predecessors again. These scripts often inspect other validator source text in addition to executing predecessors. The useful architectural rules should survive, but the pass-history topology should not.

### Current CI duplication

`scripts/ci-source-checks.sh` directly runs backup validators 2b43.1, 2b43.2 and 2b43.3. The 2b43.3 validator itself invokes 2b43.1 and 2b43.2, so both contract validators run twice in the current source-check job.

`validate-vault-2b42-final-pre-s1-closure.sh` is a large historical orchestrator. It deliberately disables some recursive chains, which proves that the repository already recognizes chaining as a runtime problem; Q5 should replace this special-case suppression with a no-chaining architecture by construction.

## Weak versus legitimate checks

### Remove from executable QA

These have no defensible role as release/security gates:

1. Historical Markdown file-existence checks.
2. PASS/PARTIAL wording checks in QA reports.
3. Presence of pass-history headings or implementation phrases in historical reports.
4. README completeness checks used as architecture gates.
5. Assertions that merely prove a prior audit document still says what it said when written.
6. Pass-number/build-history compatibility lists whose only purpose is to keep old validators runnable.

Historical Git and `docs/qa/HISTORICAL_EVIDENCE.md` are the provenance archive.

### Keep the requirement, replace the oracle

These requirements are legitimate but should no longer be proved by Markdown:

- crypto/KDF policy and exact tag/key/nonce sizes;
- byte-format versions, magic values, field sizes and canonical vectors;
- key-loss/fail-closed behavior;
- repository/library context authentication;
- plaintext ownership and lifetime;
- publication/revocation ordering;
- backup publication authority;
- SQLite/FTS/WAL residue behavior;
- crash/restart and cleanup ownership.

Preferred evidence, in order:

1. executable Rust/JS unit and integration tests;
2. deterministic property/negative/vector tests;
3. concurrency/linearization/crash tests;
4. native OS integration tests for Keychain/Keystore/filesystem boundaries;
5. narrow structural source checks only where a runtime test is impractical.

### Legitimate machine-readable specification

Markdown is not the right machine specification. The editor already demonstrates the better pattern with `editor-spec/document-spec.json` and `editor-spec/validation-cases.json`: structured data is consumed by validators while README prose explains it. Backup byte-format vectors may use an equivalent explicit fixture only if executable code cannot itself be the canonical source.

## Runtime and bloat assessment

The latest successful baseline CI run on `main`, run 24 / Actions run `43675019947`, used commit `d96fe9c722561fc46b79e552583b151f5b095442`.

Observed hosted-run durations:

| Surface | Duration |
| --- | ---: |
| Source + browser contracts job | about 10 s |
| Current source validator step | about 2 s |
| Full browser behavior step | about 1 s |
| Native macOS job | about 11 min 57 s |
| Clippy | about 1 min 21 s |
| Rust tests | about 2 min 29 s |
| Release build | about 7 min 2 s |

Therefore two different performance problems must not be conflated:

- **Current CI wall time** is dominated by native compilation/release build, not by the 100 Markdown fixtures.
- **Repository/manual QA bloat** comes from 86 pass-oriented validators, repeated source scans/Python startup, recursive predecessor invocation and historical prose gates.

Q5 should optimize the latter without weakening the former. Native compilation should be optimized separately only if profiling shows a real build-system opportunity.

## Target QA architecture

QA should be organized by invariant domain, with one-way composition:

```
qa-release
  ├── qa-toolchain
  ├── qa-editor-browser
  ├── qa-security-privacy
  ├── qa-backup-format
  └── qa-architecture
```

Rules:

- Leaf suites never invoke peer or predecessor suites.
- Only top-level orchestrators compose suites.
- A protected invariant has one primary executable owner; other suites may depend on its result, not re-run its implementation.
- Historical pass numbers do not appear in current suite names.
- Documentation is explanatory/normative, never an executable dependency.
- Structural source checks are small, named rules, not snapshots of implementation phrasing.
- Expensive native commands run once per required feature/profile matrix.
- Every suite can run independently.

### Proposed suites

**qa-toolchain**

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features --locked`
- strict Clippy with the intended feature matrix
- workspace tests with intended feature matrix
- release build
- macOS deployment-target/artifact checks

This also closes the currently documented CI gap where `internal-test-support` tests are not covered by configured Clippy/test commands.

**qa-editor-browser**

- document-spec JSON validation/corpus;
- editor behavior;
- PDF behavior/layout/runtime integrity;
- bridge/UI behavior;
- dependency-free and dependency-backed modes without source duplication.

**qa-security-privacy**

- Vault crypto tests;
- plaintext ownership lifecycle;
- privacy publication/revocation linearization;
- lock/read races;
- stale async publication;
- protected attachment/revision/autosave lifetime;
- crash/restart/recovery;
- SQLite/FTS/WAL/backup residue;
- Keychain/Keystore native integration as a separate native gate.

**qa-backup-format**

- canonical v3 constants and encoder/parser;
- round-trip and exact golden bytes;
- reserved/unknown/noncanonical rejection;
- AAD/context vectors;
- object-ID HMAC vectors;
- keyset independence;
- missing/corrupt/unavailable root fail-closed tests;
- snapshot publication authority and index reconstruction;
- no plaintext fallback.

**qa-architecture**

Keep only invariants that are genuinely structural, such as:

- crypto primitive isolation;
- no filesystem/environment root-key fallback;
- no obsolete global privacy-publication APIs;
- no dead-code suppression policy violations;
- module/capability boundaries that cannot be expressed naturally through behavior tests.

**qa-release**

Runs each required leaf suite once and records environment/revision/results. It contains no invariant logic itself.

## Migration sequence and acceptance gates

### Q2 — native-compile decoupling

1. Inventory the unique substantive assertion in each 2b.13–2b.41 validator.
2. Classify each assertion into toolchain, architecture, editor/browser, security/privacy or obsolete compiler-fix history.
3. Move behavior to executable tests; retain only narrow structural rules that still express a current invariant.
4. Add the canonical toolchain suite with the intended feature matrix.
5. Run historical native validators and replacements side-by-side and produce an invariant-level equivalence table.
6. Remove all 81 native-pass document dependencies from validators.
7. Prove zero references to the 54 `NATIVE_COMPILE_*_{PLAN,IMPLEMENTATION}.md` files and delete those 54.
8. Leave the 27 `QA_NATIVE_COMPILE_*.md` deletion to Q4 so QA-report removal happens as one controlled family.

Acceptance: every current substantive native invariant has one equal-or-stronger owner; compiler-fix/provenance-only assertions are explicitly classified obsolete rather than silently lost.

### Q3 — Vault/privacy/security historical decoupling

1. Reconstruct current E0–E3 and Vault privacy invariants from normative requirements and implementation, not historical reports.
2. Replace report headings with ownership, residue, publication, concurrency, recovery and failure tests.
3. Move contract-derived constants/policies into direct executable assertions; keep E0–E3 contracts human normative.
4. Run old and new coverage side-by-side, including adversarial and native-sensitive cases.
5. Zero-reference and delete the 11 historical `SECURITY_E0_E3_*` / `VAULT_*` compatibility records.

Acceptance: no security/privacy invariant is supported only by source phrase presence or a historical document.

### Q4 — QA-report decoupling

1. Remove remaining executable dependencies on all 35 `QA_*.md` files.
2. For backup 2b43.1, replace PASS/contract-only wording with tests against implementation/format authority.
3. For 2b42.8/2b42.9, retain the actual forensic/cleanup regressions but remove evidence-record phrase checks.
4. Verify historical evidence remains available through the consolidated ledger and immutable Git.
5. Zero-reference and delete all 35 root QA reports.

Acceptance: no executable path reads a QA report.

### Q5 — suite architecture cleanup

1. Replace pass-numbered predecessor chains with the named concern suites above.
2. Remove validator-source self-inspection when the same rule can be expressed directly.
3. Ensure leaf suites never call each other.
4. Deduplicate source scans, Python/Node startup and repeated native commands.
5. Measure standalone and aggregate runtime before/after.
6. Delete obsolete historical wrapper validators once their unique substantive assertions have owners.

Acceptance: one top-level orchestrator invokes each leaf suite once; no historical pass chain remains.

### Q6 — independent closure audit

Start de novo from the governing editor, security/privacy and backup requirements, treating the new QA implementation as untrusted.

Required closure evidence:

- requirement -> invariant -> executable test/rule traceability;
- direct review of plaintext creation/ownership, storage writers, publication paths, async completion, caches, attachments, revisions, FTS/WAL, backup/restore and shutdown;
- canonical backup byte-format and key-loss/failure tests;
- editor/browser behavior and native toolchain/release gates;
- adversarial concurrency/crash/restart cases;
- zero executable Markdown dependencies except an explicitly approved machine-readable specification (Markdown should normally be zero).

Acceptance: streamlined QA is demonstrably equal or stronger than the old suite on current invariants, with substantially less historical topology.

## Q1 objective assessment

| Objective | Result | Evidence |
| --- | --- | --- |
| Audit current QA surface de novo | PASS | Full `scripts/`, `tests/`, workflow and recursive repository inventory inspected |
| Map validator/test/script -> Markdown dependencies | PASS | 43 direct validator readers; no direct test/non-validator-script readers; 107 distinct Markdown inputs classified |
| Identify exact historical compatibility burden | PASS | 54 native + 35 QA + 11 Vault/security = 100; 132 -> 32 Markdown endpoint validated |
| Distinguish legitimate vs weak links | PASS | Provenance/existence/phrase checks separated from current security/format invariants |
| Identify validator chaining/duplication | PASS | Native, architecture, final-closure and backup-CI chaining audited |
| Assess runtime/bloat | PASS | Repository-size metrics plus current successful Actions run timing separated into CI vs legacy-suite costs |
| Produce concern-based target architecture | PASS | Five leaf suites + release orchestrator, no peer/predecessor chaining |
| Produce safe Q2–Q6 migration sequence | PASS | Side-by-side equivalence and zero-reference gates defined per family |
| Delete historical Markdown in Q1 | PASS (intentionally not done) | Q1 constraint preserved |
| Make production/QA behavior changes in Q1 | PASS (none) | This audit document is the only intended repository change |
| Preserve `main` and `legacy` | PASS at audit baseline | No write is required or intended on either branch |

## Q1 conclusion

The proposed decoupling is justified and bounded. The central problem is not that documentation exists; it is that historical prose has become an executable compatibility surface.

The safe migration rule for Q2 onward is:

```
identify current invariant
        ↓
assign executable owner
        ↓
run old + new evidence side-by-side
        ↓
prove replacement equal or stronger
        ↓
remove document dependency
        ↓
prove zero references
        ↓
delete historical fixture
```

Q2 should begin with the native-compile family because its 81 direct document dependencies are overwhelmingly existence-only and its substantive checks can be separated cleanly from historical pass provenance.

# Vault and E0–E3 implementation evolution

This is the **compressed historical record** for the Vault/security implementation series. It preserves decisions, defects, supersessions and evidence boundaries while allowing the dozens of pass-specific Markdown files to leave the working tree. Git history remains the immutable source for deleted originals.

For current architecture use [VAULT_ARCHITECTURE.md](VAULT_ARCHITECTURE.md); for lifetime rules use [PLAINTEXT_LIFETIME.md](PLAINTEXT_LIFETIME.md); for normative cryptographic/security contracts use [the contract index](../contracts/README.md).

## 1. E0–E3 closure and remediation: 2a.6–2a.19

The 2a sequence converted selective encryption from a basic feature into a recoverable managed-plaintext security system.

### 2a.6–2a.9 — transition durability, Lock revocation and UI residue

- **2a.6:** introduced the durable `vault_operations` crash journal; autosave/emergency-journal barrier; safe backup-parent ownership; password zeroization; multi-process temp safety.
- **2a.7:** made Lock a counted synchronous revocation barrier; required Vault permits for complete protected plaintext-use lifetimes; private 0700/0600 staging; structural protected-payload validation; documented external-egress/metadata limits.
- **2a.8:** removed cached plaintext PDF descriptors and bound media file handles to permits; hardened converter-created private files.
- **2a.9:** scrubbed protected-only persisted tag-filter labels and live tag catalogs; introduced the corresponding migration scrub.

### 2a.10–2a.14 — forensic cleanup and adversarial lifecycle

- **2a.10:** added FTS5 rebuild, snapshot fail-closed barrier for pending Vault operations and attachment-placeholder schema repair. Its VACUUM-based forensic method was later superseded by 2b.42.8.
- **2a.11:** tightened read-publication races, introduced OS-lock-backed sensitive-temp session ownership and durable backup-location history.
- **2a.12:** added the note-scoped plaintext-derivative transition barrier, fail-closed cache/staging/handoff cleanup and stronger temp/key/logging hygiene.
- **2a.13:** exercised the four crash states, failed cleanup/restart, cycles, attachment races and stale completion as a bounded adversarial matrix.
- **2a.14:** independent source audit found three real defects: PDF annotation response lease lifetime, fail-open protected emergency-journal deletion, and quarantine residue. All were repaired without adding a new architecture.

### 2a.15–2a.19 — canonical ownership and UI lifetime

- **2a.15:** retained Lock/session cleanup ownership after deletion failure, registered protected materialization before file creation, and strengthened protected-journal pre-quarantine deletion.
- **2a.16:** generalized I1/I2 into canonical plaintext ownership. Durable SQLite staging owns library-resident plaintext before write; process-private derivative/restore sessions use crash-recoverable process ownership. Ownership transfer and cleanup became transactional/fail-closed.
- **2a.17:** added durable privacy-reset obligations for offline/remembered backup repositories and strict attachment orphan-quarantine cleanup.
- **2a.18:** added UI privacy-generation invalidation, permit-bearing protected attachment commands/revision previews and protected revision restore.
- **2a.19:** independent adversarial closure found restore ownership gaps, incomplete library-wide producer/Vault revocation, protected revision authorization ending too early, and lock-order/conflicting-transition races. Repairs established the transition/restore ordering later consolidated by 2b.

**Supersession:** 2a.18's single global UI privacy epoch and the monolithic protected payload were later replaced by scoped publication and granular protected objects. Their security objectives remain; their specific implementation is historical.

## 2. 2b.1–2b.4 — granular Vault architecture

### 2b.1 — default-private specification and object-store foundation

Introduced:

- `SensitiveNoteData` / explicit public protected stub;
- `VaultManifest`, `NotePrivateState`, `MetadataState`, `AttachmentIndex`, `RevisionObject`;
- opaque authenticated object IDs and typed object contexts;
- `vault_objects` + `vault_note_roots`;
- atomic root publication.

A first draft made manifest work scale with full revision history; it was rejected and replaced by a predecessor-chain representation before release. 2b.1 was deliberately a **non-production foundation**: the 2a.19 monolithic payload still remained canonical until 2b.4.

### 2b.2 — PrivacyCoordinator + unified plaintext authority

Centralized:

- transition admission/conflicts;
- derivative gating;
- Vault reader revocation;
- storage mutation serialization;
- durable plaintext transfer recovery.

Early multi-note gating and restore-transfer drafts exposed ordering/recovery defects; the pass corrected them before closure. It also restricted low-level transition and durable-owner construction so callers could not reconstruct security lock order.

### 2b.3 — publication-token consolidation

Replaced scattered UI generation/Vault/manual checks with `PrivacyPublicationToken` and coordinator-backed publication authorization/leases. The audit found and corrected:

- UI-generation-only tokens missing the storage-admission interval;
- invalid predictive generation tokens under cross-note concurrency;
- stale publication surfaces and native dialog lifetimes.

RTF import/export was also removed as a separate requested cleanup.

### 2b.4 — granular protected-note cutover

Made `vault_note_roots + vault_objects` the sole production protected-note representation (schema 21), removed the monolithic steady-state store and changed common operations to selective object opens. Protected typing/autosave no longer hydrates full graph/history.

A subsequent revision-Restore audit found that a pre-Restore publication token was necessarily stale after Restore admission. The remediation introduced a semantic note-publication scope and two-token Restore flow: pre-transition authorization is discarded; post-transition publication captures fresh coordinator authorization while retaining the original UI/library/note context.

### Final 2b.1–2b.4 objective closure

A later de-novo audit found remaining caller leakage:

- protected mutation callers still knew Vault-reader → storage-lock order;
- several async publication gaps remained;
- protected tag mutations still routed through ordinary plaintext relations;
- Restore still leaked serialization knowledge.

The closure introduced coordinator-owned protected mutation guards, routed protected tags through encrypted metadata and completed publication coverage without adding another architecture.

## 3. 2b.5–2b.12 — consolidation and invariant repair

### 2b.5 — deletion/performance consolidation

Removed dead APIs, obsolete temp-layout/PID compatibility and broad export machinery; folded attachment-ingest cleanup into typed `OwnedPlaintext`; removed duplicate protected Unlock graph decryption and library-wide scanning from single-note Markdown export. Historical backup proof logic was intentionally retained because it still protected E3 cleanup.

### 2b.6 — independent audit

Found two substantive defects:

1. `Path::exists()` could treat metadata errors as absence and retire ownership.
2. encrypted-object retirement still accepted caller-influenced deletion provenance.

Repairs introduced conclusive NotFound-only absence and made the object-store commit authority derive superseded mutable objects from the authenticated old/new manifests itself.

### 2b.7 — closure repair

Found that some transition-owned code re-entered public admission-aware readers and that protected revision Restore could queue behind conflicting transitions. It introduced transition-safe internal reads, fail-closed protected Restore admission, encrypted protected link reads, centralized canonical-blob cleanup ownership, typed quarantine transfer and safer destructive revision/attachment cleanup.

### 2b.8 — plaintext authority closure

Consolidated three remaining plaintext families:

- external atomic output through durable recovery + `OwnedPlaintext`;
- emergency journal under an external durable namespace authority;
- persistent ordinary image/media derivative namespaces.

Also hardened namespace subtree deletion against symlink/containment problems.

### 2b.9 — backup ownership closure

Corrected a critical ownership-design problem: the backup owner marker had lived **inside** the recursively deleted repository. It moved to an external sibling marker bound to library identity so cleanup authority survives partial deletion. Privacy-reset obligation retires only after strict deletion, durable sync, clean recreation and absence proof.

The one-way 2b.8 inline-owner adoption path was transitional and was later removed by 2b.42.9.

### 2b.10 — cross-cutting invariant closure

Closed three issues:

- fresh durable namespace establishment must not recreate authority around a pre-existing nonempty tree;
- protected global status/toast publication must be token/lease-bound and vanish on revocation;
- remote-sync queue creation must reject protected note/attachment payloads at the storage boundary.

### 2b.11 — atomic privacy authority

Eliminated a derivative-permit → Vault-reader race and partial multi-note reader acquisition by issuing compound authorization at one coordinator linearization point. Full-lifetime attachment/PDF/media/export flows now retain one compound permit.

Introduced the encrypted `RevisionIndex` and direct retained-revision resolution, replacing hot-path predecessor history walks. Development Vault formats advanced accordingly.

### 2b.12 — fixed crypto profile and backup privacy disclosure

Made the AEGIS-256 product choice explicit and non-benchmark-gated, hardened Argon2id hostile-envelope bounds to the actual v1 profile and added post-Protect disclosure of pending backup privacy-reset obligations without turning a post-commit informational lookup failure into a false encryption failure.

## 4. 2b.42 — scoped publication and final architecture cleanup

### 2b.42.1–2b.42.4 — publication scope

Replaced global publication invalidation with:

- per-note publication generation;
- multi-note/aggregate generation;
- whole-library generation.

A transition for B invalidates B-local and aggregate surfaces without invalidating unrelated A-local publication. Navigation now separates source authorization from target authorization. Note-local and aggregate caches were systematically classified and migrated.

### 2b.42.5 — Vault action controller

Vault actions operate on the target note rather than forcing navigation. The controller synchronizes the target's latest editor state, opens note-local fail-closed UI revocation, executes the canonical transition, then publishes fresh canonical target state only if selection context is still valid. A post-audit race where unselected B could become selected/edited during checkpoint was fixed by making synchronization loop to a stable checkpoint boundary.

### 2b.42.6 — whole-library operations

Introduced `WholeLibraryOperationLease` separate from UI publication. Backup/archive/manual durability synchronize the current source first and then obtain whole-library snapshot authority. Shutdown deliberately runs `checkpoint_all` **before** the whole-library lease to avoid note-save/transition deadlock. Restore remains an exclusive library transition rather than acquiring a read-side library lease.

Notebook portability uses a narrower multi-note plaintext permit and revalidates membership under storage serialization.

### 2b.42.7–2b.42.8 — measurement and no-VACUUM decision

Opt-in profiling measured security stages without logging user-derived data. It showed per-Protect VACUUM was only a small cost.

2b.42.8 then tested the actual forensic question. A native macOS regression reported that unique historical/current markers were absent from DB/WAL and subsequent managed backup after:

```text
secure_delete + FTS5 rebuild + verified WAL TRUNCATE
```

The production per-Protect VACUUM and duplicate second checkpoint were removed. This was a security-method simplification, not a performance-driven weakening. Later full native closure remained a separate gate.

### 2b.42.9 — cleanup

Removed obsolete development compatibility and duplicate ownership/publication machinery:

- no production dead-code suppression for completed Vault work;
- external backup owner marker only;
- no inline owner/adoption mode;
- current backup-history format only;
- scoped publication only;
- no stale pass-number implementation branches.

### 2b.42.10 — independent closure

A de-novo audit found one remaining defect class: ambiguous filesystem observation still survived outside the earlier core `OwnedPlaintext` paths. It replaced sensitive boolean-existence decisions with the shared strict `managed_path_is_present` rule across Protect rollback, Vault materialization, attachment ingest/operation, native staging/handoff, Restore, portable bundles, journal and temp-owner arbitration. A second temp-owner metadata ambiguity was also repaired. One dead unauthorized façade was removed.

The report concluded source-level I1–I7 and E0–E3 PASS **after repairs**, but explicitly left native execution pending.

### 2b.42.11 — final static/package QA

No production defect was found. Several inherited validators were stale and were updated to assert the scoped/current architecture rather than force production code back toward superseded behavior. The pass recorded static/source/package closure but still required native macOS formatting, build, strict Clippy, full Rust tests, dependency-backed browser tests, profiling and manual adversarial checks before final freeze.

## 5. Current supersession map

| Earlier mechanism | Current canonical mechanism |
| --- | --- |
| Monolithic `ProtectedNotePayload` | Granular `vault_note_roots + vault_objects` graph |
| Revision predecessor-chain traversal for normal lookup | Encrypted `RevisionIndex` + direct selected-object resolution |
| Feature-local privacy lock choreography | `PrivacyCoordinator` semantic guards/permits |
| Single global UI privacy epoch | library + multi-note + per-note scoped UI/coordinator publication |
| Feature-local generation recipes | semantic `PrivacyPublicationToken` / note publication scope |
| Cached plaintext PDF file handles | request-scoped handle under live resource permit |
| Raw managed plaintext paths/cleanup convention | `OwnedPlaintext` / durable namespace / process-session owners |
| Boolean `Path::exists()` at ownership retirement | strict no-follow presence; only NotFound proves absence |
| Backup owner inside mutable repository | external library-bound durable namespace marker |
| Per-Protect `VACUUM` | secure-delete + FTS rebuild + verified WAL TRUNCATE |
| Legacy PID/temp-layout compatibility | current OS-lock-owned `session-v2` process ownership |
| Benchmark-dependent cipher choice | fixed AEGIS-256 v1 profile |

## 6. Legacy Batch 3 source disposition

The following **11 files remain at the repository root only because current validator scripts read them directly**. Their durable technical information is consolidated above. Removing them belongs to the separate QA-refinement effort, not documentation consolidation:

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

All other Batch 3 root-level `VAULT_*.md` and noncontract `SECURITY_E0_E3_*.md` records are superseded by this consolidated history/current architecture and are removed from the working branch. Their exact original bytes remain available in the frozen Git baseline; see the [archive/no-loss map](../ARCHIVE.md).

## 7. Evidence discipline

Historical reports repeatedly distinguish **source/static closure** from native executable verification. Keep that distinction. A report that says a validator passed is evidence for that revision and environment, not automatic proof for the current branch.

The consolidated current architecture should be checked against current source, while old defect narratives are retained here only when they explain a durable rule, supersession or known evidence boundary.

## 8. Complete 51-file immutable ledger

| Original | Working-tree disposition |
| --- | --- |
| [`SECURITY_E0_E3_CLOSURE_2A6.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A6.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A7.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A7.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A8.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A8.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A9.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A9.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A10.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A10.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A11.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A11.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A12.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A12.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A13.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A13.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A14.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A14.md) | Git-only after consolidation |
| [`SECURITY_E0_E3_CLOSURE_2A19.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_CLOSURE_2A19.md) | **Retained temporarily — validator exact-path dependency** |
| [`SECURITY_E0_E3_REMEDIATION_2A15.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A15.md) | **Retained temporarily — validator exact-path dependency** |
| [`SECURITY_E0_E3_REMEDIATION_2A16.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A16.md) | **Retained temporarily — validator exact-path dependency** |
| [`SECURITY_E0_E3_REMEDIATION_2A17.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A17.md) | **Retained temporarily — validator exact-path dependency** |
| [`SECURITY_E0_E3_REMEDIATION_2A18.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/SECURITY_E0_E3_REMEDIATION_2A18.md) | **Retained temporarily — validator exact-path dependency** |
| [`VAULT_2B1_2B4_FINAL_CLOSURE_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_2B4_FINAL_CLOSURE_AUDIT_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_2B4_FINAL_OBJECTIVE_CLOSURE.md) | Git-only after consolidation |
| [`VAULT_2B1_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_AUDIT_PLAN.md) | **Retained temporarily — validator exact-path dependency** |
| [`VAULT_2B1_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B1_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B2_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B2_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B3_2B4_REVISION_RESTORE_PUBLICATION_REMEDIATION.md) | Git-only after consolidation |
| [`VAULT_2B3_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B3_AUDIT_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B3_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B3_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B4_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B4_AUDIT_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B4_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B4_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B5_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B5_AUDIT_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B5_FINAL_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B5_FINAL_AUDIT_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B5_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B5_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B6_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B6_AUDIT_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B6_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B6_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B7_CLOSURE_AUDIT_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B7_CLOSURE_AUDIT_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B7_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B7_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B8_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B8_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B8_PLAINTEXT_AUTHORITY_CLOSURE_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B8_PLAINTEXT_AUTHORITY_CLOSURE_PLAN.md) | **Retained temporarily — validator exact-path dependency** |
| [`VAULT_2B9_BACKUP_OWNERSHIP_CLOSURE_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B9_BACKUP_OWNERSHIP_CLOSURE_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B9_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B9_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B10_CROSS_CUTTING_INVARIANT_CLOSURE_PLAN.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B10_CROSS_CUTTING_INVARIANT_CLOSURE_PLAN.md) | Git-only after consolidation |
| [`VAULT_2B10_IMPLEMENTATION.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B10_IMPLEMENTATION.md) | Git-only after consolidation |
| [`VAULT_2B11_ATOMIC_PRIVACY_AUTHORITY_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B11_ATOMIC_PRIVACY_AUTHORITY_CLOSURE.md) | **Retained temporarily — validator exact-path dependency** |
| [`VAULT_2B12_AEGIS_PRIVACY_DISCLOSURE_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B12_AEGIS_PRIVACY_DISCLOSURE_CLOSURE.md) | **Retained temporarily — validator exact-path dependency** |
| [`VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md) | Git-only after consolidation |
| [`VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_2_NAVIGATION_SOURCE_TARGET.md) | Git-only after consolidation |
| [`VAULT_2B42_3_NOTE_LOCAL_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_3_NOTE_LOCAL_UI.md) | Git-only after consolidation |
| [`VAULT_2B42_4_MULTI_NOTE_UI.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_4_MULTI_NOTE_UI.md) | Git-only after consolidation |
| [`VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_5_VAULT_ACTION_CONTROLLER.md) | Git-only after consolidation |
| [`VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_6_LIBRARY_WIDE_OPERATIONS.md) | Git-only after consolidation |
| [`VAULT_2B42_7_PROFILING.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_7_PROFILING.md) | Git-only after consolidation |
| [`VAULT_2B42_8_NO_VACUUM_FORENSIC.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_8_NO_VACUUM_FORENSIC.md) | **Retained temporarily — validator exact-path dependency** |
| [`VAULT_2B42_9_CLEANUP.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_9_CLEANUP.md) | **Retained temporarily — validator exact-path dependency** |
| [`VAULT_2B42_10_INDEPENDENT_CLOSURE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_10_INDEPENDENT_CLOSURE.md) | Git-only after consolidation |
| [`VAULT_2B42_10_WIP_HANDOFF.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_10_WIP_HANDOFF.md) | Git-only after consolidation |
| [`VAULT_2B42_11_FINAL_QA_PACKAGE.md`](https://github.com/oneshubhamt/birch/blob/514b797fb0a6a128050cf75ac2428fc12095106a/VAULT_2B42_11_FINAL_QA_PACKAGE.md) | Git-only after consolidation |

# Batch 2B — E2/E3 selective-encryption and privacy-residue authority

**Nature:** Source-to-clause map and amendment reconciliation only. The original contracts are immutable and remain normative for their unchanged requirements; later revisions must be read alongside them. This is **not** a full functional/forensic certification.

## Preserved E2 model

- Every protected note owns a random 256-bit note key, wrapped by an Argon2id password envelope. The original authenticated note payload encloses title/editor/plain-text, tags, citations/sources, outgoing links, PDF annotations, revision snapshots, original attachment metadata and protected blob mappings.
- While protected, ordinary SQLite retains only explicitly permitted structural/routing information. Sensitive note/title/editor/text fields, persistent FTS, tag relationships, plaintext revisions, note-owned citations, PDF annotation rows and outgoing links are excluded. The original attachment bytes and identifying metadata are stored in protected blobs rather than ordinary placeholders.
- Protect does **not** re-encrypt a shared ordinary blob in place. Exclusive plaintext cleanup and protected-ciphertext retention/index ownership must be atomic and recoverable.
- The initial E2 feature treated unlocked protected notes as **read-only**, but the later **2a.18 protected-revision-restore amendment** authorizes a narrowly scoped authenticated restore through Vault-aware resealing. Later granular protected-edit designs must be interpreted as subsequent amendments, **not** retroactively as E2's original behavior.
- Protected content stays out of persistent FTS even when unlocked; ordinary plaintext exports are blocked rather than silently producing placeholders. Complete archives preserve the ciphertext form. Persistent predecessor forensic residue, caches and pre-encryption automatic backup history require the separate E3 closure.
- E2's explicit definition of done includes negative wrong-password/tamper tests, attachment sharing/integrity, read-only gate, export gate, unchanged ordinary-note behavior and E1 crypto isolation.

## Preserved E3 managed-residue invariants

E3 expanded protection from logical SQLite rows to **Rustnote-managed plaintext equivalents** in WAL/free pages, historical FTS segments, emergency/autosave journal, automatic and safety backups (including disconnected remembered locations), caches, renderings, OS handoff, image/PDF staging, decrypted PDF sessions, attachment quarantine, restore materializations, and stale UI publication.

The E3 amendments are **cumulative** security requirements. Do not erase an older negative-path invariant merely because the latest report implements it differently:

- **2a.7–2a.9:** Atomic Lock read-permit revocation/drain, private Unix `0700` directories and `0600` files, precise unprotect pre/post-commit staging, no recovery of external application copies, no stale descriptors, scrubbed persisted tag filters.
- **2a.10–2a.11:** FTS5 forensic rebuild and migration marker v4; pending-vault-operation snapshot barrier; attachment placeholders obey schema constraints; revocable payload read, UI selection/list invalidation, OS-lock-backed process-session owners and durable backup-location history.
- **2a.12–2a.14:** Note-scoped derivative producer drains and fail-closed cleanup; privacy marker v5; adversity matrix; PDF annotation guard scope, strict protected emergency-journal startup deletion and quarantine handling.
- **2a.15–2a.17:** Cleanup authority must remain live on failed file deletion; register materializations **before** creating plaintext; pre-quarantine journal sweep; canonical `OwnedPlaintext` or durable SQLite staging owner **before first byte**, transactional ownership transfer, offline backup privacy-reset obligations and orphan-quarantine cleanup (migration marker v6).
- **2a.18–2a.19:** Privacy-epoch stale async publication checks and permit-bearing sensitive lifetimes (including external dialog and revision preview); mutually exclusive revocation transitions; whole-library restore drains derivative producers, revokes UI and performs durable ownership handoff before database replacement.

## Later source-verified evolution requiring explicit reconciliation

| Finding | Earlier text | Later evidence and disposition |
| --- | --- | --- |
| **B2-01 — VACUUM scrub method** | E3 initial SQLite section mandates WAL truncate → `VACUUM` → WAL truncate | [2b.42.8 forensic decision](../../VAULT_2B42_8_NO_VACUUM_FORENSIC.md) documents a tested no-VACUUM approach. Current `src/storage/protection.rs` invokes `PRAGMA secure_delete = ON`, `PRAGMA synchronous = FULL`, FTS5 `rebuild` and verified `wal_checkpoint(TRUNCATE)`, **without production `VACUUM`**. Interpret this as a later implementation-specific amendment **pending dedicated final-source native QA**, not proof that the original forensic objective was waived |
| **B2-02 — UI revocation scope** | E3 2a.18 describes a single UI privacy epoch | [2b.42.1 scoped publication design](../../VAULT_2B42_1_SCOPED_PUBLICATION_CORE.md) and current `src/ui/privacy.rs` / `src/security/privacy_coordinator.rs` implement **library, multi-note and per-note generations**. Preserve E3's *stale-publication safety goal*; treat its single-global-epoch implementation prescription as replaced for current code, with final end-to-end confirmation in Batch 3 |
| **B2-03 — canonical protected payload granularity** | E2's original description models one whole-note authenticated payload | [2b.42.10 source closure](../../VAULT_2B42_10_INDEPENDENT_CLOSURE.md) describes a subsequent granular Vault-object model and invariant I7. Preserve the original covered-sensitive-data requirements while distinguishing the later representation; detailed implementation-authority sweep deferred to Batch 3 |
| **B2-04 — E2 read-only language** | E2 initially forbids editing encrypted notes, with 2a.18 exception for protected revision restore | The later Vault series adds protected mutation functionality. The E2 text is **historical phase scope**, not a current product-wide read-only claim; re-audit subsequent amendments during Batch 3 |

These differences do not justify deleting E3's historical text or claiming full closure. In particular, the [2b.42.10 report](../../VAULT_2B42_10_INDEPENDENT_CLOSURE.md) explicitly records native execution as pending, despite its independent source-audit PASS.

## Complete heading/section traceability

### E2: [original contract](../../SECURITY_ENCRYPTION_E2_CONTRACT.md) (Git blob `36818fcb02e43c9c46482f7b6e25532739d8b473`)

| Original clause | Original lines | Authority treatment |
| --- | ---: | --- |
| `Rustnote Security E2 Contract — Selective Note Encryption` | 1–4 | Preserve requirement and scope |
| `Objective` | 5–10 | Phase-specific; later Vault evolution requires review |
| `Canonical protected-note model` | 11–25 | Phase-specific; later Vault evolution requires review |
| `SQLite boundary while locked` | 26–42 | Preserve requirement and scope |
| `Attachment contract` | 43–54 | Preserve requirement and scope |
| `Unlock / lock contract` | 55–62 | Phase-specific; later Vault evolution requires review |
| `Search contract` | 63–68 | Preserve requirement and scope |
| `Export / backup contract` | 69–76 | Preserve requirement and scope |
| `Plaintext-derivative boundary and E3` | 77–89 | Preserve requirement and scope |
| `Cryptographic boundary` | 90–95 | Preserve requirement and scope |
| `Integrity / future blockchain-inspired work` | 96–99 | Preserve requirement and scope |
| `E2 definition of done` | 100–114 | Preserve requirement and scope |
| `2a.18 protected revision restore amendment` | 115–122 | Preserve requirement and scope |

### E3: [original contract](../../SECURITY_ENCRYPTION_E3_CONTRACT.md) (Git blob `547fd93a2772405b621340f79c36417ab9de35bc`)

| Original clause | Original lines | Authority treatment |
| --- | ---: | --- |
| `Rustnote Security E3 Contract — Managed Plaintext-Residue Closure` | 1–4 | Preserve requirement and scope |
| `Objective` | 5–10 | Preserve requirement and scope |
| `Managed plaintext-residue guarantee` | 11–27 | Preserve requirement and scope |
| `SQLite WAL / free-page contract` | 28–39 | Later 2b.42.8 method change; B2-01 |
| `Emergency recovery contract` | 40–45 | Preserve requirement and scope |
| `Automatic backup contract` | 46–53 | Preserve requirement and scope |
| `Cache / preview contract` | 54–62 | Preserve requirement and scope |
| `Attachment handoff contract` | 63–68 | Preserve requirement and scope |
| `Temporary/staging contract` | 69–74 | Preserve requirement and scope |
| `PDF session authorization contract` | 75–80 | Preserve requirement and scope |
| `Browser/editor memory boundary` | 81–86 | Preserve requirement and scope |
| `Physical-media limitation` | 87–92 | Preserve requirement and scope |
| `Search boundary` | 93–96 | Preserve requirement and scope |
| `Export boundary` | 97–102 | Preserve requirement and scope |
| `Cryptographic invariants retained` | 103–116 | Preserve requirement and scope |
| `Explicit non-goals / limits` | 117–130 | Threat/scope limitation; preserve |
| `E3 definition of done` | 131–146 | Preserve requirement and scope |
| `2a.7 final-closure amendment — atomic Lock revocation` | 147–154 | Cumulative amendment; retain |
| `2a.7 final-closure amendment — private staging and unprotect semantics` | 155–160 | Cumulative amendment; retain |
| `2a.7 final-closure amendment — metadata and external egress` | 161–165 | Cumulative amendment; retain |
| `2a.8 final-closure amendment — plaintext descriptor lifetime` | 166–172 | Cumulative amendment; retain |
| `Persisted tag-filter contract — 2a.9 final-closure amendment` | 173–177 | Cumulative amendment; retain |
| `2a.10 behavioral final-closure amendment — FTS forensic scrub and snapshot barrier` | 178–187 | Cumulative amendment; retain |
| `2a.11 adversarial lifecycle amendment — owned reads, temp ownership and backup history` | 188–196 | Cumulative amendment; retain |
| `2a.12 security-boundary consolidation amendment` | 197–204 | Cumulative amendment; retain |
| `2a.13 adversarial behavioral closure amendment` | 205–212 | Cumulative amendment; retain |
| `2a.14 independent final closure audit amendment` | 213–224 | Cumulative amendment; retain |
| `2a.15 plaintext ownership remediation amendment` | 225–234 | Cumulative amendment; retain |
| `2a.16 canonical plaintext ownership amendment` | 235–251 | Cumulative amendment; retain |
| `2a.17 persistent residue and backup closure amendment` | 252–263 | Cumulative amendment; retain |
| `2a.18 in-memory/UI privacy lifetime amendment` | 264–273 | Later 2b.42 scope refinement; B2-02 |
| `2a.19 independent adversarial closure amendment` | 274–287 | Cumulative amendment; retain |

## Closure of this documentation sub-batch

The original E2/E3 documents have been read as complete contracts, including **all E3 amendments through 2a.19**. This table supplies a stable line-level trace without stripping precise edge cases. The original bytes and paths remain unchanged. No tests were executed and no proposed new implementation was approved. The reconciliation findings B2-01–B2-04 remain visible for the later historical Vault/code audit.

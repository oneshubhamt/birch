# Q6 — Independent closure audit

## Scope and method

Q6 starts de novo from the governing editor, E0–E3 security/privacy and backup requirements and treats the Q5 concern suites as untrusted regression aids. It does not infer closure from validator PASS wording or historical pass topology.

The audit directly traced:

- canonical editor/document authority and browser/native adapters;
- low-level cryptographic ownership and format/context authentication;
- plaintext creation, ownership, transfer, deletion and recovery;
- note-, multi-note- and library-wide publication/revocation ordering;
- async UI lifetime and protected resource permits;
- Vault transition crash states and restart recovery;
- attachment ingest, canonical blobs, quarantine, materialization and handoff;
- protected current/revision state and revision restore;
- SQLite, FTS5 and WAL residue;
- automatic/safety backup snapshots, current repository restore and whole-library ownership handoff;
- staged 2b.43 key/header/keyset authority;
- native temporary-file, descriptor, media/PDF and secure-store boundaries.

The audit also independently swept production SQLite constructors and plaintext-capable filesystem writers instead of relying only on named tests.

## Status semantics

| Status | Meaning |
| --- | --- |
| **PASS** | Current implemented requirement has direct executable/compiled evidence and no unresolved Q6 defect. |
| **PASS after remediation** | Q6 found a current defect or governance gap, fixed it, and added equal-or-stronger executable coverage. |
| **DEFERRED** | Requirement belongs to a deliberately not-yet-integrated future milestone; Q6 does not misrepresent it as implemented. |
| **NATIVE/MANUAL** | Source/mock boundaries exist, but real supported-device/OS behavior needs integration evidence outside ordinary unit tests. |

## De-novo requirement → invariant → executable owner traceability

| Requirement domain | Current invariant reconstructed de novo | Primary executable owner(s) | Q6 status |
| --- | --- | --- | --- |
| Editor authority | One versioned Rustnote/ProseMirror document grammar; unsafe/unknown structures fail closed; Rust and JS consume the same corpus | `editor-spec/document-spec.json`; `validation-cases.json`; `validate-editor-document-contract.mjs`; `shared_editor_document_contract_corpus_matches_rust_validator`; `invalid_contract_and_unsafe_content_are_rejected` | **PASS** |
| Editor runtime/bridge | Real ProseMirror transformations and one typed/versioned host bridge; bounded attachment/event data | `editor-prosemirror-behavior.test.mjs`; `editor-bridge-behavior.test.mjs`; `src/editor_bridge/{command,event,transport}.rs` compile/tests | **PASS** |
| E0 crypto profile | AEGIS-256, full 256-bit tag, 256-bit keys/nonces, Argon2id v1 profile/resource ceiling, random production keys/nonces | `aegis256_v1_uses_full_256_bit_authentication_tag`; `v1_argon2_profile_is_the_untrusted_envelope_resource_ceiling`; vault round-trip/negative tests | **PASS** |
| E1/E2 crypto isolation | Low-level AEGIS/Argon2 code lives only in `rustnote-vault`; application/storage/UI use the security facade | Q6 `qa-architecture.sh` Cargo/import isolation guard; all-feature compile/Clippy | **PASS after remediation** |
| Authenticated formats/context | Wrong key/password/context/class, tampering, truncation/trailing/reordered chunks fail before accepted plaintext publication | `object_rejects_wrong_key_context_and_tampering`; password-envelope negative tests; `protected_blob_rejects_wrong_context_tampering_truncation_and_trailing_data` | **PASS** |
| Protected graph containment | Protected title/body/tags/revisions/sources/PDF annotations/attachment metadata remain in protected graph; ordinary locked SQLite rows are routing placeholders | protection/vault-object tests including granular graph round trip and protected revision restore | **PASS** |
| Plaintext ownership | No Rustnote-managed plaintext file is created before a cleanup owner; owner survives failed cleanup or transfers atomically | `src/security/plaintext.rs` ownership tests; `plaintext_staging.rs` failed cleanup/rename/restart tests; writer sweep | **PASS** |
| Process-private temporaries | Sensitive temp roots/files are private; owner exists before materialization; abandoned sessions recover fail closed | `native/private_temp.rs`; `native/temp_owner.rs`; staging/output tests | **PASS** |
| Publication/revocation | Lock publishes revocation before drain; note lock is scoped; multi-note/library publication is invalidated at the correct scope | `privacy_coordinator.rs` 28-test matrix; navigation/note-local/multi-note/library-wide JS contracts | **PASS** |
| Protected read lifetime | Lock waits for already-authorized protected reads; new permits/publications fail once revocation begins | `lock_waits_for_active_read_permit_and_rejects_new_reads_after_revocation`; runtime/materialization tests | **PASS** |
| UI async lifetime | Decrypted async results require current privacy generation and scope immediately before publication | UI privacy tests plus navigation/publication and Vault-action JS contracts | **PASS** |
| Whole-library concurrency | Backup/restore acquires whole-library producer authority; Restore serializes against privacy transitions and blocks late producers | `whole_library_operation_serializes_against_library_restore`; library-wide JS contract; application privacy tests | **PASS** |
| Vault crash recovery | Canonical durable transition states recover without publishing unowned plaintext; post-commit cleanup failure remains journaled | protection crash-state/recovery tests including `protection_cleanup_failure_stays_journaled_until_recovery_succeeds` | **PASS** |
| Attachment ingest | Incoming plaintext is durably owned before first byte; pending work blocks conflicting protection/snapshots; restart reconciles | attachment-ingest recovery tests; `pending_attachment_mutations_block_encrypt_until_their_durable_state_is_resolved` | **PASS** |
| Protected attachments | Blob encryption/restoration is lossless; failed auth leaves no untracked plaintext; materialization remains permit-owned | `protected_note_encrypts_attachment_blob_and_restores_it_losslessly`; failed-auth/materialization tests | **PASS** |
| Attachment descriptors/handoff | Protected PDF/media/Open/Reveal sensitive lifetime retains current permit; Lock cannot succeed while descriptor/file use continues | attachment protocol/media-server/native handoff tests; note-local/Vault action JS contracts | **PASS** |
| Quarantine ownership | Canonical→orphan transfer reserves successor before rename; restart never trusts an unowned orphan | orphan-transfer restart tests; `startup_verification_rejects_preexisting_unowned_orphan_plaintext` | **PASS** |
| Revisions | Protected revision restore stays protected and does not repopulate ordinary plaintext tables; revision-only plaintext is scrubbed | `protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables`; `revision_only_plaintext_is_removed_from_sqlite_and_wal_on_protection` | **PASS** |
| FTS/WAL residue | Protect uses secure-delete + FTS rebuild + verified WAL TRUNCATE; no per-Protect VACUUM; protected markers absent from DB/WAL/managed backup | `protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot`; `no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups` | **PASS after governance repair** |
| Emergency journal/autosave | Protected plaintext cannot be recreated by stale autosave/recovery; cleanup failures fail closed | `vault_barrier_prevents_stale_plaintext_emergency_journal_recreation`; emergency-journal protected/quarantine tests | **PASS** |
| Snapshot transition barrier | Persistent snapshots reject unfinished Vault operations or genuine active plaintext staging | `persistent_snapshots_fail_closed_while_vault_operation_is_pending`; Q6 `snapshot_sanitization_does_not_hide_active_plaintext_staging` | **PASS** |
| Current managed backup privacy | Historical/pre-protection app-managed repositories are invalidated with durable offline obligations and cross-library ownership isolation | backup repository privacy-obligation, restart and namespace-marker tests | **PASS** |
| Current managed backup restore | Repository snapshot restores exact note/attachment bytes; outgoing quarantine is retired before SQLite ownership handoff; reopen remains clean | Q6 `repository_snapshot_restore_round_trips_note_and_attachment_through_library_restore` | **PASS after remediation** |
| Restore canonical ownership | Incoming-only canonical blobs are owner-backed before publication; old-only blobs receive cleanup owner before DB switch; rollback is fail closed | `backups.rs` restore ownership tests plus Q6 restore/reopen regression | **PASS** |
| 2b.43 staged root/key authority | Missing existing root is never recreated; wrong device root/context/corrupt/unavailable store fails closed; repository keysets are independent and redacted | `backup_key_authority.rs` key/root/context tests; `qa-backup-format.sh` | **PASS as staged authority** |
| 2b.43 exact staged bytes | Header/keyset/envelope constants and Apple envelope authentication/round trips are exact and bounded | `fresh_repository_header_matches_exact_2b43_2_apple_layout`; exact 204-byte keyset test; vault Apple envelope tests | **PASS as staged authority** |
| Encrypted repository v3 writer/restore | Snapshot DB, manifest/index, blob identities and staging are encrypted/authenticated under the 2b.43.2 format and restored through the device-root authority | Not integrated: active writer remains repository v2 | **DEFERRED — 2b.43.4** |
| Real Keychain/Keystore behavior | Supported OS secure store creates/retrieves/denies/loses/corrupts root exactly as specified | source boundary exists; unit authority uses protector abstraction; supported-device evidence not in ordinary CI | **NATIVE/MANUAL** |
| Native release boundary | Full feature matrix compiles/lints/tests on macOS; release artifact meets deployment target | hosted macOS `cargo check/clippy/test/build`; Mach-O deployment validator | **PASS as CI gate** |

## Independent writer/residue sweep

Q6 separately inspected the production Rust tree for SQLite connection authority and plaintext-capable filesystem creation/copy/rename/removal.

The resulting ownership map is coherent:

- SQLite implementation authority remains in `src/storage`.
- attachment `.incoming` staging reserves durable ownership before copying;
- canonical attachment replacement and quarantine transfer preserve predecessor/successor ownership across rename;
- portability/import and restore staging reserve durable or process-private ownership before plaintext bytes appear;
- Vault materialization registers its destination before decryption/materialization;
- native image/PDF staging and handoff use private process-session ownership;
- protected media compatibility avoids persistent cache publication;
- protected image/media responses are ephemeral/no-store;
- PDF/media requests retain current authorization for the descriptor/stream lifetime.

No additional unowned production plaintext writer was found after the Q6 restore remediation.

## Q6 findings and repairs

### F1 — governing E3 SQLite rule contradicted current proven behavior

The retained E3 document still said Protect must perform `VACUUM`, while current production and forensic regressions intentionally use `secure_delete + FTS rebuild + verified WAL TRUNCATE` without per-Protect `VACUUM`.

**Repair:** the E3 current governing rule now records the later no-VACUUM override and treats the old migration/VACUUM wording as historical rationale.

### F2 — current repository snapshots could become unrestorable after normal attachment removal

A normal ordinary-attachment removal moves the last canonical plaintext blob into `.orphaned` quarantine and correctly keeps a durable generic owner for the quarantine retention period. Snapshot validation counted every `managed_plaintext_staging` row as unresolved, so a later safety snapshot/Restore could reject otherwise valid steady-state quarantine.

The first Q6 end-to-end restore regression exposed this directly: Restore failed closed with an unresolved attachment/staging integrity error.

**Repair:** the snapshot **copy only** removes rows that are positively proven settled quarantine ownership. All active incoming/import/canonical/transfer/ambiguous plaintext ownership remains a snapshot blocker.

### F3 — whole-library Restore could otherwise drop the owner of excluded old-library quarantine

Because the incoming archive intentionally excludes ordinary quarantine, switching the canonical SQLite database while old `.orphaned` files survived would drop the outgoing ownership rows and leave unowned plaintext on disk.

**Repair:** after the restore safety snapshot is created and before the SQLite ownership handoff, Restore strictly retires settled old-library quarantine under the outgoing database's ownership. Cleanup failure aborts before the database switch. The end-to-end regression drops/reopens Storage after Restore to prove restart does not discover an unowned orphan.

### F4 — crypto-primitive isolation had behavior but no direct drift guard

E2 requires low-level crypto to remain owned by `rustnote-vault`.

**Repair:** `qa-architecture.sh` now fails if the root package directly depends on `aegis` or `argon2`, or if Rust outside `crates/rustnote-vault` imports those primitive namespaces.

## Adversarial restore acceptance

Q6 adds paired tests rather than a one-sided happy path:

1. `repository_snapshot_restore_round_trips_note_and_attachment_through_library_restore`
   - creates a note and real attachment;
   - takes a repository snapshot;
   - mutates the note and removes the attachment, creating settled quarantine;
   - restores the earlier snapshot;
   - verifies the exact note title, attachment identity/name and attachment bytes;
   - audits attachment integrity;
   - closes and reopens the library, proving no unowned old quarantine survives Restore.

2. `snapshot_sanitization_does_not_hide_active_plaintext_staging`
   - creates a genuine active managed plaintext owner;
   - proves safety snapshot creation remains fail closed;
   - therefore proves the Q6 snapshot sanitizer is narrow rather than a general bypass of ownership validation.

## Explicit non-closure / future work

Q6 does **not** claim either of these is already implemented:

- **Encrypted managed-backup repository v3 persistence/restore.** The active repository writer remains v2. The key/header/keyset authority is staged, while the encrypted snapshot/manifest/index/blob/staging and restore cutover belongs to 2b.43.4.
- **Real supported-device secure-store integration evidence.** The macOS Keychain / Android Keystore source boundary and fail-closed abstraction are present, but actual device/OS create/retrieve/deny/loss/corruption behavior remains a native/manual integration gate.

Likewise, a Q6 PASS is not signing/notarization, hardware certification, hostile-OS proof or a cryptographic formal proof.

## Closure judgment

For the **currently implemented** editor, E0–E3 privacy/security and active managed-backup v2 behavior, the de-novo requirement → invariant → executable-owner chain is complete **after the Q6 repairs above**. The audit found a real backup/restore ownership defect precisely because it did not trust the streamlined suite's prior green status.

Final Q6 closure still requires the permanent concern suites, dependency-backed browser tests, all-target/all-feature native check, strict Clippy, all-feature Rust tests, release build and deployment-target inspection to pass together on the final Q6 revision.

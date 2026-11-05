# Rustnote Vault 2b.5 — Final Consolidation, Deletion & Performance Closure

## Status

**Source-level 2b.5 implementation complete. Native Rust release gate remains mandatory because this execution environment has no `cargo`, `rustc`, or `rustfmt`.**

Contract build ID: `2b.5`

Implementation baseline: `rustnote_v0.12.5_build_2b.5.2_2b1_2b4_objective_closure_source`

Governing scope: Pass 2b.5 of the original **Rustnote Vault 2b — Architecture Consolidation & Granular Encrypted Object Model** contract.

The pass was performed audit-first. Existing 2b.5 reports and validators were treated as evidence only and the changed tree was reconstructed from source.

## 1. Finite objective set

The de-novo audit produced five objectives:

1. delete dead and compatibility-only transition, staging, filesystem and export machinery;
2. complete typed plaintext ownership across the remaining durable attachment-ingest lifetime;
3. re-prove that transition/publication/plaintext authorities are singular rather than duplicated;
4. verify every contract-listed performance surface and fix only algorithmically obvious regression;
5. rerun targeted, inherited security, architecture and dependency-free behavioral QA, followed by a final source re-audit.

## 2. Consolidation and deletion

### 2.1 Dead APIs/helpers removed

The following unreferenced or superseded production APIs/helpers were deleted:

- `AttachmentService::attachment_ids_for_request`;
- `AttachmentTransportStore::attachment_is_protected`;
- `AttachmentStore::is_note_protected` / `Storage::is_note_protected_canonical`;
- `Storage::protected_note_access_if_unlocked`;
- `cleanup_managed_plaintext_stage_after_error`;
- `export_markdown_library` and `export_markdown_notebook` plus the obsolete multi-scope Markdown export machinery;
- compatibility-only `scrub_protected_attachment_ingests_for_vault_unlocked`.

The existing schema 12→21 ladder, one-time E3 upgrade marker, monolithic protected-note representation and pre-ledger staging recovery were already absent in the 2b.5.2 baseline and remain absent.

### 2.2 Obsolete sensitive-temp compatibility removed

Current sensitive temporary ownership is one model:

```text
session-v2-<pid>-<random>
        +
private owner-lock file
        +
OS-held exclusive lock
        +
cleanup claim on abandoned ownership
```

Production no longer carries migration support for:

- shared `/tmp/rustnote/...` temporary namespaces;
- `rustnote-vault-*` temporary roots;
- numeric/PID-only session directories;
- old `session-*` PID layouts;
- PID-liveness heuristics.

Removal of those branches made `src/native/process.rs` dead, so that module was deleted too.

Current `session-v2` crash cleanup remains fail-closed. A markerless directory is claimed before cleanup; an owner being established concurrently checks that claim after acquiring its lock and abandons the half-owned directory instead of publishing plaintext into it.

### 2.3 Backup historical proof code retained deliberately

`backup_repository.rs` still contains historical repository ownership/proof logic. This is not compatibility for an obsolete protected-note format.

Rustnote remembers prior backup locations as privacy obligations. The historical proof logic exists so Vault cleanup can distinguish a proven Rustnote-owned plaintext backup repository from unrelated user data before deletion. Removing it would weaken E3 by either abandoning plaintext backup residue or deleting data without ownership proof.

It therefore remains security cleanup/recovery, not a dual Vault architecture.

## 3. Plaintext-authority closure

After generic `managed_plaintext_staging` ownership transfers into an `attachment_ingest_jobs` row, that row is itself the durable cleanup owner. 2b.5 now represents that lifetime through `OwnedPlaintext` rather than returning to a raw `PathBuf` deletion convention.

Conceptually:

```text
managed_plaintext_staging owner
        ↓ same SQLite transaction
attachment_ingest_jobs owner
        ↓
OwnedPlaintext::durable_attachment_ingest_existing
        ↓
unlink succeeds / path absent
        ↓
ingest row may retire
```

A failed unlink leaves the durable ingest row intact. The new regression `failed_pending_ingest_cleanup_keeps_durable_job_owner` proves that an unremovable staging path cannot cause ownership retirement.

Untracked `.incoming` files have no valid owner by definition and are now removed through the centralized native privacy filesystem boundary rather than direct `fs::remove_file`.

## 4. Authority re-audit

### Privacy transitions

Low-level Vault revocation and derivative-transition composition remains confined to `PrivacyCoordinator` and security internals. Production feature/storage code does not recreate the old revocation + derivative + storage ordering.

The ordinary storage mutex remains because ordinary database mutations still need serialization; it is not a second privacy-transition authority. Protected mutation guards are coordinator-owned and hide Vault-reader/storage ordering from callers.

### UI privacy generation

`UiPrivacyEpoch` remains an immediate UI-local revocation generation used inside semantic `PrivacyPublicationToken` capture. It cannot perform privacy transitions and is not an alternate Vault transition epoch. Coordinator privacy generation remains authoritative for protected authorization.

### Plaintext ownership

Current managed plaintext lifetimes reduce to the typed authority backends:

- durable staging;
- durable Vault operation;
- durable attachment operation;
- durable attachment ingest;
- Vault session;
- process-private session.

Explicit user-selected export destinations remain external/user-owned, as required by the contract.

## 5. Performance closure

No speculative micro-optimization was performed. Two algorithmically obvious sources of unnecessary work were removed.

### 5.1 Protected Unlock — duplicate graph decryption removed

Before this pass Unlock authenticated/decrypted the complete current granular graph, installed the NoteKey, then called `get_note()`, causing the same current graph to be decrypted again for the returned note.

It now:

```text
decrypt/authenticate current graph once
        ↓
materialize Note from that authenticated graph
        ↓
install NoteKey
        ↓
return Note
```

Unlock still authenticates all current mutable objects before retaining the NoteKey and does not traverse revision history.

### 5.2 Single-note Markdown export — library scan removed

The surviving Markdown export API is note-only. Its storage implementation nevertheless retained the old library/notebook export scope and loaded every notebook and scanned the note table before filtering to one note.

The obsolete scope machinery was deleted. Note export now performs a targeted `WHERE n.id = ?1` query and only reads that note's attachments.

### 5.3 Contract-listed surface audit

| Surface | 2b.5 source-level conclusion |
|---|---|
| Startup | Recovers small ownership/operation ledgers and current attachment state only; no protected graph decryption or revision-history traversal. |
| Ordinary note open | Ordinary SQLite row + cheap protected-root decision. Unprotected note returns before Vault hydration. |
| Ordinary typing/autosave | Cheap protected-root decision, then ordinary SQLite/FTS/revision path. No encrypted object reconstruction. |
| Ordinary search | Remains FTS5/storage-owned query translation; no Vault object read. |
| Protected unlock | Current granular graph decrypted/authenticated once; no revision-chain traversal. |
| Protected typing/autosave | Reads manifest/head plus only mutation-relevant object classes; no `read_vault_current_graph` or revision-chain traversal. |
| Revision creation | Reads current revision head and appends one immutable `RevisionObject`; no prior-history walk. |
| Revision restore | Reads selected immutable revision and the state required for the authorized restore; does not repopulate plaintext `note_revisions`. |
| Attachment ingestion | Staging copy/hash is bounded and durably owned; protected finalization encrypts the specific blob and updates attachment index. |
| Attachment opening | Reads manifest + attachment index and streams one independently encrypted blob into Vault-session plaintext. |
| Lock | Revokes/drains authorization and cleans managed plaintext; does not decrypt graph/history. |
| Protect | One-time transition may read ordinary current/revision/attachment state because it is converting the note; no steady-state dual representation remains afterward. |
| Unprotect | Explicit one-time reconstruction may traverse protected history/attachments because it converts them back to ordinary persistence. This is not a hot-path regression. |

No runtime timing claims are made in this environment. The pass fixes only the two source-level redundant/broad operations above and otherwise preserves the algorithmically granular architecture.

## 6. Direct writer/filesystem sweeps

### Protected encrypted-object writers

`vault_objects`, `vault_note_roots` and `vault_blob_refs` mutation remains concentrated in the object-store/protection implementation. No alternate monolithic writer was found.

### Ordinary revision writers

Plaintext `note_revisions` writers remain for legitimate ordinary-note history, import, pruning and explicit Unprotect reconstruction. Protected steady-state revision creation/restore uses encrypted `RevisionObject`s and does not repopulate ordinary revision storage.

### Raw filesystem operations

Raw filesystem mutation remaining in audited storage/native code was classified as one of:

- implementation inside `OwnedPlaintext`/native privacy authority;
- ciphertext/blob mutation;
- Rustnote-owned backup/archive machinery with its own ownership proof;
- external user-selected export destination;
- directory creation rather than plaintext-file creation;
- tests.

No second managed-sensitive-plaintext file lifecycle was found.

## 7. Cleanliness result

Compared with the completed 2b.5.2 source, the production `src/` changes in this pass are:

```text
165 inserted lines
593 deleted lines
net 428 production lines removed
```

The net deletion reflects actual consolidation rather than moving the same logic elsewhere: old temp-layout parsing/PID heuristics, dead APIs and broad export scope are gone, while the only new substantive code extends `OwnedPlaintext` to a real current ownership backend and adds its regression test.

## 8. Objective verification

- dead transition/lock duplication: no alternate transition authority found;
- duplicate privacy epochs: no second transition epoch; UI epoch remains token-local UI revocation state;
- old staging helpers: migration-only temp and ingest helpers removed;
- obsolete `ProtectedNotePayload`: absent;
- duplicate filesystem cleanup: remaining attachment-ingest lifetime folded into `OwnedPlaintext`;
- duplicate revision handling: protected steady-state remains encrypted-object based; ordinary revision code retained only where semantically ordinary;
- obsolete compatibility code: pre-release temp-layout/PID compatibility removed; backup proof logic retained for E3 safety;
- direct sensitive raw filesystem writers: no unmanaged current plaintext lifecycle found;
- performance: contract-listed surfaces re-audited and two algorithmically obvious regressions removed.

## 9. Release boundary

All static/adversarial and dependency-free QA available here is green; see `QA_VAULT_2B5_CONSOLIDATION.md`.

Native release qualification remains mandatory on macOS:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
./scripts/validate-security-e0-e3-closure.sh
./scripts/validate-vault-2b1-foundation.sh
./scripts/validate-vault-2b2-privacy-authority.sh
./scripts/validate-vault-2b3-publication-token.sh
./scripts/validate-vault-2b4-granular-cutover.sh
./scripts/validate-vault-2b5-consolidation.sh
```

2b.5 is source-complete subject to that native Rust release gate.

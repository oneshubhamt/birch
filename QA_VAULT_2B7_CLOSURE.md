# Rustnote Vault 2b.7 — Closure QA and Certification Record

Date: 2026-09-01  
Build: `2b.7`

## Final source-level invariant audit

| Invariant | Result | Evidence summary |
|---|---|---|
| I1 — ownership precedes managed plaintext | PASS | creation/transfer paths use pre-existing database/Vault/Add/durable ownership; quarantine successor is reserved before rename; steady-state orphan cleanup cannot adopt late ownership |
| I2 — ownership survives until plaintext disappears | PASS | typed unlink/rename retires predecessor only after conclusive source absence; failure retains durable obligation |
| I3 — one privacy transition authority | PASS | note/library privacy transition composition remains inside `PrivacyCoordinator`; production callers request coordinator guards |
| I4 — conflicting transitions fail closed | PASS | generic admissions reject conflicts; protected Restore now rejects active same-note/library transitions instead of waiting; transition-owned code no longer re-enters public protected readers |
| I5 — protected plaintext publication is authorized | PASS (source/static) | publication token retains UI/library/note/coordinator generations; publication lease is drained by transition admission; no new UI publication bypass introduced by 2b.7 |
| I6 — protected persistence is deny-by-default | PASS | exhaustive `SensitiveNoteData` classification/public-stub authority remains intact |
| I7 — mutation cost follows changed state | PASS (algorithmic) | note/metadata/index/revision/blob remain separate granular encrypted objects; revision history is not rebuilt during normal autosave |

## 16 Definition-of-Done audit

| # | Requirement | Result |
|---|---|---|
| 1 | one transition ordering authority | PASS |
| 2 | managed sensitive plaintext cannot be created without prior ownership | PASS source-level |
| 3 | ownership cannot disappear before plaintext | PASS source-level |
| 4 | rename/transfer crash recoverable | PASS source-level + targeted recovery regressions |
| 5 | valid publication token required | PASS source/static |
| 6 | default-private sensitive-data authority | PASS |
| 7 | granular protected persistence | PASS |
| 8 | revision-history growth does not change normal autosave complexity | PASS algorithmic |
| 9 | attachment metadata mutation does not rewrite attachment bytes | PASS |
| 10 | ordinary hot paths remain effectively Vault-free | PASS algorithmic; no new ordinary crypto/object reconstruction introduced |
| 11 | superseded privacy orchestration removed | PASS |
| 12 | no permanent dual protected-note representation | PASS |
| 13 | changed surfaces have targeted adversarial QA | PASS at source/static-test-definition level |
| 14 | complete application passes full regression QA | **NATIVE RELEASE GATE OUTSTANDING** — Rust toolchain unavailable in this environment |
| 15 | final independent audit finds no alternate writer/race/residue | PASS source-level after 2b.7 repairs and repeated manual sweeps |
| 16 | final source simpler than 2a.19 rather than merely different | PASS architectural assessment; no new parallel privacy subsystem added |

## Targeted regressions added/strengthened

The closure tree contains regressions covering at least:

- `protect_and_lock_complete_without_reentering_privacy_admission`
- `protected_restore_rejects_existing_same_note_transition_without_waiting`
- `protected_permanent_delete_does_not_reenter_note_access`
- protected link/backlink lock→unlock behavior
- `ordinary_source_resolves_protected_target_title_only_while_target_is_unlocked`
- `quarantine_transfer_keeps_durable_owner_until_plaintext_deletion`
- `pending_add_remains_the_only_owner_when_last_attachment_reference_is_removed`
- `corrupt_pending_add_fails_closed_before_canonical_ownership_can_move`
- `steady_state_quarantine_rejects_unowned_canonical_plaintext`
- startup rejection of unowned canonical/quarantine plaintext
- prepared ownership-transfer recovery
- ordinary revision-restore ownership handoff
- revision-pruning ownership handoff
- revision-only hard-delete ownership handoff
- expired-Trash batch ownership handoff
- protected ordinary-revision residue fail-closed behavior
- direct protected-ciphertext hard-delete/purge cleanup
- `protected_ciphertext_cleanup_owner_recovers_after_database_commit_crash_boundary`

Workspace Rust tests discovered in the frozen source: **273**.

## Executed QA on the frozen source

### Vault/security guards

PASS:

- `scripts/validate-vault-2b7-closure-repair.sh`
- inherited `validate-vault-2b1-foundation.sh`
- inherited `validate-vault-2b2-privacy-authority.sh`
- inherited `validate-vault-2b3-publication-token.sh`
- inherited `validate-vault-2b4-granular-cutover.sh`
- inherited `validate-vault-2b5-consolidation.sh`
- inherited `validate-vault-2b6-independent-closure.sh`
- `scripts/validate-security-e0-e3-closure.sh`
  - E0 static contract PASS
  - E1 static contract PASS
  - E2 static contract PASS
  - E3 static contract PASS
  - 2a.16 canonical ownership PASS
  - 2a.17 residue/backup closure PASS
  - 2a.18 UI lifetime/revision restore PASS
  - 2a.19 adversarial closure PASS

### Architecture/editor/PDF guards

PASS in static mode:

- architecture A1–A7
- architecture B1–B6
- Dioxus 0.7.10 source/lock baseline

The B2–B6 wrappers use `/bin/zsh`; this Linux sandbox lacks zsh, so an external temporary `/bin/zsh -> /bin/bash` compatibility symlink was used only while executing the static wrappers and then removed. No source-tree file was changed for this environment workaround.

### Dependency-free behavior

`node scripts/run-browser-behavior-tests.mjs`:

- **36 tests**
- **36 passed**
- **0 failed**

### Shell syntax

- **40** shell scripts checked with `bash -n`
- **40 passed**

## Manual independent closure sweeps

Repeated after implementation:

- privacy-transition caller sweep;
- direct SQLite attachment/revision/Vault reference writer sweep;
- raw sensitive filesystem writer/lifecycle sweep;
- canonical blob owner transfer/recovery sweep;
- protected link read/write symmetry sweep;
- protected/unprotected destructive-operation sweep;
- publication-token architecture sweep;
- old `ProtectedNotePayload` / `protected_notes` production representation sweep;
- startup recovery ordering sweep;
- backup/restore and FTS/WAL prior-closure regression review.

No active production `ProtectedNotePayload` or `protected_notes` table/writer was found.

## Performance conclusion

The 2b architecture satisfies the intended algorithmic locality:

- ordinary note editing does not reconstruct Vault objects;
- protected autosave rewrites the current state object and only the other object groups that changed;
- revision creation appends one immutable revision object;
- attachment bytes remain streamed/independently encrypted;
- startup does not decrypt/traverse every protected history.

This environment does **not** provide trustworthy 2a.19-vs-2b wall-clock performance measurements. Empirical comparative timing remains a native release/performance gate rather than an invented claim.

## Packaging integrity

The frozen 2b.7 source is packaged only after the source/static closure above. The release archive is then extracted into a separate verification directory and compared recursively against the frozen source tree.

Final packaging checks:

- source tree file count: **346**;
- extracted archive file count: **346**;
- recursive source-vs-extracted comparison: **0 differences**;
- no packaged `target`, `node_modules`, `.git` or `.DS_Store` paths.

The archive SHA-256 is calculated after this report is packaged and is therefore reported alongside the downloadable artifact rather than embedded recursively inside the archive itself.

## Mandatory native Mac release gate

The environment used for this closure has no `cargo`, `rustc` or `rustfmt`. Therefore the following are **not claimed as executed** here and remain mandatory before release qualification:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

Then rerun the project security/architecture validators on the Mac.

## Certification statement

**Source-level architectural/security closure for 2b.7: PASS.**

**Native release qualification: PENDING the mandatory Rust toolchain gate above.**

If the native gate reveals any compile, Clippy, behavioral or race failure, 2b.7 must be reopened rather than waived.

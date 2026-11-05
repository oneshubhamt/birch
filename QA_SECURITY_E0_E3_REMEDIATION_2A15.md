# Rustnote 0.12.5 build 2a.15 — targeted plaintext-ownership QA

## Objectives

- [x] Lock retains the revoking session/key and every plaintext cleanup obligation until deletion succeeds.
- [x] Protected attachment materialization establishes Vault ownership before file creation/decryption and never late-registers plaintext.
- [x] Concurrent attachment materialization cannot replace a live registry entry and orphan the prior plaintext path.
- [x] Protected deterministic emergency-journal records are removed before parsing/quarantine and swept again after recovery.
- [x] Existing non-file journal nodes are treated as cleanup failure, not successful absence.

## Static/security validation executed in this environment

- [x] `validate-security-e0.sh` static mode.
- [x] `validate-security-e1.sh` static mode.
- [x] `validate-security-e2.sh` static mode.
- [x] `validate-security-e3.sh` static mode.
- [x] `validate-security-e0-e3-closure.sh` updated 2a.15 targeted remediation regression guard.
- [x] Shell syntax validation for the modified security validators.
- [x] Source-order assertions verify cleanup-before-forget, attachment tracking-before-create/decrypt, and protected journal sweep-before-parse/after-recovery.
- [x] Source scan confirms the removed late-registration/unregister-before-delete helper patterns are absent.
- [x] `git diff --no-index --check` reports no whitespace errors in the changed source/security-validator files.

## Added Rust regressions

- `failed_lock_cleanup_retains_session_and_plaintext_for_retry`
- `tracked_materialization_cannot_promote_after_revocation_begins`
- `concurrent_materialization_reuses_canonical_path_without_orphaning_duplicate`
- `failed_protected_attachment_authentication_leaves_no_untracked_plaintext`
- `startup_discards_corrupt_protected_records_before_quarantine`
- `startup_fails_closed_when_protected_root_record_cannot_be_removed`

## Native-toolchain limitation

This execution environment contains no `cargo`, `rustc` or `rustfmt`, including outside `PATH`. Therefore native formatting, compilation, Clippy and Rust test execution could not be performed here. The source bundle retains the normal validators so those native gates can be run on the development Mac before installation/release.

## Remediation verdict

The three requested defects and the directly connected concurrent-materialization ownership race are fixed at source level. This pass does not claim a new independent full E0–E3 closure certification; that should follow after native Rust gates and a fresh de-novo closure audit.

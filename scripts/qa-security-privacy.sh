#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
from pathlib import Path

owners = {
    Path("crates/rustnote-vault/src/lib.rs"): (
        "aegis256_v1_uses_full_256_bit_authentication_tag",
        "v1_argon2_profile_is_the_untrusted_envelope_resource_ceiling",
        "object_rejects_wrong_key_context_and_tampering",
        "password_envelope_rejects_tampered_kdf_parameters",
        "protected_blob_rejects_wrong_context_tampering_truncation_and_trailing_data",
    ),
    Path("src/security/privacy_coordinator.rs"): (
        "compound_plaintext_operation_finishes_after_lock_admission_without_circular_wait",
        "lock_invalidates_publication_and_reader_authorization_before_reader_drain_completes",
        "lock_b_leaves_note_a_publication_valid_and_undrained",
        "lock_b_invalidates_and_drains_multi_note_publication",
        "whole_library_operation_serializes_against_library_restore",
    ),
    Path("src/security/runtime.rs"): (
        "failed_lock_cleanup_retains_session_and_plaintext_for_retry",
        "concurrent_materialization_reuses_canonical_path_without_orphaning_duplicate",
        "lock_waits_for_active_read_permit_and_rejects_new_reads_after_revocation",
    ),
    Path("src/security/plaintext.rs"): (
        "metadata_failure_is_not_treated_as_plaintext_absence",
        "durable_namespace_establish_refuses_posthoc_adoption_of_nonempty_root",
        "durable_namespace_open_existing_never_recreates_missing_marker",
    ),
    Path("src/storage/plaintext_staging.rs"): (
        "failed_plaintext_cleanup_keeps_durable_owner_for_retry",
        "restart_recovers_durable_rename_after_filesystem_transfer_before_owner_retirement",
    ),
    Path("src/storage/attachments.rs"): (
        "ambiguous_attachment_operation_stage_presence_fails_closed_until_retry",
        "abandoned_pending_add_transfers_ownership_before_operation_retirement",
    ),
    Path("src/storage/protection.rs"): (
        "protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot",
        "no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups",
        "protected_note_scrubs_rustnote_managed_plaintext_residue",
        "vault_recovery_restores_plaintext_retired_before_protection_commit",
        "protection_cleanup_failure_stays_journaled_until_recovery_succeeds",
    ),
    Path("src/storage/backup_repository.rs"): (
        "interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear",
        "restart_recovers_after_repository_removed_before_privacy_obligation_retirement",
        "steady_state_backup_writes_fail_closed_after_namespace_marker_loss",
    ),
    Path("src/autosave.rs"): (
        "vault_barrier_prevents_stale_plaintext_emergency_journal_recreation",
    ),
    Path("src/ui/note_card.rs"): (
        "protect_success_discloses_pending_backup_plaintext_risk",
        "protect_success_is_conservative_when_backup_status_refresh_fails",
    ),
}

missing = []
for path, symbols in owners.items():
    source = path.read_text(errors="ignore")
    for symbol in symbols:
        if f"fn {symbol}" not in source:
            missing.append(f"{path}: {symbol}")
if missing:
    raise SystemExit("security/privacy behavioral owner disappeared:\n- " + "\n- ".join(missing))

vault = Path("crates/rustnote-vault/src/lib.rs").read_text()
for marker in (
    "const TAG_BYTES: usize = 32;",
    "const V1_ARGON2_MEMORY_KIB: u32 = 64 * 1024;",
    "const V1_ARGON2_TIME_COST: u32 = 3;",
    "const V1_ARGON2_LANES: u32 = 4;",
):
    if marker not in vault:
        raise SystemExit(f"security crypto profile marker missing: {marker}")

coordinator = Path("src/security/privacy_coordinator.rs").read_text()
for marker in (
    "PrivacyCoordinator",
    "capture_note",
    "capture_multi_note",
    "begin_library_operation",
):
    if marker not in coordinator:
        raise SystemExit(f"privacy authority marker missing: {marker}")

print("qa-security-privacy: code-owned security/privacy behavior owners are present.")
PY

node --test \
  tests/js/navigation-publication-contract.test.mjs \
  tests/js/note-local-privacy-contract.test.mjs \
  tests/js/multi-note-privacy-contract.test.mjs \
  tests/js/library-wide-privacy-contract.test.mjs \
  tests/js/vault-action-privacy-contract.test.mjs \
  tests/js/vault-profile-contract.test.mjs \
  tests/js/vault-no-vacuum-forensic-contract.test.mjs \
  tests/js/vault-cleanup-2b42-9-contract.test.mjs \
  tests/js/vault-independent-closure-2b42-10-contract.test.mjs

echo "qa-security-privacy: publication, lifetime, residue and recovery contracts passed."

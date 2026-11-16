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
    Path("src/security/plaintext.rs"): (
        "failed_process_private_rename_retains_predecessor_ownership",
        "metadata_failure_is_not_treated_as_plaintext_absence",
        "process_private_delete_failure_keeps_retiring_state_and_owner",
        "durable_namespace_establish_refuses_posthoc_adoption_of_nonempty_root",
    ),
    Path("src/security/privacy_coordinator.rs"): (
        "same_note_transition_conflicts_fail_closed",
        "compound_plaintext_operation_finishes_after_lock_admission_without_circular_wait",
        "lock_invalidates_publication_and_reader_authorization_before_reader_drain_completes",
        "lock_b_leaves_note_a_publication_valid_and_undrained",
        "lock_b_invalidates_and_drains_multi_note_publication",
        "library_restore_invalidates_and_drains_every_publication_scope",
        "whole_library_operation_serializes_against_library_restore",
    ),
    Path("src/security/runtime.rs"): (
        "tracked_materialization_cannot_promote_after_revocation_begins",
        "failed_lock_cleanup_retains_session_and_plaintext_for_retry",
        "concurrent_materialization_reuses_canonical_path_without_orphaning_duplicate",
        "lock_waits_for_active_read_permit_and_rejects_new_reads_after_revocation",
    ),
    Path("src/storage/plaintext_staging.rs"): (
        "startup_verification_rejects_preexisting_unowned_orphan_plaintext",
        "failed_plaintext_cleanup_keeps_durable_owner_for_retry",
        "restart_recovers_durable_rename_after_filesystem_transfer_before_owner_retirement",
    ),
    Path("src/storage/attachment_ingest.rs"): (
        "prepared_attachment_import_is_durable_until_finalization",
        "ambiguous_pending_ingest_presence_keeps_durable_owner_until_retry",
        "pending_attachment_ingest_recovery_replays_managed_stage_once",
    ),
    Path("src/storage/attachments.rs"): (
        "ambiguous_attachment_operation_stage_presence_fails_closed_until_retry",
        "abandoned_pending_add_transfers_ownership_before_operation_retirement",
        "revision_pruning_transfers_revision_only_blob_ownership_before_commit",
    ),
    Path("src/storage/protection.rs"): (
        "protected_revision_preview_resource_holds_lock_until_derivation_lifetime_ends",
        "protected_attachment_command_resource_holds_lock_until_sensitive_lifetime_ends",
        "protected_revision_restore_commits_granular_graph_without_repopulating_plaintext_tables",
        "protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot",
        "no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups",
        "persistent_snapshots_fail_closed_while_vault_operation_is_pending",
        "protected_note_scrubs_rustnote_managed_plaintext_residue",
        "protected_note_encrypts_attachment_blob_and_restores_it_losslessly",
        "failed_protected_attachment_authentication_leaves_no_untracked_plaintext",
        "vault_recovery_restores_plaintext_retired_before_protection_commit",
        "protection_cleanup_failure_stays_journaled_until_recovery_succeeds",
        "pending_attachment_mutations_block_encrypt_until_their_durable_state_is_resolved",
        "revision_only_plaintext_is_removed_from_sqlite_and_wal_on_protection",
    ),
    Path("src/storage/backup_repository.rs"): (
        "repository_snapshot_restore_round_trips_note_and_attachment_through_library_restore",
        "repository_snapshot_restore_recovers_protected_note_and_attachment_ciphertext",
        "snapshot_sanitization_does_not_hide_active_plaintext_staging",
        "interrupted_vault_purge_recovers_after_arbitrary_repository_children_disappear",
        "restart_recovers_after_repository_removed_before_privacy_obligation_retirement",
        "steady_state_backup_writes_fail_closed_after_namespace_marker_loss",
    ),
    Path("src/storage/backups.rs"): (
        "whole_library_restore_retires_old_only_canonical_plaintext",
        "restore_seeds_old_only_cleanup_in_incoming_database_before_snapshot_switch",
        "protected_note_plaintext_and_markdown_exports_fail_closed",
    ),
    Path("src/storage/portability.rs"): (
        "protected_note_portability_exports_fail_closed_for_every_format",
        "undo_import_keeps_blob_cleanup_owned_until_unlink_succeeds",
    ),
    Path("src/storage/notes.rs"): (
        "protected_note_and_attachment_payloads_never_enter_sync_queue",
        "protected_ciphertext_cleanup_owner_recovers_after_database_commit_crash_boundary",
        "permanent_delete_waits_for_plaintext_producer_then_scrubs_late_staging",
    ),
    Path("src/storage/transport.rs"): (
        "protected_pdf_transport_resource_keeps_lock_waiting_until_request_releases_it",
    ),
    Path("src/autosave.rs"): (
        "vault_barrier_prevents_stale_plaintext_emergency_journal_recreation",
        "unlocked_protected_note_autosaves_and_close_checkpoints_without_plaintext_journal",
    ),
    Path("src/autosave/emergency_journal.rs"): (
        "protected_recovery_cleanup_fails_closed_when_plaintext_cannot_be_removed",
        "startup_removes_legacy_quarantine_for_protected_note",
        "protected_recovery_cleanup_removes_known_plaintext_record",
    ),
    Path("src/attachment_protocol.rs"): (
        "protected_pdf_annotation_guard_keeps_lock_waiting_until_response_scope_drops",
        "protected_pdf_file_handle_keeps_lock_waiting_until_descriptor_drops",
    ),
    Path("src/media_server.rs"): (
        "protected_media_file_keeps_lock_waiting_until_file_handle_drops",
    ),
    Path("src/native/staging.rs"): (
        "strict_plaintext_cleanup_does_not_accept_ambiguous_absence",
        "derivative_stage_identity_is_enforced_and_targeted_cleanup_is_scoped",
    ),
    Path("src/native/temp_owner.rs"): (
        "live_owner_lock_prevents_cleanup",
        "abandoned_owner_lock_is_removed_without_pid_heuristics",
        "v2_owner_setup_crash_window_is_removed_without_pid_heuristics",
    ),
    Path("src/ui/privacy.rs"): (
        "note_revocation_changes_only_the_note_and_multi_note_generations",
        "multi_note_revocation_changes_only_the_multi_note_generation",
        "library_revocation_invalidates_every_ui_scope_without_per_note_iteration",
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
    "capture_note_publication_authorization",
    "capture_multi_note_publication_authorization",
    "begin_whole_library_operation",
):
    if marker not in coordinator:
        raise SystemExit(f"privacy authority marker missing: {marker}")

print("qa-security-privacy: de-novo security/privacy owner traceability is present.")
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

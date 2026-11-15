#!/bin/zsh
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2a.19" || "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
SCRIPT_SHELL="$(command -v zsh || command -v bash)"
RUSTNOTE_E0_STATIC_ONLY=1 "$SCRIPT_SHELL" ./scripts/validate-security-e0.sh
RUSTNOTE_E1_STATIC_ONLY=1 "$SCRIPT_SHELL" ./scripts/validate-security-e1.sh
RUSTNOTE_E2_STATIC_ONLY=1 "$SCRIPT_SHELL" ./scripts/validate-security-e2.sh
RUSTNOTE_E3_STATIC_ONLY=1 "$SCRIPT_SHELL" ./scripts/validate-security-e3.sh
"$SCRIPT_SHELL" ./scripts/validate-security-e0-e3-ownership-2a16.sh
"$SCRIPT_SHELL" ./scripts/validate-security-e0-e3-residue-2a17.sh
"$SCRIPT_SHELL" ./scripts/validate-security-e0-e3-ui-lifetime-2a18.sh
"$SCRIPT_SHELL" ./scripts/validate-security-e0-e3-adversarial-closure-2a19.sh
python3 - <<'PY2'
from pathlib import Path
runtime=Path('src/security/runtime.rs').read_text()
protection=Path('src/storage/protection.rs').read_text()
journal=Path('src/autosave/emergency_journal.rs').read_text()
for marker in (
    'failed_lock_cleanup_retains_session_and_plaintext_for_retry',
    'tracked_materialization_cannot_promote_after_revocation_begins',
    'concurrent_materialization_reuses_canonical_path_without_orphaning_duplicate',
    'Snapshot without unregistering',
):
    if marker not in runtime:
        raise SystemExit(f'2a.15 Lock/runtime remediation missing: {marker}')
for marker in (
    'OwnedPlaintext::reserve_vault_session(',
    'promote_vault_materialized_attachment(note_id, attachment.id.clone())',
    'failed_protected_attachment_authentication_leaves_no_untracked_plaintext',
):
    if marker not in protection:
        raise SystemExit(f'2a.15 protected-attachment remediation missing: {marker}')
for marker in (
    'discard_protected_journal_records(storage, &directory)?',
    'startup_discards_corrupt_protected_records_before_quarantine',
    'startup_fails_closed_when_protected_root_record_cannot_be_removed',
):
    if marker not in journal:
        raise SystemExit(f'2a.15 emergency-journal remediation missing: {marker}')
PY2
echo "E0-E3 2a.19 cumulative independent adversarial closure guard passed."

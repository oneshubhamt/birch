#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path


def read(path):
    return Path(path).read_text(errors="ignore")


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f"{label} missing: {marker}")


def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f"{label} contains forbidden marker: {marker}")


def segment(text, start, end):
    i = text.find(start)
    if i < 0:
        raise SystemExit(f"missing segment start: {start}")
    j = text.find(end, i + len(start))
    if j < 0:
        raise SystemExit(f"missing segment end: {end}")
    return text[i:j]

protection = read("src/storage/protection.rs")
profiler = read("src/vault_profile.rs")
runner = read("scripts/run-browser-behavior-tests.mjs")
implementation = read("VAULT_2B42_8_NO_VACUUM_FORENSIC.md")
qa = read("QA_VAULT_2B42_8_NO_VACUUM_FORENSIC.md")

# 1. The experiment is closed: production has one canonical path, with no
# residual mode abstraction, test-only alternate helper, runtime switch, VACUUM
# SQL statement, or obsolete profiling stage.
forbid(protection, [
    "SqliteResidueScrubMode",
    "NoVacuumForensicTest",
    "protect_note_with_password_without_vacuum_forensic",
    'execute_batch("VACUUM;")',
    "RUSTNOTE_NO_VACUUM",
    "RUSTNOTE_VAULT_NO_VACUUM",
], "canonical no-VACUUM production path")
forbid(profiler, ["VaultProfileStage::Vacuum", 'Self::Vacuum => "vacuum"'], "profiler vocabulary")

# 2. Production Protect itself reaches the canonical managed-residue scrub.
production = segment(
    protection,
    "pub(crate) fn protect_note_with_password_and_public_label(",
    "/// Close Rustnote-managed plaintext residues after the canonical protection",
)
require(production, ["scrub_managed_plaintext_residue_after_protection("], "production Protect")

# 3. Canonical SQLite residue cleanup is the proved sequence:
# secure_delete/full sync -> FTS rebuild -> exactly one verified WAL truncate.
scrub = segment(
    protection,
    "fn scrub_sqlite_plaintext_residue_unlocked(",
    "/// Durable Vault transitions block autosave mutation admission",
)
require(scrub, [
    "PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;",
    "INSERT INTO notes_fts(notes_fts) VALUES('rebuild');",
    "ensure_wal_truncated(&connection)",
], "SQLite forensic cleanup")
if scrub.count("ensure_wal_truncated(&connection)") != 1:
    raise SystemExit("SQLite forensic cleanup must contain exactly one verified WAL truncation")
forbid(scrub, ['execute_batch("VACUUM;")', "SqliteResidueScrubMode"], "SQLite forensic cleanup")

# 4. Native forensic regression seeds historical-only and current secrets and
# exercises the canonical production Protect path.
forensic_test = segment(
    protection,
    "fn no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups()",
    "#[test]\n    fn persistent_snapshots_fail_closed_while_vault_operation_is_pending()",
)
require(forensic_test, [
    "RUSTNOTE-E3-NOVAC-HISTORICAL-",
    "RUSTNOTE-E3-NOVAC-CURRENT-",
    "historical revision plaintext must exist before the forensic Protect",
    "current plaintext must exist before the forensic Protect",
    '.protect_note_with_password(&note.id, b"correct horse battery staple")',
], "production no-VACUUM forensic regression")

# 5. Database and WAL are inspected independently at byte level.
require(forensic_test, [
    "let database_bytes = fs::read(&storage.database_path)",
    "let wal_bytes = file_bytes_if_present(&wal_path);",
    "production no-VACUUM Protect must remove {marker} from the live database bytes",
    "production no-VACUUM Protect must remove {marker} from WAL bytes",
], "database/WAL forensic assertions")

# 6. Managed backup history is proven dirty before Protect, invalidated, and
# clean again after a fresh post-Protect snapshot.
require(forensic_test, [
    "the test must prove a Rustnote-managed backup contained historical plaintext before Protect",
    "!pre_protection_backup.exists()",
    "managed backup repository must not retain {marker} after Protect",
    "post-Protect backup snapshot must not contain {marker}",
    "managed backup repository must remain free of {marker} after the new snapshot",
], "managed backup forensic assertions")

# 7. The older single-marker production residue regression now names the final
# secure_delete + FTS + WAL sequence, and the dependency-free runner executes
# the 2b.42.8 source contracts.
production_test = segment(
    protection,
    "fn protected_note_fts_plaintext_is_absent_from_database_wal_and_backup_snapshot()",
    "#[test]\n    fn no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups()",
)
require(production_test, [
    "secure_delete + FTS rebuild + WAL truncation must remove protected plaintext bytes",
], "production forensic regression")
require(runner, ["vault-no-vacuum-forensic-contract.test.mjs"], "behavior runner")

# 8. Documentation records the native decision evidence and final production
# choice instead of leaving the experiment open-ended.
require(implementation, [
    "Native decision result: PASS",
    "Production decision: remove per-Protect `VACUUM`",
    "secure_delete + FTS5 rebuild + verified WAL TRUNCATE",
], "2b.42.8 implementation record")
require(qa, [
    "Native macOS forensic test: PASS",
    "Production VACUUM removed",
    "Final native closure gate",
], "2b.42.8 QA record")

print("2b.42.8 no-VACUUM closure validation passed (8/8 contracts).")
PY

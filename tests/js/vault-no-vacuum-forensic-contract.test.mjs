import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const protection = fs.readFileSync(path.join(root, 'src', 'storage', 'protection.rs'), 'utf8');
const profiler = fs.readFileSync(path.join(root, 'src', 'vault_profile.rs'), 'utf8');

function segment(text, start, end) {
  const begin = text.indexOf(start);
  assert.ok(begin >= 0, `missing segment start: ${start}`);
  const finish = text.indexOf(end, begin + start.length);
  assert.ok(finish >= 0, `missing segment end: ${end}`);
  return text.slice(begin, finish);
}

test('production Protect uses one canonical no-VACUUM residue path with no experimental mode abstraction', () => {
  const production = segment(
    protection,
    'pub(crate) fn protect_note_with_password_and_public_label(',
    '/// Close Rustnote-managed plaintext residues after the canonical protection',
  );
  assert.match(production, /scrub_managed_plaintext_residue_after_protection/);
  assert.doesNotMatch(protection, /SqliteResidueScrubMode|NoVacuumForensicTest|protect_note_with_password_without_vacuum_forensic/);
});

test('production source has no VACUUM SQL, no VACUUM profiler stage, and no runtime bypass switch', () => {
  assert.doesNotMatch(protection, /execute_batch\("VACUUM;"\)|SqliteResidueScrubMode/);
  assert.doesNotMatch(profiler, /VaultProfileStage::Vacuum|Self::Vacuum\s*=>\s*"vacuum"/);
  assert.doesNotMatch(protection, /RUSTNOTE_.*VACUUM|NO_VACUUM.*env|std::env.*vacuum/i);
});

test('canonical SQLite scrub retains secure delete, FTS rebuild, and exactly one verified WAL truncation', () => {
  const scrub = segment(
    protection,
    'fn scrub_sqlite_plaintext_residue_unlocked(',
    '/// Durable Vault transitions block autosave mutation admission',
  );
  assert.match(scrub, /PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;/);
  assert.match(scrub, /INSERT INTO notes_fts\(notes_fts\) VALUES\('rebuild'\)/);
  assert.equal((scrub.match(/ensure_wal_truncated\(&connection\)/g) || []).length, 1);
  assert.doesNotMatch(scrub, /execute_batch\("VACUUM;"\)|SqliteResidueScrubMode/);
});

test('forensic residue test seeds both historical-only and current plaintext before production Protect', () => {
  const forensicTest = segment(
    protection,
    'fn no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups()',
    '#[test]\n    fn persistent_snapshots_fail_closed_while_vault_operation_is_pending()',
  );
  assert.match(forensicTest, /RUSTNOTE-E3-NOVAC-HISTORICAL/);
  assert.match(forensicTest, /RUSTNOTE-E3-NOVAC-CURRENT/);
  assert.match(forensicTest, /historical revision plaintext must exist before the forensic Protect/);
  assert.match(forensicTest, /current plaintext must exist before the forensic Protect/);
  assert.match(forensicTest, /\.protect_note_with_password\(&note\.id, b"correct horse battery staple"\)/);
});

test('forensic residue test checks database and WAL bytes separately after production no-VACUUM Protect', () => {
  const forensicTest = segment(
    protection,
    'fn no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups()',
    '#[test]\n    fn persistent_snapshots_fail_closed_while_vault_operation_is_pending()',
  );
  assert.match(forensicTest, /let database_bytes = fs::read\(&storage\.database_path\)/);
  assert.match(forensicTest, /let wal_bytes = file_bytes_if_present\(&wal_path\)/);
  assert.match(forensicTest, /production no-VACUUM Protect must remove \{marker\} from the live database bytes/);
  assert.match(forensicTest, /production no-VACUUM Protect must remove \{marker\} from WAL bytes/);
});

test('forensic residue test proves managed backups are invalidated and remain plaintext-free after recreation', () => {
  const forensicTest = segment(
    protection,
    'fn no_vacuum_forensic_protect_removes_historical_and_current_plaintext_from_db_wal_and_managed_backups()',
    '#[test]\n    fn persistent_snapshots_fail_closed_while_vault_operation_is_pending()',
  );
  assert.match(forensicTest, /maybe_create_automatic_backup\(\)/);
  assert.match(forensicTest, /the test must prove a Rustnote-managed backup contained historical plaintext before Protect/);
  assert.match(forensicTest, /!pre_protection_backup\.exists\(\)/);
  assert.match(forensicTest, /managed backup repository must not retain \{marker\} after Protect/);
  assert.match(forensicTest, /post-Protect backup snapshot must not contain \{marker\}/);
  assert.match(forensicTest, /managed backup repository must remain free of \{marker\} after the new snapshot/);
});

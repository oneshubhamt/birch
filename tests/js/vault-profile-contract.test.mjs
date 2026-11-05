import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const read = (relative) => readFileSync(path.join(root, relative), 'utf8');

const profile = read('src/vault_profile.rs');
const ui = read('src/ui/note_card.rs');
const protection = read('src/storage/protection.rs');
const applicationVault = read('src/application/vault.rs');
const lib = read('src/lib.rs');

function segment(text, start, end) {
  const from = text.indexOf(start);
  assert.ok(from >= 0, `missing segment start: ${start}`);
  const to = text.indexOf(end, from + start.length);
  assert.ok(to >= 0, `missing segment end: ${end}`);
  return text.slice(from, to);
}

test('Vault profiling is exact opt-in and its disabled path avoids timestamps and formatting', () => {
  assert.match(profile, /const VAULT_PROFILE_ENV: &str = "RUSTNOTE_VAULT_PROFILE"/);
  assert.match(profile, /static VAULT_PROFILE_ENABLED: OnceLock<bool>/);
  assert.match(profile, /value == "1"/);
  assert.match(profile, /started_at: vault_profile_enabled\(\)\.then\(Instant::now\)/);
  assert.match(profile, /let Some\(started_at\) = self\.started_at else \{[\s\S]*?return;/);
  assert.match(profile, /\[rustnote-vault-profile\] kind=\{\} name=\{\} elapsed_us=\{\}/);
  const output = segment(profile, 'impl Drop for VaultProfileSpan', '#[inline]\nfn vault_profile_enabled');
  assert.doesNotMatch(output, /note_id|password|public_label|file_name|path|sha256|byte_size|content/);
  assert.match(lib, /mod vault_profile;/);
});

test('profiling vocabulary is closed over the four operations and required static stages', () => {
  for (const marker of [
    'Protect',
    'Lock',
    'Unlock',
    'RemoveEncryption',
    'UiCheckpoint',
    'AutosaveBarrier',
    'AutosaveRelease',
    'PrivacyDrains',
    'ProtectPreparation',
    'Crypto',
    'AttachmentsEncrypt',
    'AttachmentsStage',
    'AttachmentsCleanup',
    'ManagedResidueScrub',
    'CacheCleanup',
    'StagingCleanup',
    'HandoffCleanup',
    'BackupRepositoryReset',
    'SqliteResidueScrub',
    'TransitionFinalize',
    'AttachmentsDecrypt',
    'AttachmentsFinalize',
    'ObjectWrites',
    'Fts',
    'Wal',
    'CanonicalPublication',
  ]) {
    assert.match(profile, new RegExp(`\\b${marker}\\b`));
  }
  assert.match(profile, /fn new\(kind: VaultProfileKind, name: &'static str\)/);
  assert.doesNotMatch(profile, /pub\(crate\) fn new\(/);
});

test('UI controller profiles end-to-end operations, checkpoint, and canonical publication only for Vault actions', () => {
  const mapping = segment(ui, 'fn vault_profile_operation(', 'fn vault_action_progress_message(');
  assert.match(mapping, /Encrypt \{ \.\. \} => Some\(VaultProfileOperation::Protect\)/);
  assert.match(mapping, /UnlockEncrypted \{ \.\. \} => Some\(VaultProfileOperation::Unlock\)/);
  assert.match(mapping, /LockEncrypted\(_\) => Some\(VaultProfileOperation::Lock\)/);
  assert.match(mapping, /RemoveEncryption \{ \.\. \} => Some\(VaultProfileOperation::RemoveEncryption\)/);
  assert.match(mapping, /_ => None/);

  const action = segment(ui, 'pub(super) fn apply_note_card_action(', 'pub(super) fn apply_note_card_tag_action(');
  assert.match(action, /profile_operation\.map\(VaultProfileSpan::operation\)/);
  assert.match(action, /vault_profiled_action[\s\S]*?VaultProfileStage::UiCheckpoint/);
  assert.ok((action.match(/VaultProfileStage::CanonicalPublication/g) ?? []).length >= 2);
  assert.match(action, /vault_profiled_action[\s\S]*?VaultProfileStage::CanonicalPublication/);
});

test('Protect profiles privacy admission, crypto, attachment work, and object writes', () => {
  const protect = segment(
    protection,
    'pub(crate) fn protect_note_with_password_and_public_label(',
    'pub(crate) fn protected_note_ids_for_privacy_recovery(',
  );
  const privacy = protect.indexOf('VaultProfileStage::PrivacyDrains');
  const preparation = protect.indexOf('VaultProfileStage::ProtectPreparation');
  const crypto = protect.indexOf('VaultProfileStage::Crypto');
  const attachmentEncrypt = protect.indexOf('VaultProfileStage::AttachmentsEncrypt');
  const attachmentStage = protect.indexOf('VaultProfileStage::AttachmentsStage');
  const objectWrites = protect.indexOf('VaultProfileStage::ObjectWrites');
  const attachmentCleanup = protect.indexOf('VaultProfileStage::AttachmentsCleanup');
  const managedResidue = protect.indexOf('VaultProfileStage::ManagedResidueScrub');
  const transitionFinalize = protect.indexOf('VaultProfileStage::TransitionFinalize');
  assert.ok(privacy >= 0 && preparation > privacy && crypto > preparation && attachmentEncrypt > crypto && attachmentStage > attachmentEncrypt);
  assert.ok(objectWrites > attachmentStage && attachmentCleanup > objectWrites);
  assert.ok(managedResidue > attachmentCleanup && transitionFinalize > managedResidue);
});

test('Lock, Unlock, and Remove Encryption expose their relevant storage stages', () => {
  const unlock = segment(protection, 'pub(crate) fn unlock_protected_note(', 'pub(crate) fn lock_protected_note(');
  assert.match(unlock, /VaultProfileStage::PrivacyDrains/);
  assert.match(unlock, /VaultProfileStage::Crypto/);

  const lock = segment(protection, 'pub(crate) fn lock_protected_note(', 'pub(crate) fn remove_note_protection_with_password(');
  assert.match(lock, /VaultProfileStage::PrivacyDrains/);
  assert.match(lock, /VaultProfileStage::AttachmentsCleanup/);

  const unprotect = segment(protection, 'pub(crate) fn remove_note_protection_with_password(', 'pub(super) fn hydrate_protected_note(');
  assert.match(unprotect, /VaultProfileStage::PrivacyDrains/);
  assert.match(unprotect, /VaultProfileStage::Crypto/);
  assert.match(unprotect, /VaultProfileStage::AttachmentsDecrypt/);
  assert.match(unprotect, /VaultProfileStage::ObjectWrites/);
  assert.match(unprotect, /VaultProfileStage::AttachmentsFinalize/);
});


test('application Vault workflow profiles autosave barrier acquisition and release without changing calls', () => {
  assert.match(applicationVault, /VaultProfileStage::AutosaveBarrier/);
  assert.match(applicationVault, /block_note_for_vault\(note_id\)/);
  assert.match(applicationVault, /VaultProfileStage::AutosaveRelease/);
  assert.match(applicationVault, /unblock_note_after_vault\(note_id\)/);
});

test('Protect managed residue scrub is decomposed into static cleanup stages', () => {
  const scrub = segment(
    protection,
    'fn scrub_managed_plaintext_residue_after_hashes(',
    '/// Protected note identifiers are exposed',
  );
  const cache = scrub.indexOf('VaultProfileStage::CacheCleanup');
  const staging = scrub.indexOf('VaultProfileStage::StagingCleanup');
  const handoff = scrub.indexOf('VaultProfileStage::HandoffCleanup');
  const backup = scrub.indexOf('VaultProfileStage::BackupRepositoryReset');
  const sqlite = scrub.indexOf('VaultProfileStage::SqliteResidueScrub');
  assert.ok(cache >= 0 && staging > cache && handoff > staging && backup > handoff && sqlite > backup);
  assert.match(scrub, /discard_image_rendition_cache_strict/);
  assert.match(scrub, /discard_staged_derivatives_for_note/);
  assert.match(scrub, /discard_handoffs_for_source_keys/);
  assert.match(scrub, /reset_backup_repository_for_vault_unlocked/);
  assert.match(scrub, /scrub_sqlite_plaintext_residue_unlocked/);
});

test('SQLite forensic cleanup profiles FTS and the verified WAL truncation in canonical order', () => {
  const scrub = segment(
    protection,
    'fn scrub_sqlite_plaintext_residue_unlocked(',
    '/// Durable Vault transitions block autosave mutation admission',
  );
  const fts = scrub.indexOf('VaultProfileStage::Fts');
  const rebuild = scrub.indexOf("INSERT INTO notes_fts(notes_fts) VALUES('rebuild')");
  const wal = scrub.indexOf('VaultProfileStage::Wal');
  const truncate = scrub.indexOf('ensure_wal_truncated(&connection)');
  assert.ok(fts >= 0 && rebuild > fts && wal > rebuild && truncate > wal);
  assert.equal((scrub.match(/VaultProfileStage::Wal/g) || []).length, 1);
  assert.equal((scrub.match(/ensure_wal_truncated\(&connection\)/g) || []).length, 1);
  assert.doesNotMatch(scrub, /VaultProfileStage::Vacuum|execute_batch\("VACUUM;"\)/);
});

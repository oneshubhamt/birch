import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (relative) => readFileSync(path.join(root, relative), 'utf8');

test('2b.42.9 removes dead-code suppression without hiding parse-only schema fields', () => {
  const all = [
    read('src/editor_document_spec.rs'),
    read('src/security/vault_object_spec.rs'),
  ].join('\n');
  assert.doesNotMatch(all, /allow\(dead_code\)|expect\(dead_code\)/);
  assert.match(all, /_prosemirror: Option<Value>/);
  assert.match(all, /_default_value: Option<Value>/);
});

test('2b.42.9 leaves no post-hoc durable namespace adoption escape hatch', () => {
  const plaintext = read('src/security/plaintext.rs');
  assert.doesNotMatch(plaintext, /adopt_existing_with_proof|allow_nonempty_adoption/);
  assert.match(plaintext, /if root_nonempty \{/);
  assert.match(plaintext, /marker is missing for a nonempty managed root/);
});

test('2b.42.9 backup repository uses only external durable ownership', () => {
  const backup = read('src/storage/backup_repository.rs');
  assert.match(backup, /\.rustnote-backup-repository-owner/);
  assert.doesNotMatch(backup, /INLINE_OWNER|InlineRepositoryOwner|inline_owner/);
  assert.match(backup, /storage_refuses_nonempty_repository_without_durable_namespace_authority/);
});

test('2b.42.9 backup history accepts only the current identity-bearing format', () => {
  const locations = read('src/storage/locations.rs');
  assert.match(locations, /if history\.version != BACKUP_LOCATION_HISTORY_VERSION/);
  assert.doesNotMatch(locations, /1 \| 2 \| BACKUP_LOCATION_HISTORY_VERSION/);
  assert.doesNotMatch(locations, /library_id\.is_empty\(\)/);
  assert.match(locations, /backup_history_rejects_precurrent_versions/);
});

test('2b.42.9 editor synchronization owns its publication capture helper', () => {
  const sync = read('src/ui/synchronization.rs');
  assert.match(sync, /fn capture_publication\(&self\)/);
  assert.match(sync, /context\.capture_publication\(\)/);
  assert.doesNotMatch(sync, /capture_current_editor_publication/);
});


import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');

const plaintext = read('src/security/plaintext.rs');
const runtime = read('src/security/runtime.rs');
const protection = read('src/storage/protection.rs');
const ingest = read('src/storage/attachment_ingest.rs');
const attachments = read('src/storage/attachments.rs');
const tempOwner = read('src/native/temp_owner.rs');

test('managed plaintext absence is NotFound-only', () => {
  assert.match(plaintext, /pub\(crate\) fn managed_path_is_present/);
  assert.match(plaintext, /ErrorKind::NotFound\s*=>\s*Ok\(false\)/);
  assert.match(plaintext, /Err\(error\)\s*=>\s*Err\(error\)/);
});

test('Vault materialization refuses ambiguous predecessor replacement', () => {
  assert.match(runtime, /managed_path_is_present\(&existing\)/);
  assert.match(runtime, /could not prove prior vault materialization is absent/);
  assert.match(runtime, /ambiguous_materialized_attachment_presence_fails_closed_without_replacement/);
});

test('durable attachment owners survive ambiguous path observation', () => {
  assert.match(ingest, /ambiguous_pending_ingest_presence_keeps_durable_owner_until_retry/);
  assert.match(attachments, /ambiguous_attachment_operation_stage_presence_fails_closed_until_retry/);
});

test('Protect rollback uses typed conclusive path observation', () => {
  assert.match(protection, /if !blob\.plaintext\.path_entry_exists\(\)\?/);
  assert.match(protection, /retired_plaintext_restore_rejects_ambiguous_absence_and_retries_after_repair/);
});

test('temporary-session cleanup never interprets metadata error as dead ownership', () => {
  assert.match(tempOwner, /fn valid_owner_marker\(path: &Path\) -> io::Result<bool>/);
  assert.match(tempOwner, /ambiguous_owner_marker_metadata_never_authorizes_cleanup/);
});

test('2b.42 cleanup leaves no unused unauthorised derived-image Storage façade', () => {
  assert.doesNotMatch(attachments, /\bpub\s+fn\s+add_derived_image_attachment_file\s*\(/);
  assert.match(attachments, /add_derived_image_attachment_file_authorized/);
});

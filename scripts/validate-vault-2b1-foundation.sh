#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

for required in \
  VAULT_2B1_AUDIT_PLAN.md \
  src/security/sensitive_data.rs \
  src/security/vault_object_spec.rs \
  src/storage/vault_objects.rs; do
  [[ -f "$required" ]] || { echo "Missing 2b.1 foundation file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path
import re
import sqlite3

build_id = Path('build-id.txt').read_text().strip()


def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f"{label} missing: {marker}")

schema = Path('src/storage/schema.rs').read_text()
init = Path('src/storage/initialization.rs').read_text()
sensitive = Path('src/security/sensitive_data.rs').read_text()
object_spec = Path('src/security/vault_object_spec.rs').read_text()
objects = Path('src/storage/vault_objects.rs').read_text()
vault = Path('crates/rustnote-vault/src/lib.rs').read_text()
note_domain = Path('src/domain/note.rs').read_text()
protection = Path('src/storage/protection.rs').read_text()
plan = Path('VAULT_2B1_AUDIT_PLAN.md').read_text()

require(plan, [
    'Current note private state', 'Private metadata', 'Current attachment metadata',
    'Revision history', 'Revision attachment references', 'Blob mapping',
    'Public structural state', 'Explicit pass boundary',
], '2b.1 pre-edit audit')

require(sensitive, [
    'pub(crate) const SENSITIVE_NOTE_DATA_SPEC_VERSION: u16 = 1;',
    f"pub(crate) const VAULT_NOTE_FORMAT_VERSION: u16 = {3 if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') else 2};",
    'struct ProtectedPublicPersistenceSpec',
    'pub(crate) const NOTE_COLUMNS',
    'pub(crate) const NOTE_SANITIZED_COLUMNS',
    'pub(crate) const ATTACHMENT_COLUMNS',
    'pub(crate) const ATTACHMENT_SANITIZED_COLUMNS',
    'pub(crate) const VAULT_OBJECT_COLUMNS',
    'pub(crate) const VAULT_ROOT_COLUMNS',
    'pub(crate) const CURRENT_PROTECTED_NOTE_SAFE_TABLES',
    'pub(crate) const CURRENT_PROTECTED_NOTE_FORBIDDEN_TABLES',
    'pub(crate) const CURRENT_PROTECTED_NOTE_SPECIAL_SURFACES',
    'pub(crate) const SENSITIVE_RELATIONAL_SURFACES',
    'pub(crate) const DERIVED_SENSITIVE_FIELDS',
    'enum ProtectedScrubStep',
    'pub(crate) const SCRUB_STEPS',
    'sensitive_relational_surfaces',
    'scrub_plan_covers_every_declared_sensitive_relational_surface',
    'pub(crate) struct NotePrivateState',
    'pub(crate) struct MetadataState',
    'pub(crate) struct AttachmentIndex',
    'pub(crate) struct RevisionObject', 'previous_revision_object: Option<VaultObjectId>',
    'pub(crate) struct SensitiveNoteData',
    'Deliberately **not** serializable',
    '#[serde(deny_unknown_fields)]',
    'let Note {', 'let NoteDocument {',
], 'canonical sensitive-data authority')
if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(sensitive, [
        'pub(crate) const REVISION_INDEX_FORMAT_VERSION: u16 = 1;',
        'pub(crate) const VAULT_MANIFEST_FORMAT_VERSION: u16 = 2;',
        'pub(crate) struct RevisionIndexEntry',
        'pub(crate) struct RevisionIndex',
    ], '2b.11 encrypted revision index authority')
if sensitive.count('snapshot_json.fill(0);') < 3:
    raise SystemExit('revision-object temporary plaintext serialization is not wiped on all expected paths')

# The logical aggregate itself must never derive Serialize: that would make it
# too easy to recreate the monolithic protected-note payload.
aggregate_header = sensitive[sensitive.index('pub(crate) struct SensitiveNoteData')-180:sensitive.index('pub(crate) struct SensitiveNoteData')]
if 'Serialize' in aggregate_header or 'Deserialize' in aggregate_header:
    raise SystemExit('SensitiveNoteData logical aggregate must remain non-serializable')

# Default-private compile-time classification proof: every current Note and
# NoteDocument field must appear in the exhaustive classifier destructuring,
# with no `..` catch-all.
def struct_fields(source, struct_name):
    m = re.search(rf'pub struct {struct_name}\s*\{{(.*?)\n\}}', source, re.S)
    if not m:
        raise SystemExit(f'Could not parse {struct_name} fields')
    fields=[]
    for line in m.group(1).splitlines():
        line=line.strip()
        if not line or line.startswith('///') or line.startswith('#['):
            continue
        fm=re.match(r'pub\s+([A-Za-z_][A-Za-z0-9_]*)\s*:', line)
        if fm:
            fields.append(fm.group(1))
    return fields

classifier = sensitive[sensitive.index('pub(crate) fn classify('):sensitive.index('pub(crate) fn validate(&self)')]
if re.search(r'let Note\s*\{.*\.\.', classifier, re.S) or re.search(r'let NoteDocument\s*\{.*\.\.', classifier, re.S):
    raise SystemExit('Sensitive note classifier may not use `..` catch-all destructuring')
for field in struct_fields(note_domain, 'Note'):
    if not re.search(rf'\b{re.escape(field)}\b', classifier):
        raise SystemExit(f'Note field is not classified by SensitiveNoteData::classify: {field}')
for field in struct_fields(note_domain, 'NoteDocument'):
    if not re.search(rf'\b{re.escape(field)}\b', classifier):
        raise SystemExit(f'NoteDocument field is not classified by SensitiveNoteData::classify: {field}')

require(object_spec, [
    'pub(crate) struct VaultObjectId',
    'pub(crate) const VAULT_OBJECT_ID_BYTES: usize = 16;',
    'pub(crate) const VAULT_OBJECT_ID_HEX_LEN',
    'pub(crate) fn parse',
], 'opaque Vault object identity')

require(objects, [
    'enum VaultObjectKind', 'Manifest', 'NotePrivateState', 'MetadataState',
    'AttachmentIndex', 'Revision',
    'const VAULT_OBJECT_CONTEXT_VERSION: u16 = 1;',
    'vault_object_context_bytes', 'note_id.as_str().as_bytes()',
    'kind: VaultObjectKind', 'object_id: &VaultObjectId', 'format_version: u16',
    'trait VaultObjectPayload', 'insert_vault_object', 'read_vault_object',
    'read_vault_revision_chain',
    'install_vault_note_root', 'verify_object_exists',
    'let manifest: VaultManifest =',
    'struct VaultManifest', 'schema_generation: u64', 'revision_head',
    'revision_floor', 'revision_count',
    'ciphertext_cannot_open_under_a_different_logical_object_context',
    'granular_object_store_round_trips_with_authenticated_root',
], 'typed encrypted-object store')
if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(objects, [
        'RevisionIndex', 'revision_index: VaultObjectId',
        'read_vault_revision_index', 'read_vault_revision_by_handle',
    ], '2b.11 direct encrypted revision lookup')

manifest_block = objects[objects.index('pub(crate) struct VaultManifest'):objects.index('pub(crate) trait VaultObjectPayload')]
if 'revision_objects: Vec<' in manifest_block:
    raise SystemExit('VaultManifest must not scale with revision-history length')
if 'revision_head: Option<VaultObjectId>' not in manifest_block or 'revision_floor: Option<VaultObjectId>' not in manifest_block or 'revision_count: u64' not in manifest_block:
    raise SystemExit('VaultManifest is missing O(1) revision-chain bounds')
install_block = objects[objects.index('pub(crate) fn install_vault_note_root'):objects.index('pub(crate) fn vault_note_root_record')]
if 'for revision' in install_block or 'revision_objects' in install_block:
    raise SystemExit('Root installation must not scan revision history')
if 'read_vault_object(transaction, note_key, note_id, manifest_object_id)' not in install_block:
    raise SystemExit('Root installation must authenticate the exact manifest object being committed')
read_block = objects[objects.index('pub(crate) fn read_vault_object'):objects.index('/// Install/replace the canonical encrypted root')]
if 'plaintext.fill(0);' not in read_block:
    raise SystemExit('Vault object reader must wipe decrypted serialization bytes before returning')

require(protection, [
    'SensitivePdfAnnotation', 'SensitiveSourceLink', 'VaultBlobReference',
], '2a payload reuse of canonical sensitive DTOs')
for obsolete in ('struct ProtectedSourceLink', 'struct ProtectedPdfAnnotation', 'struct ProtectedBlobRecord'):
    if obsolete in protection:
        raise SystemExit(f'2b.1 left a duplicate privacy DTO in the legacy payload: {obsolete}')

vault_domains = [
    'Manifest = 6', 'NotePrivateState = 7', 'NoteMetadata = 8', 'AttachmentIndex = 9',
]
if build_id in ('2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    vault_domains.append('RevisionIndex = 10')
require(vault, vault_domains, 'vault object domains')

if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    require(schema, [
        'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;',
        'pub(super) const BASE_SCHEMA',
        'CREATE TABLE vault_objects', 'CREATE TABLE vault_note_roots',
        'ciphertext  BLOB NOT NULL CHECK(length(ciphertext) > 0)',
        'key_envelope        BLOB NOT NULL CHECK(length(key_envelope) > 0)',
        'manifest_object_id', 'FOREIGN KEY(note_id, manifest_object_id)',
        'REFERENCES vault_objects(note_id, object_id)',
        'CREATE TABLE vault_blob_refs',
        'transfer_destination  TEXT', 'transfer_state        TEXT',
        'PRAGMA user_version = 21;',
    ], 'schema-21 canonical Vault object foundation')
    if 'const SCHEMA_' in schema or 'protected_notes' in schema or 'protected_note_blobs' in schema:
        raise SystemExit('2b.5 retained obsolete development-schema migration/monolithic table compatibility')
    require(init, ['0 => {', 'transaction.execute_batch(BASE_SCHEMA)?;', 'CURRENT_SCHEMA_VERSION => Ok(())'], 'fresh/current-only schema initialization')
else:
    if build_id == '2b.4':
        require(schema, [
            'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;',
            'pub(super) const SCHEMA_18_TO_19',
            'CREATE TABLE vault_objects', 'CREATE TABLE vault_note_roots',
            'ciphertext  BLOB NOT NULL CHECK(length(ciphertext) > 0)',
            'key_envelope        BLOB NOT NULL CHECK(length(key_envelope) > 0)',
            'manifest_object_id', 'FOREIGN KEY(note_id, manifest_object_id)',
            'REFERENCES vault_objects(note_id, object_id)', 'PRAGMA user_version = 19;',
            'pub(super) const SCHEMA_19_TO_20', 'PRAGMA user_version = 20;',
            'pub(super) const SCHEMA_20_TO_21', 'CREATE TABLE vault_blob_refs',
            'DROP TABLE protected_notes;', 'PRAGMA user_version = 21;',
        ], 'schema-21 retaining the inherited Vault object foundation')
    else:
        require(schema, [
            'pub(super) const CURRENT_SCHEMA_VERSION: i64 = 20;',
            'pub(super) const SCHEMA_18_TO_19',
            'CREATE TABLE vault_objects', 'CREATE TABLE vault_note_roots',
            'ciphertext  BLOB NOT NULL CHECK(length(ciphertext) > 0)',
            'key_envelope        BLOB NOT NULL CHECK(length(key_envelope) > 0)',
            'manifest_object_id', 'FOREIGN KEY(note_id, manifest_object_id)',
            'REFERENCES vault_objects(note_id, object_id)', 'PRAGMA user_version = 19;',
            'pub(super) const SCHEMA_19_TO_20', 'PRAGMA user_version = 20;',
        ], 'schema-20 retaining schema-19 Vault object foundation')
    require(init, ['SCHEMA_18_TO_19', '18 => {', 'transaction.execute_batch(SCHEMA_18_TO_19)?;'], '18→19 migration')


# I6 closure: the executable protected-row scrubber must consume the canonical
# SensitiveNoteData persistence plan instead of maintaining a parallel list.
sanitize_start = protection.find('fn sanitize_protected_note_rows')
if sanitize_start < 0:
    raise SystemExit('missing protected-row sanitizer')
sanitize_end = protection.find('\nfn ', sanitize_start + 10)
sanitize = protection[sanitize_start:sanitize_end if sanitize_end >= 0 else len(protection)]
require(sanitize, [
    'for &step in ProtectedPublicPersistenceSpec::SCRUB_STEPS',
    'ProtectedPublicPersistenceSpec::NOTE_SANITIZED_COLUMNS',
    'ProtectedPublicPersistenceSpec::ATTACHMENT_SANITIZED_COLUMNS',
    'protected_note_scrub_assignment',
    'protected_attachment_scrub_assignment',
], 'canonical default-private scrub plan')

# New object tables must not introduce plaintext JSON/title/body columns.
for table_name in ('vault_objects', 'vault_note_roots'):
    m = re.search(rf'CREATE TABLE {table_name}\s*\((.*?)\n\);', schema, re.S)
    if not m:
        raise SystemExit(f'Missing {table_name} definition')
    lowered = m.group(1).lower()
    for forbidden in ('title ', 'body ', 'plain_text', 'editor_state', 'snapshot_json', 'metadata_json'):
        if forbidden in lowered:
            raise SystemExit(f'{table_name} contains plaintext-shadow column marker: {forbidden}')

# Deny-by-default applies to new object-store metadata too. Object kind/version
# and mutation timestamps stay inside authenticated ciphertext/context instead
# of creating a plaintext activity side-channel in `vault_objects`.
objects_table = re.search(r'CREATE TABLE vault_objects\s*\((.*?)\n\);', schema, re.S).group(1).lower()
for forbidden in ('kind ', 'format_version', 'created_at', 'updated_at'):
    if forbidden in objects_table:
        raise SystemExit(f'vault_objects leaks unnecessary plaintext object metadata: {forbidden}')
roots_table = re.search(r'CREATE TABLE vault_note_roots\s*\((.*?)\n\);', schema, re.S).group(1).lower()
for forbidden in ('created_at', 'updated_at'):
    if forbidden in roots_table:
        raise SystemExit(f'vault_note_roots leaks unnecessary plaintext activity metadata: {forbidden}')

# Until 2b.4 the object store is foundation-only. In 2b.4 the same inherited
# foundation must become the single production representation, so usage is
# expected and validated by the dedicated 2b.4 cutover guard.
if build_id not in ('2b.4', '2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    for path in Path('src').rglob('*.rs'):
        if path.as_posix() == 'src/storage/vault_objects.rs':
            continue
        text = path.read_text(errors='ignore')
        for marker in ('insert_vault_object(', 'install_vault_note_root(', 'vault_note_root_record('):
            if marker in text:
                raise SystemExit(f'2b.1 partial production cutover detected in {path}: {marker}')

# Executable SQLite proof. 2b.5 deliberately supports only a fresh/current
# pre-release schema, so validate the canonical object tables directly instead
# of preserving obsolete migration code.
def raw_const(name):
    m = re.search(rf'const {name}: &str = r#"(.*?)"#;', schema, re.S)
    if not m:
        raise SystemExit(f'Could not extract {name}')
    return m.group(1)

def normalized_table_sql(conn, table):
    row = conn.execute("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?", (table,)).fetchone()
    if not row or not row[0]:
        raise SystemExit(f'Missing SQLite table {table}')
    return ' '.join(row[0].split())

base = raw_const('BASE_SCHEMA')
clean = sqlite3.connect(':memory:')
clean.executescript(base)
expected_current_version = 21 if build_id in ('2b.4', '2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43') else 20
if clean.execute('PRAGMA user_version').fetchone()[0] != expected_current_version:
    raise SystemExit(f'Clean schema did not reach current user_version {expected_current_version}')
if build_id in ('2b.5', '2b.6', '2b.7', '2b.8', '2b.9', '2b.10', '2b.11', '2b.12', '2b.13', '2b.14', '2b.15', '2b.16', '2b.17', '2b.18', '2b.19', '2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    for table in ('vault_objects', 'vault_note_roots', 'vault_blob_refs'):
        normalized_table_sql(clean, table)
else:
    migration = raw_const('SCHEMA_18_TO_19')
    migrated = sqlite3.connect(':memory:')
    migrated.executescript(base)
    migrated.executescript('DROP TABLE vault_note_roots; DROP TABLE vault_objects; PRAGMA user_version = 18;')
    migrated.executescript(migration)
    if migrated.execute('PRAGMA user_version').fetchone()[0] != 19:
        raise SystemExit('18→19 migration did not reach user_version 19')
    for table in ('vault_objects', 'vault_note_roots'):
        if normalized_table_sql(clean, table) != normalized_table_sql(migrated, table):
            raise SystemExit(f'18→19 migration no longer creates the canonical {table} definition')

def const_string_list(source, name):
    match = re.search(
        rf'pub\(crate\) const {name}:.*?=\s*&\[(.*?)\];',
        source,
        re.S,
    )
    if not match:
        raise SystemExit(f'Could not extract privacy classification constant {name}')
    return re.findall(r'"([^"]+)"', match.group(1))

expected_columns = {
    'notes': const_string_list(sensitive, 'NOTE_COLUMNS')
        + const_string_list(sensitive, 'NOTE_SANITIZED_COLUMNS'),
    'attachments': const_string_list(sensitive, 'ATTACHMENT_COLUMNS')
        + const_string_list(sensitive, 'ATTACHMENT_SANITIZED_COLUMNS'),
    'vault_objects': const_string_list(sensitive, 'VAULT_OBJECT_COLUMNS'),
    'vault_note_roots': const_string_list(sensitive, 'VAULT_ROOT_COLUMNS'),
}
for table, expected in expected_columns.items():
    actual = [row[1] for row in clean.execute(f'PRAGMA table_info({table})')]
    if set(actual) != set(expected) or len(actual) != len(expected):
        raise SystemExit(
            f'{table} has an unclassified/missing persistence column: '
            f'expected {expected}, got {actual}'
        )

safe_tables = set(const_string_list(sensitive, 'CURRENT_PROTECTED_NOTE_SAFE_TABLES'))
forbidden_tables = set(const_string_list(sensitive, 'CURRENT_PROTECTED_NOTE_FORBIDDEN_TABLES'))
special_surfaces = const_string_list(sensitive, 'CURRENT_PROTECTED_NOTE_SPECIAL_SURFACES')
special_tables = {surface.split(':', 1)[0] for surface in special_surfaces}
if safe_tables & forbidden_tables:
    raise SystemExit('Protected-note persistence table classification overlaps safe/forbidden sets')

all_tables = [
    row[0]
    for row in clean.execute(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'"
    )
]
note_owned_tables = set()
foreign_parents = {}
for table in all_tables:
    columns = [row[1] for row in clean.execute(f'PRAGMA table_info({table})')]
    parents = {row[2] for row in clean.execute(f'PRAGMA foreign_key_list({table})')}
    foreign_parents[table] = parents
    if (
        any(column == 'note_id' or column.endswith('_note_id') for column in columns)
        or 'notes' in parents
    ):
        note_owned_tables.add(table)

# Follow child→parent foreign-key ownership transitively: e.g. PDF annotations
# belong to attachments, and revision attachment refs belong to revisions.
changed = True
while changed:
    changed = False
    for table, parents in foreign_parents.items():
        if table not in note_owned_tables and parents & note_owned_tables:
            note_owned_tables.add(table)
            changed = True

unclassified = note_owned_tables - safe_tables - forbidden_tables - special_tables
if unclassified:
    raise SystemExit(
        'New note-owned SQLite surface lacks an explicit protected-data classification: '
        + ', '.join(sorted(unclassified))
    )

for required_special in (
    'notes_fts', 'note_links', 'revision_attachment_refs', 'pdf_annotation_pages',
    'sync_queue', 'tags', 'sources', 'settings',
):
    if required_special not in special_tables:
        raise SystemExit(f'Missing indirect/directional privacy classification: {required_special}')

# The root→manifest relationship is an executable SQLite invariant, not only a
# source marker. A root may never point outside its note's object namespace.
fk = sqlite3.connect(':memory:')
fk.execute('PRAGMA foreign_keys = ON')
fk.executescript(base)
fk.execute(
    "INSERT INTO notebooks(id,name,parent_id,sort_order,created_at,updated_at) "
    "VALUES ('privacy-test-notebook','Privacy Test',NULL,0,1,1)"
)
fk.execute(
    "INSERT INTO notes(id,fts_rowid,notebook_id,title,body,editor_state,plain_text,"
    "pinned,archived,trashed,purge_after,created_at,updated_at,title_style) "
    "VALUES ('privacy-test-note',999999,'privacy-test-notebook','','','{}','',0,0,0,NULL,1,1,'{}')"
)
missing_object = '00' * 16
try:
    fk.execute(
        "INSERT INTO vault_note_roots(note_id,format_version,key_envelope,manifest_object_id) "
        "VALUES (?,?,?,?)",
        ('privacy-test-note', 2, b'envelope', missing_object),
    )
except sqlite3.IntegrityError:
    pass
else:
    raise SystemExit('vault_note_roots accepted a missing manifest object')
fk.execute(
    "INSERT INTO vault_objects(note_id,object_id,ciphertext) VALUES (?,?,?)",
    ('privacy-test-note', missing_object, b'ciphertext'),
)
fk.execute(
    "INSERT INTO vault_note_roots(note_id,format_version,key_envelope,manifest_object_id) "
    "VALUES (?,?,?,?)",
    ('privacy-test-note', 2, b'envelope', missing_object),
)

print('Vault 2b.1 foundation static contract passed.')
PY

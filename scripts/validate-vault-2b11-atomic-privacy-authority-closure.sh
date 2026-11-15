#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

for required in \
  QA_VAULT_2B11_CLOSURE.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.11 closure file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path
import re


def read(path):
    return Path(path).read_text(errors='ignore')

def require(text, markers, label):
    for marker in markers:
        if marker not in text:
            raise SystemExit(f"{label} missing: {marker}")

def forbid(text, markers, label):
    for marker in markers:
        if marker in text:
            raise SystemExit(f"{label} contains forbidden legacy pattern: {marker}")

def segment(text, start, end=None):
    i=text.find(start)
    if i<0: raise SystemExit(f"missing segment start: {start}")
    j=len(text) if end is None else text.find(end, i+len(start))
    if j<0: j=len(text)
    return text[i:j]

def ordered(text, markers, label):
    p=0
    for m in markers:
        q=text.find(m,p)
        if q<0: raise SystemExit(f"{label} missing/out of order: {m}")
        p=q+len(m)

coord=read('src/security/privacy_coordinator.rs')
derivative=read('src/security/derivative.rs')
storage=read('src/storage/mod.rs')
transport=read('src/storage/transport.rs')
caps=read('src/storage/capabilities.rs')
ingest=read('src/storage/attachment_ingest.rs')
attachments=read('src/storage/attachments.rs')
app_attach=read('src/application/attachments.rs')
media_server=read('src/media_server.rs')
media_compat=read('src/media_compat.rs')
protocol=read('src/attachment_protocol.rs')
sensitive=read('src/security/sensitive_data.rs')
vault=read('src/storage/vault_objects.rs')
protection=read('src/storage/protection.rs')
vaultlib=read('crates/rustnote-vault/src/lib.rs')
all_src='\n'.join(p.read_text(errors='ignore') for p in Path('src').rglob('*.rs'))

# 1. One-note plaintext+Vault authority must have one admission linearization point.
require(coord,[
    'pub(crate) struct PlaintextOperationPermit',
    'access: Option<VaultNoteAccess>',
    '_derivative: PlaintextDerivativePermit',
    'pub(crate) fn acquire_plaintext_operation(',
    'pub(crate) fn begin_plaintext_operation_mutation(',
], 'compound plaintext operation authority')
compound=segment(coord,'pub(crate) fn acquire_plaintext_operation(', 'pub(crate) fn begin_plaintext_operation_mutation(')
ordered(compound,[
    'let state = self.wait_for_note_admission_clear(note_id)?;',
    'let derivative = self.inner.plaintext_derivatives.acquire(note_id)?;',
    'let access = self.inner.vault_runtime.acquire_note_access(note_id)?;',
    'drop(state);',
    'Ok(PlaintextOperationPermit',
], 'compound authorization acquisition')
mutation=segment(coord,'pub(crate) fn begin_plaintext_operation_mutation(', 'pub(crate) fn begin_plaintext_derivative_cleanup(')
ordered(mutation,[
    'let operation_guard = self.lock_storage()?;',
    '_authorization: permit.clone()',
], 'compound mutation lifetime')
forbid(
    mutation,
    ['wait_for_note_admission_clear', 'read_permit_is_publishable'],
    'compound mutation must trust its pre-admission authorization without re-entering privacy state',
)

# Standalone derivative acquisition is test-only; production callers cannot reassemble old choreography.
standalone=coord.find('pub(crate) fn acquire_plaintext_derivative(')
if standalone < 0 or '#[cfg(test)]' not in coord[max(0,standalone-120):standalone]:
    raise SystemExit('standalone plaintext derivative acquisition is not test-only')
forbid(storage,['acquire_plaintext_derivative_for_note'], 'production Storage privacy API')

# 2. Multi-note protected mutations capture the complete reader set atomically.
multi=segment(coord,'pub(crate) fn begin_protected_notes_mutation(', 'pub(crate) fn acquire_note_access(')
ordered(multi,[
    'let state = self.wait_for_notes_admission_clear(&ids)?;',
    'for note_id in &ids',
    'self.inner.vault_runtime.acquire_note_access(note_id)?',
    'let operation_guard = self.lock_storage()?;',
    'drop(state);',
], 'set-wise protected mutation')
forbid(multi,['wait_for_note_admission_clear(note_id)'], 'set-wise protected mutation')
require(coord,[
    'compound_plaintext_operation_finishes_after_lock_admission_without_circular_wait',
    'ordinary_compound_plaintext_operation_finishes_before_admitted_protect',
    'multi_protected_mutation_acquires_entire_reader_set_before_transition_can_admit',
], 'deterministic deadlock regression tests')

# 3. No production reverse lock edge: storage operation lock -> privacy admission/reader acquisition.
# Strip test modules to avoid deliberate adversarial lock manipulation.
prod_files=[]
for p in Path('src').rglob('*.rs'):
    t=p.read_text(errors='ignore')
    t=t.split('#[cfg(test)]\nmod tests {',1)[0]
    prod_files.append((str(p),t))
auth_markers=(
    'acquire_note_access(', 'acquire_plaintext_operation_for_note(',
    'begin_protected_note_mutation(', 'begin_protected_notes_mutation(',
    'begin_note_transition(', 'begin_note_transitions(', 'begin_library_transition(',
)
for path,t in prod_files:
    # Conservative function-body scanner; braces are sufficient for these Rust functions.
    for m in re.finditer(r'fn\s+([A-Za-z0-9_]+)\s*\([^;]*?\)\s*(?:->[^\{]+)?\{', t, re.S):
        start=m.start(); brace=t.find('{',m.start()); depth=0; end=None
        for i in range(brace,len(t)):
            if t[i]=='{': depth+=1
            elif t[i]=='}':
                depth-=1
                if depth==0: end=i+1; break
        if end is None: continue
        body=t[brace:end]
        lock_positions=[body.find('lock_storage()'), body.find('operation_lock.lock()')]
        lock_positions=[x for x in lock_positions if x>=0]
        if not lock_positions: continue
        first=min(lock_positions)
        later=[mark for mark in auth_markers if body.find(mark, first+1)>=0]
        if later:
            raise SystemExit(f'reverse storage->privacy lock edge in {path}:{m.group(1)}: {later}')

# 4. Full sensitive OS/transport lifetime owns the compound permit itself.
require(caps,[
    'struct AttachmentFileCommandResource',
    'struct PdfExportCommandResource',
    'struct NoteExportResource',
], 'native sensitive resources')
for name in ['struct AttachmentFileCommandResource','struct PdfExportCommandResource','struct NoteExportResource']:
    s=segment(caps,name,'impl ')
    if '_authorization: crate::security::PlaintextOperationPermit' not in s:
        raise SystemExit(f'{name} does not retain compound authorization')
require(transport,[
    'pub struct AttachmentTransportResource',
    'authorization: crate::security::PlaintextOperationPermit',
    'acquire_plaintext_operation_for_note(note_id)',
], 'transport full-lifetime authorization')
require(media_server,['struct LeasedMediaFile','_authorization: crate::security::PlaintextOperationPermit'], 'media descriptor authorization')
require(protocol,['struct PdfReadHandle','_transport_guard: AttachmentTransportResource'], 'PDF descriptor authorization')
require(media_compat,['AttachmentTransportResource'], 'media compatibility job authorization')

# Old caller-side two-step acquisition API must be absent from production source.
prod_src='\n'.join(t for _,t in prod_files)
forbid(prod_src,[
    'acquire_plaintext_derivative_for_note',
    '_plaintext_permit: crate::security::PlaintextDerivativePermit',
], 'production source')

# 5. Direct retained-revision resolution uses an encrypted compact index.
require(sensitive,[
    'pub(crate) const REVISION_INDEX_FORMAT_VERSION: u16 = 1;',
    'pub(crate) struct RevisionIndexEntry',
    'pub(crate) struct RevisionIndex',
    'Newest -> oldest retained revisions.',
], 'revision index specification')
require(vaultlib,['RevisionIndex = 10'], 'revision index cryptographic domain')
require(vault,[
    'RevisionIndex,',
    'pub(crate) revision_index: VaultObjectId',
    'VaultObjectKind::RevisionIndex',
    'pub(crate) fn read_vault_revision_by_handle(',
    'pub(crate) fn read_vault_revision_index(',
    'validate_revision_index_for_manifest',
], 'revision index object store')
lookup=segment(vault,'pub(crate) fn read_vault_revision_by_handle(', 'pub(crate) fn read_vault_revision_index(')
require(lookup,[
    '&manifest.revision_index',
    '.find(|entry| entry.object_id.as_str() == revision_handle)',
    '&entry.object_id',
    'revision.revision_no != entry.revision_no',
], 'direct revision lookup')
forbid(lookup,['read_vault_revision_chain'], 'direct revision lookup')
require(vault,[
    'a valid same-note orphan revision must not bypass retained-index membership',
], 'retained revision membership regression')
summary=segment(protection,'pub(super) fn protected_revision_summaries_if_unlocked(', 'pub(super) fn protected_revision_snapshot_if_unlocked(')
require(summary,['read_vault_revision_index(', 'id: entry.object_id.as_str().to_string()'], 'protected revision listing')
forbid(summary,['read_vault_revision_chain'], 'protected revision listing')
preview=segment(protection,'pub(super) fn protected_revision_snapshot_resource_if_unlocked(', 'pub(super) fn restore_protected_revision_if_unlocked(')
require(preview,['read_vault_revision_by_handle('], 'protected revision preview')
forbid(preview,['read_vault_revision_chain'], 'protected revision preview')
restore=segment(protection,'pub(super) fn restore_protected_revision_if_unlocked(', 'pub(super) fn delete_protected_revision_history')
require(restore,['read_vault_revision_by_handle('], 'protected revision restore')
# Keep normal autosave history-independent: encrypted index replacement happens only when appending a revision.
graph_update=segment(protection,'let mut revision_index_id = old_manifest.revision_index.clone();','let manifest = VaultManifest::new(')
if graph_update.find('if let Some(revision) = appended_revision') < 0:
    raise SystemExit('vault graph update lacks revision-append locality boundary')
read_idx=graph_update.find('read_vault_revision_index(')
append_if=graph_update.find('if let Some(revision) = appended_revision')
if read_idx < append_if:
    raise SystemExit('ordinary protected autosave opens revision index outside revision append path')

# Root/manifest format was intentionally advanced for the new object graph; no dual format remains.
require(sensitive,[
    'pub(crate) const VAULT_MANIFEST_FORMAT_VERSION: u16 = 2;',
    'pub(crate) const VAULT_NOTE_FORMAT_VERSION: u16 = 3;',
], '2b.11 Vault format versions')
forbid(prod_src,['ProtectedNotePayload'], 'production protected persistence')

# 6. I1 hardening: nominal durable-owner constructors remain a deliberately
# tiny internal boundary. Rust module visibility cannot make a crate-internal
# reservation token cryptographically/unforgeably privileged, so the stronger
# lean enforcement is to freeze every production constructor to the one ledger
# module that proves/persists that authority before construction. Existing 2b.8
# and E0-E3 guards verify the ordering inside those modules.
durable_constructor_owners = {
    'durable_managed': {'src/storage/plaintext_staging.rs'},
    'durable_managed_existing': {'src/storage/plaintext_staging.rs'},
    'durable_vault_operation_reserved': {'src/storage/protection.rs'},
    'durable_vault_operation_existing': {'src/storage/protection.rs'},
    'durable_attachment_operation_existing': {'src/storage/attachments.rs'},
    'durable_attachment_ingest_existing': {'src/storage/attachment_ingest.rs'},
    'durable_external_output_reserved': {'src/native/output.rs'},
    'durable_external_output_existing': {'src/native/output.rs'},
}
for constructor, allowed_paths in durable_constructor_owners.items():
    needle = f'OwnedPlaintext::{constructor}('
    qualified = f'crate::security::OwnedPlaintext::{constructor}('
    seen = set()
    for path, text in prod_files:
        if needle in text or qualified in text:
            seen.add(path)
    if seen != allowed_paths:
        raise SystemExit(
            f'durable ownership constructor {constructor} escaped canonical ledger authority: '
            f'expected {sorted(allowed_paths)}, found {sorted(seen)}'
        )

print('Vault 2b.11 atomic privacy-authority/revision-index closure guard passed.')
PY

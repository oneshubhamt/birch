#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }

for required in NATIVE_COMPILE_2B15_PLAN.md NATIVE_COMPILE_2B15_IMPLEMENTATION.md QA_NATIVE_COMPILE_2B15.md; do
  [[ -f "$required" ]] || { echo "Missing 2b.15 closure file: $required" >&2; exit 1; }
done

python3 - <<'PY'
from pathlib import Path

build_id = Path('build-id.txt').read_text().strip()
transport = Path('src/storage/transport.rs').read_text()
storage_mod = Path('src/storage/mod.rs').read_text()
protocol = Path('src/attachment_protocol.rs').read_text()
plaintext = Path('src/security/plaintext.rs').read_text()
runtime = Path('src/security/runtime.rs').read_text()
sensitive = Path('src/security/sensitive_data.rs').read_text()
errors = Path('src/application/errors.rs').read_text()
media = Path('src/media_compat.rs').read_text()
attachments = Path('src/storage/attachments.rs').read_text()
capabilities = Path('src/storage/capabilities.rs').read_text()
protection = Path('src/storage/protection.rs').read_text()
coordinator = Path('src/security/privacy_coordinator.rs').read_text()

def require(text, marker, label):
    if marker not in text:
        raise SystemExit(f'{label} missing: {marker}')

def reject(text, marker, label):
    if marker in text:
        raise SystemExit(f'{label} retained forbidden/stale marker: {marker}')

# Security types stay private.
require(coordinator, 'pub(crate) struct PlaintextOperationPermit', 'private compound permit')
require(plaintext, 'pub(crate) struct OwnedPlaintext', 'private plaintext owner')

# Public binary-facing transport API is opaque and re-exported from public storage.
for marker in (
    'pub struct AttachmentPlaintextIngressGuard',
    'pub struct AttachmentEphemeralPlaintextGuard',
    'pub fn prepare_ephemeral_plaintext_directory(',
    'pub fn plaintext_attachment_ingress_permit(',
):
    require(transport, marker, 'opaque transport façade')
for marker in ('AttachmentEphemeralPlaintextGuard', 'AttachmentPlaintextIngressGuard'):
    require(storage_mod, marker, 'opaque transport re-export')
reject(transport, 'pub fn plaintext_operation_permit(', 'unused permit exposure')
reject(transport, 'pub fn attachment_resource_authorized(', 'unused re-authorization exposure')

# Ownership must be established before directory creation and binary must not bypass it.
segment = transport[transport.index('pub fn prepare_ephemeral_plaintext_directory('):]
if segment.index('OwnedPlaintext::reserve_vault_session') > segment.index('owned.create_private_directory()?'):
    raise SystemExit('preview directory is materialized before Vault-session ownership reservation')
require(plaintext, 'pub(crate) fn create_private_directory(&mut self)', 'owned directory authority')
require(plaintext, 'create_private_directory_in_existing_directory(&self.path)?', 'owned private directory materialization')
for marker in ('native::private_temp', 'security::OwnedPlaintext', 'PlaintextOperationPermit', 'track_ephemeral_plaintext'):
    reject(protocol, marker, 'binary security-internal bypass')
require(protocol, '.prepare_ephemeral_plaintext_directory(preview_root.clone())', 'protected preview opaque staging')
require(protocol, 'preview_guard.remove()', 'protected preview opaque cleanup')

# Exact warning cleanup from the native 2b.14 compile layer.
reject(errors, 'AttachmentNotFound', 'dead ExportError variant')
reject(media, 'pub(crate) fn discard_cache(', 'dead media cache wrapper')
reject(attachments, 'pub(super) fn discard_image_rendition_cache(&self', 'dead rendition wrapper')
reject(capabilities, 'pub(crate) fn add_attachment_file_as(\n', 'dead attachment façade')
reject(capabilities, 'pub(crate) fn add_derived_image_attachment_file(\n', 'dead derived-image façade')
require(runtime, '#[cfg(test)]\n    pub(crate) fn remove_note_key(', 'test-only runtime remove helper')
if build_id in ('2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33'):
    require(runtime, '#[cfg(any(test, feature = "internal-test-support"))]\n    pub(crate) fn contains_note_key(', 'internal-test-support runtime contains helper')
else:
    require(runtime, '#[cfg(test)]\n    pub(crate) fn contains_note_key(', 'test-only runtime contains helper')
require(sensitive, '#[cfg(test)]\n    pub(crate) fn validate(&self)', 'test-only sensitive validator')
require(plaintext, 'VaultOperation { _ownership: DurableVaultOperationOwnership }', 'durable Vault ownership marker retention')
require(plaintext, 'AttachmentOperation { _ownership: DurableAttachmentOperationOwnership }', 'durable attachment ownership marker retention')
require(plaintext, 'AttachmentIngest { _ownership: DurableAttachmentIngestOwnership }', 'durable ingest ownership marker retention')
require(plaintext, 'DurableExternalOutput { _ownership: DurableExternalOutputOwnership }', 'durable external ownership marker retention')

# The reduced protected resource remains a lifetime carrier, not duplicated metadata.
resource = protection[protection.index('pub(super) struct ProtectedAttachmentResource'):]
resource = resource[:resource.index('\n}\n') + 3]
for marker in ('pub path: PathBuf', 'pub permit: VaultReadPermit'):
    require(resource, marker, 'protected resource lifetime field')
for marker in ('pub note_id: NoteId', 'pub attachment: Attachment'):
    reject(resource, marker, 'unread protected resource field')

print('Rustnote 2b.15 transport visibility/warning closure static guard passed.')
PY

bash scripts/validate-vault-2b14-editor-native-capture-closure.sh
bash scripts/validate-vault-2b13-native-compile-closure.sh
bash scripts/validate-vault-2b12-aegis-privacy-disclosure-closure.sh
bash scripts/validate-vault-2b11-atomic-privacy-authority-closure.sh
bash scripts/validate-security-e0-e3-ownership-2a16.sh
bash scripts/validate-security-e0-e3-residue-2a17.sh
bash scripts/validate-security-e0-e3-ui-lifetime-2a18.sh
bash scripts/validate-security-e0-e3-adversarial-closure-2a19.sh

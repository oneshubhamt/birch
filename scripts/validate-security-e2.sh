#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2a.18" || "$BUILD_ID" == "2a.19" || "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }
[[ -f src/storage/protection.rs ]] || { echo "Missing E2 protection persistence boundary." >&2; exit 1; }
[[ -f src/application/vault.rs ]] || { echo "Missing E2 application vault workflow." >&2; exit 1; }
[[ -f src/security/runtime.rs ]] || { echo "Missing E2 vault runtime." >&2; exit 1; }

python3 - <<'PY'
from pathlib import Path

schema = Path("src/storage/schema.rs").read_text()
protection = Path("src/storage/protection.rs").read_text()
notes = Path("src/storage/notes.rs").read_text()
attachments = Path("src/storage/attachments.rs").read_text()
backups = Path("src/storage/backups.rs").read_text()
portability = Path("src/storage/portability.rs").read_text()
bundle = Path("src/storage/portable_bundle.rs").read_text()
transport = Path("src/storage/transport.rs").read_text()
sources = Path("src/storage/sources.rs").read_text()
media = Path("src/media_compat.rs").read_text()
ui = "\n".join(path.read_text(errors="ignore") for path in Path("src/ui").rglob("*.rs"))
vault = Path("crates/rustnote-vault/src/lib.rs").read_text()
build_id = Path("build-id.txt").read_text().strip()

def is_2b_at_least(minor: int) -> bool:
    parts = build_id.split(".")
    if len(parts) < 2 or parts[0] != "2b":
        return False
    try:
        return int(parts[1]) >= minor
    except ValueError:
        return False

if is_2b_at_least(4):
    schema_markers = [
        "pub(super) const CURRENT_SCHEMA_VERSION: i64 = 21;",
        "CREATE TABLE vault_objects", "CREATE TABLE vault_note_roots",
        "CREATE TABLE vault_blob_refs", "PRAGMA user_version = 21;",
    ]
    if build_id == "2b.4":
        schema_markers.append("DROP TABLE protected_notes;")
    else:
        if "protected_notes" in schema or "protected_note_blobs" in schema or "const SCHEMA_" in schema:
            raise SystemExit("E2/2b.5 retained obsolete monolithic/schema migration compatibility")
    for marker in schema_markers:
        if marker not in schema:
            raise SystemExit(f"E2/2b.4 schema marker missing: {marker}")
    for marker in (
        "protect_note_with_password", "unlock_protected_note", "lock_protected_note",
        "remove_note_protection_with_password", "Argon2idParams::V1_INTERACTIVE",
        "VaultDomain::Attachment", "seal_protected_blob", "open_protected_blob",
        "vault_note_roots", "vault_blob_refs", "LOCKED_ATTACHMENT_NAME",
        "DELETE FROM notes_fts", "DELETE FROM note_tags", "DELETE FROM note_revisions",
        "DELETE FROM note_links", "DELETE FROM citations", "DELETE FROM note_sources",
        "DELETE FROM pdf_annotation_pages", "embedded = 0", "source_attachment_id = NULL",
        "protected_revision_summaries_if_unlocked", "protected_note_sources_if_unlocked",
        "protected_pdf_annotation_rows_if_unlocked",
        "protected_note_encrypts_attachment_blob_and_restores_it_losslessly",
        "protection_commits_attachment_placeholder_under_schema_constraint",
        "protect_note_blob_from_path", "cleanup_uncommitted_protected_blobs",
        "read_vault_current_graph", "commit_vault_graph_update",
    ):
        if marker not in protection:
            raise SystemExit(f"E2/2b.4 protection marker missing: {marker}")
    if "persist_protected_note_content" not in notes:
        raise SystemExit("E2/2b.4 protected-note persistence authority is missing")
    if "vault_blob_refs" not in attachments or "vault_blob_refs" not in backups:
        raise SystemExit("E2/2b.4 protected ciphertext is not covered by attachment/backup accounting")
    if "replace_protected_pdf_annotation_page_if_unlocked" not in transport:
        raise SystemExit("E2/2b.4 protected PDF metadata mutation authority is missing")
    if "mutate_protected_metadata_if_unlocked" not in sources:
        raise SystemExit("E2/2b.4 protected source mutation authority is missing")
    if is_2b_at_least(10):
        for marker in (
            "fn sync_payload_allowed",
            "vault_note_roots",
            "clear_sync_payloads_for_entity",
            "protected_note_and_attachment_payloads_never_enter_sync_queue",
        ):
            if marker not in notes:
                raise SystemExit(f"E2/2b.10 protected sync-queue boundary missing: {marker}")
        upsert = notes[notes.index("pub(super) fn enqueue_sync_upsert"):notes.index("pub(super) fn enqueue_sync_delete")]
        if upsert.index("sync_payload_allowed") > upsert.index("serde_json::to_string(payload)"):
            raise SystemExit("E2/2b.10 protected sync classification must precede payload serialization")
else:
    for marker in (
        "pub(super) const CURRENT_SCHEMA_VERSION: i64 = 20;",
        "CREATE TABLE protected_notes",
        "CREATE TABLE protected_note_blobs",
        "PRAGMA user_version = 18;",
    ):
        if marker not in schema:
            raise SystemExit(f"E2 schema marker missing: {marker}")
    for marker in (
        "protect_note_with_password", "unlock_protected_note", "lock_protected_note",
        "remove_note_protection_with_password", "Argon2idParams::V1_INTERACTIVE",
        "VaultDomain::Note", "VaultDomain::Attachment", "seal_protected_blob",
        "open_protected_blob", "protected_note_blobs", "LOCKED_ATTACHMENT_NAME",
        "DELETE FROM notes_fts", "DELETE FROM note_tags", "DELETE FROM note_revisions",
        "DELETE FROM note_links", "DELETE FROM citations", "DELETE FROM note_sources",
        "DELETE FROM pdf_annotation_pages", "embedded = 0", "unreferenced_at = ?4", "source_attachment_id = NULL",
        "protected_revision_summaries_if_unlocked", "protected_note_sources_if_unlocked",
        "protected_pdf_annotation_rows_if_unlocked",
        "protected_note_encrypts_attachment_blob_and_restores_it_losslessly",
        "protection_commits_attachment_placeholder_under_schema_constraint",
        "protect_single_note_blob", "cleanup_uncommitted_protected_blobs", "plaintext.fill(0)",
    ):
        if marker not in protection:
            raise SystemExit(f"E2 protection marker missing: {marker}")
    if "encrypted notes are read-only in E2" not in notes:
        raise SystemExit("E2 protected-note persistence guard is missing")
    if "protected_note_blobs" not in attachments or "protected_note_blobs" not in backups:
        raise SystemExit("E2 protected ciphertext is not covered by attachment/backup accounting")
    if "ensure_pdf_annotations_unprotected" not in transport:
        raise SystemExit("E2 protected PDF mutation guard is missing")
    if "sources are read-only while the note is encrypted" not in sources:
        raise SystemExit("E2 protected source mutation guard is missing")

if "ensure_portability_scope_unprotected" not in portability or "ensure_portability_scope_unprotected" not in bundle:
    raise SystemExit("E2 portable export containment is missing")
if "resource.protected" not in media:
    raise SystemExit("E2 protected media compatibility-cache guard is missing")
ui_markers = [
    "Encrypt note", "Unlock note", "Remove encryption", "Encrypted note",
    "EntityAction::EncryptNote", "EntityAction::RemoveNoteEncryption",
]
if is_2b_at_least(31):
    ui_markers.extend(("Locked-note label", "Visible without the password"))
else:
    ui_markers.append("Locked note")
for marker in ui_markers:
    if marker not in ui:
        raise SystemExit(f"E2 UI marker missing: {marker}")

allowed_vault_users = {
    "src/security/mod.rs",
    "src/security/backup_key_authority.rs",
    "src/platform/secure_key_store.rs",
}
for path in Path("src").rglob("*.rs"):
    text = path.read_text(errors="ignore")
    if path.as_posix() not in allowed_vault_users and "rustnote_vault" in text:
        raise SystemExit(f"E2 bypasses audited Rustnote security boundary: {path}")
    for marker in ("use aegis::", "use argon2::", "use chacha20poly1305::", "use aes_gcm::"):
        if marker in text:
            raise SystemExit(f"Low-level crypto primitive leaked into app code: {path}")

for forbidden in ("sqlcipher", "SQLCipher", "web_sys::SubtleCrypto", "SubtleCrypto"):
    if forbidden in "\n".join(path.read_text(errors="ignore") for path in Path("src").rglob("*.rs")):
        raise SystemExit(f"E2 introduced forbidden encryption architecture marker: {forbidden}")

if "const TAG_BYTES: usize = 32;" not in vault or "Aegis256::<TAG_BYTES>" not in vault:
    raise SystemExit("E2 requires the full 256-bit AEGIS-256 authentication tag")

PY

if [[ "${RUSTNOTE_E2_STATIC_ONLY:-0}" == "1" ]]; then
  echo "Security E2 static contract passed (build $BUILD_ID; Rust compilation skipped by RUSTNOTE_E2_STATIC_ONLY=1)."
  exit 0
fi

cargo fmt --all -- --check
cargo metadata --locked --format-version 1 >/dev/null
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p rustnote-vault --locked
cargo test --workspace --all-features --locked
echo "Security E2 contract passed (build $BUILD_ID)."

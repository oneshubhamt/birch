#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2a.18" || "$BUILD_ID" == "2a.19" || "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || {
  echo "Unexpected Rustnote source build: $BUILD_ID" >&2
  exit 1
}

[[ -f SECURITY_ENCRYPTION_E1_CONTRACT.md ]] || { echo "Missing E1 security contract." >&2; exit 1; }
[[ -f crates/rustnote-vault/Cargo.toml ]] || { echo "Missing rustnote-vault crate." >&2; exit 1; }
[[ -f crates/rustnote-vault/src/lib.rs ]] || { echo "Missing rustnote-vault implementation." >&2; exit 1; }

python3 - <<'PY'
from pathlib import Path
import re

root = Path("Cargo.toml").read_text()
vault = Path("crates/rustnote-vault/Cargo.toml").read_text()
source = Path("crates/rustnote-vault/src/lib.rs").read_text()
app_security = Path("src/security/mod.rs").read_text()
lock = Path("Cargo.lock").read_text()

for marker in (
    'members = ["crates/rustnote-media-compat", "crates/rustnote-vault"]',
    'rustnote-vault = { path = "crates/rustnote-vault" }',
):
    if marker not in root:
        raise SystemExit(f"E1 root manifest missing: {marker}")

for pin in (
    'aegis = "=0.9.15"',
    'argon2 = { version = "=0.5.3", default-features = false, features = ["alloc", "zeroize"] }',
    'getrandom = "=0.3.4"',
    'zeroize = { version = "=1.9.0", features = ["alloc"] }',
):
    if pin not in vault:
        raise SystemExit(f"E1 vault dependency is not exactly pinned: {pin}")

for marker in (
    'const OBJECT_MAGIC: &[u8; 8] = b"RNVOBJ01";',
    'const KEY_MAGIC: &[u8; 8] = b"RNVKEY01";',
    'const BLOB_MAGIC: &[u8; 8] = b"RNVBLB01";',
    'const FORMAT_VERSION: u16 = 1;',
    'const ALGORITHM_AEGIS_256: u8 = 1;',
    'Aegis256::<TAG_BYTES>',
    'Algorithm::Argon2id',
    'Zeroizing<[u8; KEY_BYTES]>',
    'seal_protected_blob',
    'open_protected_blob',
):
    if marker not in source:
        raise SystemExit(f"E1 vault implementation missing marker: {marker}")

if "const TAG_BYTES: usize = 32;" not in source or "Aegis256::<TAG_BYTES>" not in source:
    raise SystemExit("E1 AEGIS-256 format must use the full 256-bit authentication tag")

window = source[source.index('pub struct VaultKey')-100:source.index('pub struct VaultKey')+50]
if '#[derive(Clone' in window:
    raise SystemExit("VaultKey must remain non-Clone")

for marker in ('VaultService', 'VaultKey', 'Argon2idParams', 'VaultContext'):
    if marker not in app_security:
        raise SystemExit(f"Application security boundary does not expose {marker}")

# App/storage code may use only Rustnote's security facade, never the vault
# crate or external primitive crates directly.
allowed_vault_users = {
    "src/security/mod.rs",
    "src/security/backup_key_authority.rs",
    "src/platform/secure_key_store.rs",
}
for path in Path("src").rglob("*.rs"):
    text = path.read_text(errors="ignore")
    if path.as_posix() not in allowed_vault_users and "rustnote_vault" in text:
        raise SystemExit(f"rustnote-vault bypasses audited security boundary: {path}")
    for marker in ("use aegis::", "use argon2::", "use chacha20poly1305::", "use aes_gcm::"):
        if marker in text:
            raise SystemExit(f"Low-level crypto primitive leaked into application code: {path}")

required_lock = {
    "aegis": "0.9.15", "argon2": "0.5.3", "base64ct": "1.8.3",
    "blake2": "0.10.6", "softaes": "0.1.5", "password-hash": "0.5.0",
    "rustnote-vault": "0.1.0",
}
packages = {}
for block in lock.split("[[package]]")[1:]:
    name = re.search(r'^name = "([^"]+)"$', block, re.MULTILINE)
    version = re.search(r'^version = "([^"]+)"$', block, re.MULTILINE)
    if name and version:
        packages.setdefault(name.group(1), set()).add(version.group(1))
for name, version in required_lock.items():
    if version not in packages.get(name, set()):
        raise SystemExit(f"Cargo.lock missing E1 package {name} {version}")

contract = Path("SECURITY_ENCRYPTION_E1_CONTRACT.md").read_text()
for marker in (
    "AEGIS-256 policy", "Password / Argon2id policy", "Large protected blob contract",
    "Memory/key handling contract", "Explicit E1 non-goals", "Blockchain-inspired integrity",
    "E2 entry criteria",
):
    if marker not in contract:
        raise SystemExit(f"E1 security contract missing section: {marker}")
PY

if [[ "${RUSTNOTE_E1_STATIC_ONLY:-0}" == "1" ]]; then
  echo "Security E1 static contract passed (build $BUILD_ID; Cargo checks skipped by RUSTNOTE_E1_STATIC_ONLY=1)."
  exit 0
fi

cargo metadata --locked --format-version 1 >/dev/null
cargo test -p rustnote-vault --locked
echo "Security E1 contract passed (build $BUILD_ID)."

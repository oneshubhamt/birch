#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2a.18" || "$BUILD_ID" == "2a.19" || "$BUILD_ID" == "2b.1" || "$BUILD_ID" == "2b.2" || "$BUILD_ID" == "2b.3" || "$BUILD_ID" == "2b.4" || "$BUILD_ID" == "2b.5" || "$BUILD_ID" == "2b.6" || "$BUILD_ID" == "2b.7" || "$BUILD_ID" == "2b.8" || "$BUILD_ID" == "2b.9" || "$BUILD_ID" == "2b.10" || "$BUILD_ID" == "2b.11" || "$BUILD_ID" == "2b.12" || "$BUILD_ID" == "2b.13" || "$BUILD_ID" == "2b.14" || "$BUILD_ID" == "2b.15" || "$BUILD_ID" == "2b.16" || "$BUILD_ID" == "2b.17" || "$BUILD_ID" == "2b.18" || "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || {
  echo "Unexpected Rustnote source build: $BUILD_ID" >&2
  exit 1
}

[[ -f SECURITY_ENCRYPTION_E0_CONTRACT.md ]] || {
  echo "Missing E0 security contract." >&2
  exit 1
}
[[ -f tools/e0-crypto-bench/Cargo.toml ]] || {
  echo "Missing isolated E0 crypto benchmark." >&2
  exit 1
}
python3 - <<'PY'
from pathlib import Path

root_manifest = Path("Cargo.toml").read_text()
# Production primitive dependencies remain isolated behind rustnote-vault.
for crate in ("aegis", "aes-gcm", "argon2", "chacha20poly1305"):
    if f"{crate} =" in root_manifest:
        raise SystemExit(f"External crypto primitive {crate!r} leaked into root Cargo.toml")

tool_manifest = Path("tools/e0-crypto-bench/Cargo.toml").read_text()
required_pins = (
    'aegis = "=0.9.15"',
    'aes-gcm = "=0.11.1"',
    'argon2 = { version = "=0.6.0", features = ["parallel", "zeroize"] }',
    'chacha20poly1305 = { version = "=0.11.0", features = ["zeroize"] }',
)
for pin in required_pins:
    if pin not in tool_manifest:
        raise SystemExit(f"E0 benchmark dependency is not exactly pinned: {pin}")

contract = Path("SECURITY_ENCRYPTION_E0_CONTRACT.md").read_text()
for marker in (
    "Threat model",
    "Explicit non-goals / limits",
    "Argon2id",
    "AEGIS-256",
    "XChaCha20-Poly1305",
    "AES-256-GCM",
    "Privacy boundary: plaintext derivatives",
    "Search contract",
    "Attachment contract",
    "Integrity and blockchain-inspired ideas",
    "Quantum-resistance posture",
    "E1 entry criteria",
):
    if marker not in contract:
        raise SystemExit(f"E0 security contract is missing required section/marker: {marker}")

# E0's isolation rule evolves cumulatively: E2 may use the vault facade, but
# low-level external primitives still cannot be scattered through app code.
for path in Path("src").rglob("*.rs"):
    text = path.read_text(errors="ignore")
    for marker in ("use aegis::", "use argon2::", "use chacha20poly1305::", "use aes_gcm::"):
        if marker in text:
            raise SystemExit(f"Low-level crypto primitive leaked outside rustnote-vault: {path}: {marker}")
PY

if [[ "${RUSTNOTE_E0_STATIC_ONLY:-0}" == "1" ]]; then
  echo "Security E0 static contract passed (build $BUILD_ID; Cargo checks skipped by RUSTNOTE_E0_STATIC_ONLY=1)."
  exit 0
fi

cargo metadata --locked --format-version 1 >/dev/null
echo "Security E0 contract passed (build $BUILD_ID)."

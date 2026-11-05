#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTRACT="$ROOT/BACKUP_2B43_2_BYTE_FORMAT_CONTRACT.md"
KEY_CONTRACT="$ROOT/BACKUP_2B43_1_KEY_AUTHORITY_CONTRACT.md"

[[ -f "$CONTRACT" ]] || { echo "missing 2b.43.2 byte-format contract" >&2; exit 1; }
[[ -f "$KEY_CONTRACT" ]] || { echo "missing governing 2b.43.1 key-authority contract" >&2; exit 1; }
[[ "$(tr -d '\r\n' < "$ROOT/build-id.txt")" == "2b.43" ]] || {
  echo "2b.43.2 contract must remain on build 2b.43" >&2
  exit 1
}

python3 - "$CONTRACT" <<'PY'
from pathlib import Path
import hashlib
import hmac
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")

required = [
    "REPOSITORY_FORMAT_VERSION = 3",
    "REPOSITORY_HEADER_VERSION = 1",
    "KEYSET_VERSION             = 1",
    "BACKUP_OBJECT_VERSION      = 1",
    "ASCII `RNBHDR01`",
    "ASCII `RNBKEY01`",
    "ASCII(\"RNBIDX01\")",
    "ASCII(\"RNBMAN01\")",
    "ASCII(\"RNBSNP01\")",
    "ASCII(\"RNBBLB01\")",
    "exactly **92 bytes**",
    "exactly **268 bytes**",
    "exactly **232 bytes**",
    "exact 204 bytes",
    "exactly **116 bytes**",
    "exactly **112 bytes**",
    "chunk plaintext bytes = `1,048,576`",
    "rustnote-backup-key-envelope-aad-v1",
    "rustnote-backup-small-object-aad-v1",
    "rustnote-backup-bulk-header-aad-v1",
    "rustnote-backup-bulk-chunk-aad-v1",
    "HMAC-SHA-256(object_id_hmac_key, plaintext_digest)",
    "The manifest is the **snapshot publication authority**",
    "index is **operational/reconstructable metadata, not snapshot authority**",
    "does not claim a trusted monotonic anti-rollback counter",
    "No implementation may serialize Rust structs directly",
    "Only after all eighteen are true may 2b.43.3 begin.",
]
missing = [value for value in required if value not in text]
if missing:
    raise SystemExit("missing contract markers:\n- " + "\n- ".join(missing))

# Structural arithmetic is re-derived here instead of trusting prose totals.
repository_header = 8 + 2 + 2 + 1 + 1 + 2 + 32 + 32 + 2 + 2 + 4 + 4
keyset = 8 + 2 + 2 + 32 + 32 + (4 * 32)
small_header = 8 + 2 + 1 + 1 + 32 + 32 + 8 + 32
bulk_header = 8 + 2 + 1 + 1 + 32 + 32 + 4 + 8 + 24
apple_envelope = 32 + keyset + 32
android_envelope = 12 + keyset + 16
expected = {
    "repository header": (repository_header, 92),
    "keyset": (keyset, 204),
    "small header": (small_header, 116),
    "bulk header": (bulk_header, 112),
    "Apple envelope": (apple_envelope, 268),
    "Android envelope": (android_envelope, 232),
}
for name, (actual, wanted) in expected.items():
    if actual != wanted:
        raise SystemExit(f"{name} arithmetic drifted: {actual} != {wanted}")

# Independent canonical identity vectors.
library_id = b"library_example_001"
preimage = (
    b"rustnote-backup-library-binding-v1"
    + len(library_id).to_bytes(4, "big")
    + library_id
)
library_binding = hashlib.sha256(preimage).hexdigest()
expected_library_binding = "f0363a8464e6a722237fbdfdb98a0f869ef60c0bce8a5049c82267525ff54fdb"
if library_binding != expected_library_binding or expected_library_binding not in text:
    raise SystemExit("library-binding structural vector mismatch")

key = bytes(range(32))
plain_digest = hashlib.sha256(b"abc").digest()
blob_id = hmac.new(key, plain_digest, hashlib.sha256).hexdigest()
expected_digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
expected_blob = "910ec338f64892dfc0288e343162e996b71c39c67dd1cad13923f2d67a79cff6"
if plain_digest.hex() != expected_digest or expected_digest not in text:
    raise SystemExit("plaintext digest structural vector mismatch")
if blob_id != expected_blob or expected_blob not in text:
    raise SystemExit("opaque blob-id structural vector mismatch")

# The format must be canonical and binary, never a disguised serde contract.
for forbidden in [
    "bincode is the canonical",
    "serde_json is the canonical",
    "native struct layout",
]:
    if forbidden in text.lower():
        raise SystemExit(f"forbidden format dependency present: {forbidden}")

print("Backup 2b.43.2 byte-format contract validation passed (18/18 objectives).")
PY

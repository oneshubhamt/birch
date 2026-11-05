#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ -n "$BUILD_ID" ]] || {
  echo "Rustnote build-id.txt is empty." >&2
  exit 1
}

grep -Fq 'dioxus = { version = "=0.7.10", features = ["desktop"] }' Cargo.toml || {
  echo "Rustnote dioxus dependency is not exactly pinned to 0.7.10." >&2
  exit 1
}
grep -Fq 'dioxus-desktop = "=0.7.10"' Cargo.toml || {
  echo "Rustnote dioxus-desktop dependency is not exactly pinned to 0.7.10." >&2
  exit 1
}

python3 - <<'PY_LOCK'
from pathlib import Path
import re

text = Path("Cargo.lock").read_text()
packages = {}
for block in text.split("[[package]]")[1:]:
    name_match = re.search(r'^name = "([^"]+)"$', block, re.MULTILINE)
    version_match = re.search(r'^version = "([^"]+)"$', block, re.MULTILINE)
    if not name_match or not version_match:
        continue
    packages.setdefault(name_match.group(1), set()).add(version_match.group(1))

required = {
    "dioxus": "0.7.10",
    "dioxus-desktop": "0.7.10",
}
for name, expected in required.items():
    versions = packages.get(name, set())
    if versions != {expected}:
        raise SystemExit(
            f"{name} lock versions are {sorted(versions)!r}, expected only {expected}"
        )

family = {
    name: versions
    for name, versions in packages.items()
    if name == "dioxus"
    or name.startswith("dioxus-")
    or name == "manganis"
    or name.startswith("manganis-")
}
wrong = {
    name: sorted(versions)
    for name, versions in family.items()
    if versions != {"0.7.10"}
}
if wrong:
    raise SystemExit(f"Dioxus/Manganis baseline is not uniformly 0.7.10: {wrong}")
PY_LOCK

if [[ "${RUSTNOTE_DIOXUS_STATIC_ONLY:-0}" == "1" ]]; then
  echo "Dioxus 0.7.10 source/lock static baseline passed (build $BUILD_ID; Cargo metadata skipped)."
  exit 0
fi

command -v cargo >/dev/null 2>&1 || {
  echo "cargo is required for full Dioxus baseline validation." >&2
  exit 1
}

cargo metadata --locked --format-version 1 >/dev/null

echo "Dioxus 0.7.10 source/lock baseline passed (build $BUILD_ID)."

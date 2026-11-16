#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
from pathlib import Path
import plistlib
import re
import tomllib

root = Path(".")

with (root / "Cargo.toml").open("rb") as handle:
    cargo = tomllib.load(handle)
assert cargo["package"]["name"] == "rustnote"
assert cargo["package"]["version"] == "0.12.6"
assert cargo["dependencies"]["dioxus"]["version"] == "=0.7.10"
assert cargo["dependencies"]["dioxus-desktop"] == "=0.7.10"

assert (root / "build-id.txt").read_text().strip() == "2b.43"

toolchain = tomllib.loads((root / "rust-toolchain.toml").read_text())
assert toolchain["toolchain"]["channel"] == "1.97.1"
assert {"rustfmt", "clippy"} <= set(toolchain["toolchain"]["components"])

lock = (root / "Cargo.lock").read_text()
assert '[[package]]\nname = "rustnote"\nversion = "0.12.6"' in lock
assert '[[package]]\nname = "rustnote"\nversion = "0.12.5"' not in lock

packages = {}
for block in lock.split("[[package]]")[1:]:
    name = re.search(r'^name = "([^"]+)"$', block, re.MULTILINE)
    version = re.search(r'^version = "([^"]+)"$', block, re.MULTILINE)
    if name and version:
        packages.setdefault(name.group(1), set()).add(version.group(1))
for name in ("dioxus", "dioxus-desktop"):
    assert packages.get(name) == {"0.7.10"}, (name, packages.get(name))
wrong_family = {
    name: sorted(versions)
    for name, versions in packages.items()
    if (name == "dioxus" or name.startswith("dioxus-") or name == "manganis" or name.startswith("manganis-"))
    and versions != {"0.7.10"}
}
assert not wrong_family, wrong_family

config = (root / ".cargo/config.toml").read_text()
assert re.search(
    r'(?m)^\s*MACOSX_DEPLOYMENT_TARGET\s*=\s*\{\s*value\s*=\s*"12\.0"\s*,\s*force\s*=\s*true\s*\}\s*$',
    config,
)
with (root / "bundle/macos/Info.plist").open("rb") as handle:
    plist = plistlib.load(handle)
assert plist["CFBundleShortVersionString"] == "0.12.6"
assert plist["CFBundleVersion"] == "0.12.6"
assert plist["LSMinimumSystemVersion"] == "12.0"

vault_manifest = (root / "crates/rustnote-vault/Cargo.toml").read_text()
assert 'aegis = "=0.9.15"' in vault_manifest
assert "pure-rust" not in vault_manifest
assert 'name = "aegis"\nversion = "0.9.15"' in lock

for script in (
    "scripts/prepare-rustnote-mac.sh",
    "scripts/run-rustnote-mac.sh",
    "scripts/build-rustnote-macos-app.sh",
):
    assert "export MACOSX_DEPLOYMENT_TARGET=" not in (root / script).read_text(), script

build_info = (root / "src/build_info.rs").read_text()
for marker in (
    'env!("CARGO_PKG_VERSION")',
    'env!("RUSTNOTE_BUILD_ID")',
    'concat!(env!("CARGO_PKG_VERSION"), "+", env!("RUSTNOTE_BUILD_ID"))',
):
    assert marker in build_info, marker

print("qa-toolchain: pinned version, Rust, Dioxus, crypto and macOS deployment contracts passed.")
PY

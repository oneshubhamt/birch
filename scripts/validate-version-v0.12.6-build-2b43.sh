#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
python3 - <<'PY'
from pathlib import Path
import plistlib
import tomllib

root = Path('.')
with (root/'Cargo.toml').open('rb') as f:
    cargo = tomllib.load(f)
assert cargo['package']['name'] == 'rustnote'
assert cargo['package']['version'] == '0.12.6', cargo['package']['version']

build_id = (root/'build-id.txt').read_text().strip()
assert build_id == '2b.43', build_id

lock = (root/'Cargo.lock').read_text()
assert '[[package]]\nname = "rustnote"\nversion = "0.12.6"' in lock
assert '[[package]]\nname = "rustnote"\nversion = "0.12.5"' not in lock

with (root/'bundle/macos/Info.plist').open('rb') as f:
    plist = plistlib.load(f)
assert plist['CFBundleShortVersionString'] == '0.12.6'
assert plist['CFBundleVersion'] == '0.12.6'

build_info = (root/'src/build_info.rs').read_text()
assert 'env!("CARGO_PKG_VERSION")' in build_info
assert 'env!("RUSTNOTE_BUILD_ID")' in build_info
assert 'concat!(env!("CARGO_PKG_VERSION"), "+", env!("RUSTNOTE_BUILD_ID"))' in build_info

print('Rustnote version/build promotion validation passed: v0.12.6+2b.43')
PY

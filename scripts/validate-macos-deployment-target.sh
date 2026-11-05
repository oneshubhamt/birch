#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
from pathlib import Path
import re

config_path = Path('.cargo/config.toml')
assert config_path.is_file(), 'missing .cargo/config.toml'
config = config_path.read_text()
env_match = re.search(r'(?ms)^\[env\]\s*(.*?)(?=^\[|\Z)', config)
assert env_match, 'missing [env] section in .cargo/config.toml'
env_body = env_match.group(1)
entry = re.search(
    r'(?m)^\s*MACOSX_DEPLOYMENT_TARGET\s*=\s*\{\s*value\s*=\s*"12\.0"\s*,\s*force\s*=\s*true\s*\}\s*$',
    env_body,
)
assert entry, 'MACOSX_DEPLOYMENT_TARGET must be exactly { value = "12.0", force = true }'
plist = Path('bundle/macos/Info.plist').read_text()
assert '<key>LSMinimumSystemVersion</key>\n    <string>12.0</string>' in plist
vault_cargo = Path('crates/rustnote-vault/Cargo.toml').read_text()
assert 'aegis = "=0.9.15"' in vault_cargo
assert 'pure-rust' not in vault_cargo
lock = Path('Cargo.lock').read_text()
assert 'name = "aegis"\nversion = "0.9.15"' in lock
for script in ('scripts/prepare-rustnote-mac.sh','scripts/run-rustnote-mac.sh','scripts/build-rustnote-macos-app.sh'):
    text = Path(script).read_text()
    assert 'export MACOSX_DEPLOYMENT_TARGET=' not in text, script
print('Rustnote macOS 12.0 deployment-target source contract passed.')
PY

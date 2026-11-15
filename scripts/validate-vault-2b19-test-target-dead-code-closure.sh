#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ "$BUILD_ID" == "2b.19" || "$BUILD_ID" == "2b.20" || "$BUILD_ID" == "2b.21" || "$BUILD_ID" == "2b.22" || "$BUILD_ID" == "2b.23" || "$BUILD_ID" == "2b.24" || "$BUILD_ID" == "2b.25" || "$BUILD_ID" == "2b.26" || "$BUILD_ID" == "2b.27" || "$BUILD_ID" == "2b.28" || "$BUILD_ID" == "2b.29" || "$BUILD_ID" == "2b.30" || "$BUILD_ID" == "2b.31" || "$BUILD_ID" == "2b.32" || "$BUILD_ID" == "2b.33" || "$BUILD_ID" == "2b.34" || "$BUILD_ID" == "2b.35" || "$BUILD_ID" == "2b.36" || "$BUILD_ID" == "2b.37" || "$BUILD_ID" == "2b.38" || "$BUILD_ID" == "2b.39" || "$BUILD_ID" == "2b.40" || "$BUILD_ID" == "2b.41" || "$BUILD_ID" == "2b.43" ]] || { echo "Unexpected Rustnote source build: $BUILD_ID" >&2; exit 1; }


python3 - <<'PY'
from pathlib import Path
root = Path('.')
build_id = (root/'build-id.txt').read_text().strip()
app_paths = (root/'src/app_paths.rs').read_text()
output = (root/'src/native/output.rs').read_text()

assert 'use std::{env, path::PathBuf};' in app_paths
assert '#[allow(dead_code)]' not in app_paths
assert '#[allow(dead_code)]' not in output

if build_id in ('2b.20', '2b.21', '2b.22', '2b.23', '2b.24', '2b.25', '2b.26', '2b.27', '2b.28', '2b.29', '2b.30', '2b.31', '2b.32', '2b.33', '2b.34', '2b.35', '2b.36', '2b.37', '2b.38', '2b.39', '2b.40', '2b.41', '2b.43'):
    production_cfg = '#[cfg(not(any(test, feature = "internal-test-support")))]'
    test_cfg = '#[cfg(any(test, feature = "internal-test-support"))]'
    assert f'{production_cfg}\nuse std::io;' in app_paths
    assert f'{production_cfg}\npub(crate) fn durable_configuration_directory() -> io::Result<PathBuf> {{' in app_paths
    assert f'{production_cfg}\nconst OUTPUT_JOURNAL_NAMESPACE: &str = "output-transfers";' in output
    assert f'{production_cfg}\nfn journal_root() -> io::Result<PathBuf> {{' in output
    assert f'{test_cfg}\nfn journal_root() -> io::Result<PathBuf> {{' in output
else:
    assert '#[cfg(not(test))]\nuse std::io;' in app_paths
    assert '#[cfg(not(test))]\npub(crate) fn durable_configuration_directory() -> io::Result<PathBuf> {' in app_paths
    assert '#[cfg(not(test))]\nconst OUTPUT_JOURNAL_NAMESPACE: &str = "output-transfers";' in output
    assert '#[cfg(not(test))]\nfn journal_root() -> io::Result<PathBuf> {' in output
    assert '#[cfg(test)]\nfn journal_root() -> io::Result<PathBuf> {' in output

assert 'crate::app_paths::durable_configuration_directory()?.join(OUTPUT_JOURNAL_NAMESPACE)' in output
assert 'rustnote-output-journal-test-{}' in output
print('Rustnote 2b.19+ test-target dead-code closure static guard passed.')
PY
bash scripts/validate-vault-2b18-strict-clippy-closure.sh

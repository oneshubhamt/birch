#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for command_name in bash node python3; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Missing source QA dependency: $command_name" >&2
    exit 1
  }
done

echo '[CI] Bash shell syntax'
while IFS= read -r -d '' script; do
  bash -n "$script"
done < <(find scripts -type f -name '*.sh' -print0)

echo '[CI] JavaScript syntax'
while IFS= read -r -d '' file; do
  node --check "$file" > /dev/null
done < <(find . -type f \( -name '*.js' -o -name '*.mjs' \) \
  -not -path './target/*' -not -path './node_modules/*' \
  -not -path './editor-build/node_modules/*' -print0)

echo '[CI] Baseline source validators'
for script in \
  scripts/validate-version-v0.12.6-build-2b43.sh \
  scripts/validate-backup-2b43-1-key-contract.sh \
  scripts/validate-backup-2b43-2-byte-format.sh \
  scripts/validate-backup-2b43-3-secure-key-store.sh; do
  bash "$script"
done

echo '[CI] Current native source/invariant suite'
bash scripts/validate-native-current.sh

echo '[CI] Source checks completed successfully'

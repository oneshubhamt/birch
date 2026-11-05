#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

[[ -f src/editor_runtime_generated.rs ]] || {
  echo "The prepared editor runtime is missing."
  echo "Run ./scripts/prepare-rustnote-mac.sh once, then rerun this safety net."
  exit 1
}

./scripts/validate-dioxus-0.7.10-baseline.sh

echo
echo "Architecture safety net passed."
echo "Full Rust behavior is validated separately by the serial workspace test gate."

#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

for command_name in rustup cargo; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "$command_name is required to run Rustnote."
    echo "Run ./scripts/prepare-rustnote-mac.sh first."
    exit 1
  }
done

echo "Starting optimized Rustnote..."
exec cargo run --release --locked

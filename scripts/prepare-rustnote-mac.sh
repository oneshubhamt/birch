#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

for command_name in rustup cargo; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "$command_name is required to prepare Rustnote."
    echo "Install Rust from https://rustup.rs, reopen Terminal, then retry."
    exit 1
  }
done

command -v xcrun >/dev/null 2>&1 || {
  echo "Apple's Xcode Command Line Tools are required to link the macOS app and system frameworks."
  echo "Run: xcode-select --install"
  exit 1
}

./scripts/qa-toolchain.sh
./scripts/prepare-rustnote-browser-runtimes.sh

echo "Building optimized Rustnote..."
cargo build --release --locked
./scripts/validate-macos-binary-deployment-target.sh "$ROOT_DIR/target/release/rustnote"

echo
echo "Rustnote is ready."
echo "Launch it with:"
echo "  ./scripts/run-rustnote-mac.sh"

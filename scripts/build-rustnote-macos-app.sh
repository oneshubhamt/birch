#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

for command_name in cargo dx ditto plutil; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "$command_name is required to build Rustnote.app." >&2
    exit 1
  }
done

DX_VERSION="$(dx --version 2>/dev/null | head -n 1)"
if [[ "$DX_VERSION" != *"0.7.10"* ]]; then
  echo "Rustnote is pinned to Dioxus 0.7.10; expected dx 0.7.10, got: $DX_VERSION" >&2
  echo "Upgrade with: cargo binstall dioxus-cli@0.7.10 --force" >&2
  exit 1
fi

BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
[[ -n "$BUILD_ID" ]] || {
  echo "Rustnote build-id.txt is empty." >&2
  exit 1
}

./scripts/validate-macos-deployment-target.sh
./scripts/validate-dioxus-0.7.10-baseline.sh

echo "Preparing Rustnote browser runtimes..."
./scripts/prepare-rustnote-browser-runtimes.sh

# Dioxus 0.7.2+ handles release-profile stripping around asset extraction
# itself. Keep Rustnote's normal strip = true release profile intact and let
# dx 0.7.10 perform its supported bundling pipeline without overrides.
echo "Building Rustnote.app with Dioxus 0.7.10 asset processing..."
rm -rf "$ROOT_DIR/dist/Rustnote.app"
if [[ -d "$ROOT_DIR/target/dx" ]]; then
  find "$ROOT_DIR/target/dx" -type d -name 'Rustnote.app' -prune -exec rm -rf {} + 2>/dev/null || true
fi

dx bundle --desktop --release --package-types macos

DX_APP="$(find "$ROOT_DIR/target/dx" -type d -name 'Rustnote.app' -prune 2>/dev/null | head -n 1)"
[[ -n "$DX_APP" && -d "$DX_APP" ]] || {
  echo "DX completed without producing Rustnote.app under target/dx." >&2
  exit 1
}

./scripts/validate-rustnote-macos-app.sh "$DX_APP"

mkdir -p "$ROOT_DIR/dist"
/usr/bin/ditto "$DX_APP" "$ROOT_DIR/dist/Rustnote.app"
./scripts/validate-rustnote-macos-app.sh "$ROOT_DIR/dist/Rustnote.app"

echo
echo "Rustnote.app is ready:"
echo "  $ROOT_DIR/dist/Rustnote.app"
echo
echo "Launch it with:"
echo "  open \"$ROOT_DIR/dist/Rustnote.app\""
echo
echo "Install it with:"
echo "  sudo rm -rf /Applications/Rustnote.app"
echo "  sudo ditto \"$ROOT_DIR/dist/Rustnote.app\" /Applications/Rustnote.app"

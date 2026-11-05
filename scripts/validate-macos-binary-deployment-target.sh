#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${1:-$ROOT_DIR/target/release/rustnote}"

command -v otool >/dev/null 2>&1 || {
  echo "otool is required to validate the macOS deployment target." >&2
  exit 1
}
[[ -x "$BINARY" ]] || {
  echo "Rustnote executable does not exist or is not executable: $BINARY" >&2
  exit 1
}

MINIMUM_MACOS_VERSION="$(otool -l "$BINARY" | awk '
  $1 == "cmd" {
    mode = ($2 == "LC_BUILD_VERSION" ? "build" : ($2 == "LC_VERSION_MIN_MACOSX" ? "legacy" : ""))
    next
  }
  mode == "build" && $1 == "minos" { print $2; exit }
  mode == "legacy" && $1 == "version" { print $2; exit }
')"

case "$MINIMUM_MACOS_VERSION" in
  12.0|12.0.0) ;;
  *)
    echo "Rustnote executable minimum macOS version is ${MINIMUM_MACOS_VERSION:-<missing>}; expected 12.0." >&2
    exit 1
    ;;
esac

echo "Rustnote Mach-O deployment target validated: macOS $MINIMUM_MACOS_VERSION"

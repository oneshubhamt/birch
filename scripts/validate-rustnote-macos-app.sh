#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

APP="${1:-$ROOT_DIR/dist/Rustnote.app}"
[[ -d "$APP" ]] || {
  echo "Rustnote.app does not exist: $APP" >&2
  exit 1
}

EXECUTABLE="$APP/Contents/MacOS/rustnote"
PLIST="$APP/Contents/Info.plist"
RESOURCES="$APP/Contents/Resources"
BUILD_ID="$(tr -d '[:space:]' < build-id.txt)"
VERSION="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"

[[ -x "$EXECUTABLE" ]] || { echo "Rustnote executable is missing." >&2; exit 1; }
[[ -f "$PLIST" ]] || { echo "Rustnote Info.plist is missing." >&2; exit 1; }
[[ -d "$RESOURCES" ]] || { echo "Rustnote Contents/Resources is missing." >&2; exit 1; }
plutil -lint "$PLIST" >/dev/null

[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$PLIST")" == "rustnote" ]] || {
  echo "CFBundleExecutable is invalid." >&2
  exit 1
}
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$PLIST")" == "local.rustnote.app" ]] || {
  echo "CFBundleIdentifier is invalid." >&2
  exit 1
}
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundlePackageType' "$PLIST")" == "APPL" ]] || {
  echo "CFBundlePackageType is invalid." >&2
  exit 1
}
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST")" == "$VERSION" ]] || {
  echo "CFBundleShortVersionString is invalid." >&2
  exit 1
}
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$PLIST")" == "$VERSION" ]] || {
  echo "CFBundleVersion is invalid." >&2
  exit 1
}
[[ "$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$PLIST")" == "12.0" ]] || {
  echo "LSMinimumSystemVersion must be 12.0." >&2
  exit 1
}

./scripts/validate-macos-binary-deployment-target.sh "$EXECUTABLE"

# Dioxus may preserve or hash asset filenames depending on the platform and
# bundler path. The bundle contract is the presence and validity of the actual
# first-class resources, not an implementation-specific string inside the
# executable.
MAIN_CSS="$(find "$RESOURCES" -type f \( -name 'main-*.css' -o -name 'main.css' \) | head -n 1)"
KATEX_CSS="$(find "$RESOURCES" -type f \( -name 'katex-embedded-*.css' -o -name 'katex-embedded.css' \) | head -n 1)"
EDITOR_JS="$(find "$RESOURCES" -type f \( -name 'prosemirror-editor-*.js' -o -name 'prosemirror-editor.js' \) | head -n 1)"

[[ -n "$MAIN_CSS" && -s "$MAIN_CSS" ]] || {
  echo "Dioxus did not bundle Rustnote main.css." >&2
  exit 1
}
[[ -n "$KATEX_CSS" && -s "$KATEX_CSS" ]] || {
  echo "Dioxus did not bundle Rustnote KaTeX CSS." >&2
  exit 1
}
[[ -n "$EDITOR_JS" && -s "$EDITOR_JS" ]] || {
  echo "Dioxus did not bundle the ProseMirror runtime." >&2
  exit 1
}

/usr/bin/grep -q -- 'note-' "$MAIN_CSS" || {
  echo "Bundled main stylesheet does not look like Rustnote CSS." >&2
  exit 1
}
/usr/bin/grep -q -- 'katex' "$KATEX_CSS" || {
  echo "Bundled KaTeX stylesheet is invalid." >&2
  exit 1
}
/usr/bin/grep -q -- 'ProseMirror' "$EDITOR_JS" || {
  echo "Bundled editor runtime is invalid." >&2
  exit 1
}

/usr/bin/grep -aFq -- "${VERSION}+${BUILD_ID}" "$EXECUTABLE" || {
  echo "Packaged executable does not contain the expected Rustnote build identifier." >&2
  exit 1
}

echo "Rustnote macOS bundle validated:"
echo "  $APP"
echo "  main CSS: ${MAIN_CSS#$APP/}"
echo "  KaTeX CSS: ${KATEX_CSS#$APP/}"
echo "  editor JS: ${EDITOR_JS#$APP/}"
du -sh "$APP"

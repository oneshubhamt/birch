#!/bin/sh
set -eu

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="5.7.284"
DISTRIBUTION="npm-legacy"
ARCHIVE_NAME="pdfjs-dist-${VERSION}.tgz"
ARCHIVE_URL="https://registry.npmjs.org/pdfjs-dist/-/${ARCHIVE_NAME}"
# npm integrity for pdfjs-dist@5.7.284:
# sha512-h4EdYQczmGhbOlqc3PPZwxevn7ApdWPbovAuWXOB/DjIyigSnwfy2oze7c6mRcSr9XgLp3eN3EeL4DyySTPMFw==
EXPECTED_SHA512="87811d61073398685b3a5a9cdcf3d9c317af9fb0297563dba2f02e597381fc38c8ca28129f07f2da8cdeedcea645c4abf5780ba7778ddc478be03cb24933cc17"
PDFJS_BUILD_DIR="target/rustnote-pdfjs"
CACHE_DIR="$PDFJS_BUILD_DIR/cache"
RUNTIME_DIR="$PDFJS_BUILD_DIR/runtime"
UPSTREAM_DIR="$PDFJS_BUILD_DIR/upstream"
ARCHIVE="$CACHE_DIR/$ARCHIVE_NAME"
FONT_DIR="$RUNTIME_DIR/standard_fonts"
WASM_DIR="$RUNTIME_DIR/wasm"
FONT_LIST="$RUNTIME_DIR/STANDARD-FONTS.txt"
FONT_LICENSE_LIST="$RUNTIME_DIR/STANDARD-FONT-LICENSES.txt"
WASM_LIST="$RUNTIME_DIR/WASM-ASSETS.txt"
FONT_MAP_RUST="$RUNTIME_DIR/standard_fonts.rs"
WASM_MAP_RUST="$RUNTIME_DIR/wasm_assets.rs"

for command_name in curl shasum tar awk mktemp node npm; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "$command_name is required to prepare the PDF.js runtime." >&2
    exit 1
  }
done

mkdir -p "$CACHE_DIR"
rm -rf "$UPSTREAM_DIR" "$RUNTIME_DIR"
mkdir -p "$UPSTREAM_DIR/build" "$UPSTREAM_DIR/web" "$FONT_DIR" "$WASM_DIR"

archive_valid() {
  [ -s "$ARCHIVE" ] && [ "$(shasum -a 512 "$ARCHIVE" | awk '{print $1}')" = "$EXPECTED_SHA512" ]
}

if ! archive_valid; then
  rm -f "$ARCHIVE"
  temp_archive="$(mktemp "$CACHE_DIR/pdfjs-download.XXXXXX")"
  trap 'rm -f "$temp_archive"' EXIT HUP INT TERM
  echo "Downloading pinned PDF.js ${VERSION} npm package..."
  curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
    "$ARCHIVE_URL" -o "$temp_archive"
  actual_sha512="$(shasum -a 512 "$temp_archive" | awk '{print $1}')"
  if [ "$actual_sha512" != "$EXPECTED_SHA512" ]; then
    echo "PDF.js package checksum mismatch; refusing to use the download." >&2
    exit 1
  fi
  mv "$temp_archive" "$ARCHIVE"
  trap - EXIT HUP INT TERM
fi

extract_verified() {
  member="$1"
  destination="$2"
  mkdir -p "$(dirname "$destination")"
  temp_file="$(mktemp "$UPSTREAM_DIR/pdfjs-file.XXXXXX")"
  if ! tar -xOf "$ARCHIVE" "$member" > "$temp_file"; then
    rm -f "$temp_file"
    echo "PDF.js package is missing $member" >&2
    exit 1
  fi
  [ -s "$temp_file" ] || {
    rm -f "$temp_file"
    echo "PDF.js package is missing $member" >&2
    exit 1
  }
  mv "$temp_file" "$destination"
}

# Use Mozilla's pinned legacy subtree, then bundle it for a conservative WebKit
# syntax target. This is the same known-good compatibility approach used by the
# working 1A.6A branch, while retaining the 2A.1 local-I/O/WASM improvements.
extract_verified "package/legacy/build/pdf.mjs" "$UPSTREAM_DIR/build/pdf.mjs"
extract_verified "package/legacy/build/pdf.worker.mjs" "$UPSTREAM_DIR/build/pdf.worker.mjs"
extract_verified "package/legacy/web/pdf_viewer.mjs" "$UPSTREAM_DIR/web/pdf_viewer.mjs"
extract_verified "package/LICENSE" "assets/pdfjs/PDFJS-LICENSE.txt"

: > "$FONT_LIST"
tar -tzf "$ARCHIVE" | while IFS= read -r member; do
  case "$member" in
    package/standard_fonts/*.pfb|package/standard_fonts/*.ttf) ;;
    *) continue ;;
  esac
  relative_name="${member#package/standard_fonts/}"
  case "$relative_name" in
    ''|*/*|*[!A-Za-z0-9._-]*)
      echo "Unexpected PDF.js standard-font filename: $relative_name" >&2
      exit 1
      ;;
  esac
  extract_verified "$member" "$FONT_DIR/$relative_name"
  printf '%s\n' "$relative_name" >> "$FONT_LIST"
done
font_count="$(wc -l < "$FONT_LIST" | tr -d '[:space:]')"
[ "$font_count" -ge 10 ] || {
  echo "PDF.js package did not provide the expected standard-font set." >&2
  exit 1
}

: > "$FONT_LICENSE_LIST"
tar -tzf "$ARCHIVE" | while IFS= read -r member; do
  case "$member" in
    package/standard_fonts/LICENSE_*) ;;
    *) continue ;;
  esac
  relative_name="${member#package/standard_fonts/}"
  case "$relative_name" in
    ''|*/*|*[!A-Za-z0-9._-]*)
      echo "Unexpected PDF.js standard-font license filename: $relative_name" >&2
      exit 1
      ;;
  esac
  extract_verified "$member" "$RUNTIME_DIR/$relative_name"
  printf '%s\n' "$relative_name" >> "$FONT_LICENSE_LIST"
done
[ -s "$FONT_LICENSE_LIST" ] || {
  echo "PDF.js package did not provide standard-font license notices." >&2
  exit 1
}

: > "$WASM_LIST"
tar -tzf "$ARCHIVE" | while IFS= read -r member; do
  case "$member" in
    package/wasm/openjpeg.wasm|package/wasm/openjpeg_nowasm_fallback.js|package/wasm/qcms_bg.wasm|package/wasm/jbig2.wasm|package/wasm/jbig2_nowasm_fallback.js|package/wasm/LICENSE_*) ;;
    *) continue ;;
  esac
  relative_name="${member#package/wasm/}"
  case "$relative_name" in
    ''|*/*|*[!A-Za-z0-9._-]*)
      echo "Unexpected PDF.js WASM asset filename: $relative_name" >&2
      exit 1
      ;;
  esac
  extract_verified "$member" "$WASM_DIR/$relative_name"
  printf '%s\n' "$relative_name" >> "$WASM_LIST"
done
for required_wasm in openjpeg.wasm qcms_bg.wasm jbig2.wasm; do
  grep -qx "$required_wasm" "$WASM_LIST" || {
    echo "PDF.js package did not provide $required_wasm." >&2
    exit 1
  }
done

# Compile-time private-protocol lookups. There is now exactly one PDF.js runtime,
# so profile dispatch and profile-scoped duplicate assets are intentionally gone.
{
  echo 'pub(crate) fn pdf_standard_font_asset(name: &str) -> Option<&'"'"'static [u8]> {'
  echo '    match name {'
  while IFS= read -r name; do
    printf '        "%s" => Some(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/rustnote-pdfjs/runtime/standard_fonts/%s"))),\n' "$name" "$name"
  done < "$FONT_LIST"
  echo '        _ => None,'
  echo '    }'
  echo '}'
} > "$FONT_MAP_RUST"

{
  echo 'pub(crate) fn pdf_wasm_asset(name: &str) -> Option<&'"'"'static [u8]> {'
  echo '    match name {'
  while IFS= read -r name; do
    case "$name" in
      *.wasm|*.js)
        printf '        "%s" => Some(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/target/rustnote-pdfjs/runtime/wasm/%s"))),\n' "$name" "$name"
        ;;
    esac
  done < "$WASM_LIST"
  echo '        _ => None,'
  echo '    }'
  echo '}'
} > "$WASM_MAP_RUST"

printf '%s\n' "$VERSION" > "$RUNTIME_DIR/VERSION"
printf '%s\n' "$EXPECTED_SHA512" > "$RUNTIME_DIR/PACKAGE-SHA512"
printf '%s\n' "$DISTRIBUTION" > "$RUNTIME_DIR/DISTRIBUTION"

esbuild_ready() {
  node -e "const expected=require('./editor-build/package.json').devDependencies.esbuild; const actual=require('./editor-build/node_modules/esbuild/package.json').version; process.exit(actual === expected ? 0 : 1)" >/dev/null 2>&1
}

if ! esbuild_ready; then
  echo "Installing Rustnote's locked JavaScript build tooling..."
  (cd editor-build && npm ci --no-audit --no-fund)
fi
esbuild_ready || {
  echo "Rustnote's pinned esbuild runtime is unavailable after npm ci." >&2
  exit 1
}

echo "Building PDF.js ${VERSION} legacy runtime for WebKit..."
node ./scripts/build-pdfjs-runtime.mjs

for path in \
  "$RUNTIME_DIR/viewer.mjs" \
  "$RUNTIME_DIR/pdf.worker.mjs" \
  "$RUNTIME_DIR/MANIFEST.json" \
  "$RUNTIME_DIR/VERSION" \
  "$RUNTIME_DIR/PACKAGE-SHA512" \
  "$RUNTIME_DIR/DISTRIBUTION" \
  "$RUNTIME_DIR/STANDARD-FONTS.txt" \
  "$RUNTIME_DIR/STANDARD-FONT-LICENSES.txt" \
  "$RUNTIME_DIR/WASM-ASSETS.txt" \
  "$FONT_MAP_RUST" \
  "$WASM_MAP_RUST"
do
  [ -s "$path" ] || {
    echo "PDF.js runtime preparation failed; missing $path" >&2
    exit 1
  }
done

# Upstream modules are verified build inputs only. The prepared viewer and worker
# are self-contained so WKWebView never imports raw upstream PDF.js modules.
rm -rf "$UPSTREAM_DIR"

echo "Pinned PDF.js ${VERSION} ${DISTRIBUTION} WebKit runtime is prepared."

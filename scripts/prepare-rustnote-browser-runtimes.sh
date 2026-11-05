#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

for command_name in node npm; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "$command_name is required to prepare Rustnote's browser runtimes." >&2
    echo "Install Node.js, reopen Terminal, then retry." >&2
    exit 1
  }
done

echo "Preparing Rustnote's locked JavaScript build dependencies..."
if ! node <<'NODE' >/dev/null 2>&1
const fs = require('node:fs');
const path = require('node:path');
const root = process.cwd();
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'editor-build', 'package.json'), 'utf8'));
for (const [name, expected] of Object.entries({...(pkg.dependencies || {}), ...(pkg.devDependencies || {})})) {
  const installedPath = path.join(root, 'editor-build', 'node_modules', name, 'package.json');
  if (!fs.existsSync(installedPath)) process.exit(1);
  const actual = JSON.parse(fs.readFileSync(installedPath, 'utf8')).version;
  if (actual !== expected) process.exit(1);
}
NODE
then
  echo "Installing exact versions from editor-build/package-lock.json..."
  (cd editor-build && npm ci --no-audit --no-fund)
fi

echo "Preparing the pinned PDF.js runtime..."
./scripts/prepare-pdfjs-runtime.sh

echo "Building the pinned ProseMirror editor runtime from source..."
rm -f assets/prosemirror-editor.js assets/prosemirror-editor-manifest.json src/editor_runtime_generated.rs
node ./scripts/build-prosemirror-editor.mjs

echo "Validating prepared browser runtimes..."
./scripts/validate-prepared-runtime.sh

#!/bin/sh
set -eu

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

for command_name in node; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "$command_name is required to validate the prepared runtime." >&2
    exit 1
  }
done

PDF_RUNTIME="target/rustnote-pdfjs/runtime"
for path in \
  "$PDF_RUNTIME/viewer.mjs" \
  "$PDF_RUNTIME/pdf.worker.mjs" \
  "$PDF_RUNTIME/MANIFEST.json" \
  "$PDF_RUNTIME/VERSION" \
  "$PDF_RUNTIME/PACKAGE-SHA512" \
  "$PDF_RUNTIME/DISTRIBUTION" \
  "$PDF_RUNTIME/STANDARD-FONT-LICENSES.txt" \
  "$PDF_RUNTIME/standard_fonts.rs" \
  "$PDF_RUNTIME/wasm_assets.rs" \
  "assets/prosemirror-editor.js" \
  "assets/prosemirror-editor-manifest.json" \
  "src/editor_runtime_generated.rs"
do
  [ -s "$path" ] || {
    echo "Prepared runtime artifact is missing: $path" >&2
    exit 1
  }
done

for stale in \
  "$PDF_RUNTIME/native" \
  "$PDF_RUNTIME/compat" \
  "$PDF_RUNTIME/pdf.mjs" \
  "$PDF_RUNTIME/pdf_viewer.mjs" \
  "$PDF_RUNTIME/app.mjs" \
  "$PDF_RUNTIME/NATIVE-VERSION" \
  "$PDF_RUNTIME/COMPAT-VERSION" \
  "$PDF_RUNTIME/NATIVE-PACKAGE-SHA512" \
  "$PDF_RUNTIME/COMPAT-PACKAGE-SHA512"
do
  [ ! -e "$stale" ] || {
    echo "Stale dual/generic PDF runtime asset remains: $stale" >&2
    exit 1
  }
done

[ ! -e "target/rustnote-pdfjs/upstream" ] || {
  echo "PDF.js upstream build inputs must not remain after preparation." >&2
  exit 1
}
[ ! -e "assets/pdfjs/runtime" ] && [ ! -e "assets/pdfjs/.cache" ] || {
  echo "Generated PDF.js runtime/cache must remain outside Dioxus assets." >&2
  exit 1
}
[ ! -e "scripts/pdfjs-webkit-compat.js" ] || {
  echo "Obsolete PDF.js WebKit shim source remains in the codebase." >&2
  exit 1
}

node <<'NODE'
const fs = require('node:fs');
const crypto = require('node:crypto');
const vm = require('node:vm');

const readBytes = path => fs.readFileSync(path);
const readText = path => fs.readFileSync(path, 'utf8');
const readTrimmed = path => readText(path).trim();
const sha256Bytes = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const sha256 = path => sha256Bytes(readBytes(path));
const sha512 = path => crypto.createHash('sha512').update(readBytes(path)).digest('hex');
const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};
const canonicalJson = value => {
  if (Array.isArray(value)) return value.map(canonicalJson);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map(key => [key, canonicalJson(value[key])]));
  }
  return value;
};
const sameJson = (left, right) => JSON.stringify(canonicalJson(left)) === JSON.stringify(canonicalJson(right));

// Prepared PDF.js artifact integrity. Source semantics belong to architecture/
// behavioral tests; this validator only proves the prepared artifact matches
// the canonical current inputs and pinned runtime configuration.
const runtime = 'target/rustnote-pdfjs/runtime';
const pdfManifest = JSON.parse(readText(`${runtime}/MANIFEST.json`));
const packageDocument = JSON.parse(readText('editor-build/package.json'));
const expectedPdfVersion = '5.7.284';
const expectedPdfPackageSha512 = '87811d61073398685b3a5a9cdcf3d9c317af9fb0297563dba2f02e597381fc38c8ca28129f07f2da8cdeedcea645c4abf5780ba7778ddc478be03cb24933cc17';

assert(readTrimmed(`${runtime}/VERSION`) === expectedPdfVersion, 'Prepared PDF runtime version pin is invalid');
assert(readTrimmed(`${runtime}/PACKAGE-SHA512`) === expectedPdfPackageSha512, 'Prepared PDF package SHA-512 pin is invalid');
assert(readTrimmed(`${runtime}/DISTRIBUTION`) === 'npm-legacy', 'Prepared PDF distribution pin is invalid');
assert(pdfManifest.bundle_kind === 'rustnote-pdfjs-webkit-legacy', 'Prepared PDF bundle kind is invalid');
assert(pdfManifest.runtime_profile === 'legacy', 'Prepared PDF runtime profile is invalid');
assert(pdfManifest.pdfjs_version === expectedPdfVersion, 'Prepared PDF manifest version is invalid');
assert(pdfManifest.pdfjs_distribution === 'npm-legacy', 'Prepared PDF manifest distribution is invalid');
assert(pdfManifest.pdfjs_package_sha512 === expectedPdfPackageSha512, 'Prepared PDF manifest package pin is invalid');
assert(sameJson(pdfManifest.javascript_syntax_target, ['safari14']), 'Prepared PDF JavaScript target is invalid');
assert(pdfManifest.build_dependency === packageDocument.devDependencies.esbuild, 'Prepared PDF esbuild pin is invalid');
assert(pdfManifest.worker_url === '/rustnotepdf/runtime/legacy/pdf.worker.mjs', 'Prepared PDF worker URL is invalid');
assert(pdfManifest.standard_font_data_url === '/rustnotepdf/runtime/legacy/standard_fonts/', 'Prepared PDF standard-font URL is invalid');
assert(pdfManifest.wasm_url === '/rustnotepdf/runtime/legacy/wasm/', 'Prepared PDF WASM URL is invalid');
assert(pdfManifest.viewer_output_sha256 === sha256(`${runtime}/viewer.mjs`), 'Prepared PDF viewer checksum mismatch');
assert(pdfManifest.worker_output_sha256 === sha256(`${runtime}/pdf.worker.mjs`), 'Prepared PDF worker checksum mismatch');
assert(pdfManifest.viewer_output_bytes === fs.statSync(`${runtime}/viewer.mjs`).size, 'Prepared PDF viewer size mismatch');
assert(pdfManifest.worker_output_bytes === fs.statSync(`${runtime}/pdf.worker.mjs`).size, 'Prepared PDF worker size mismatch');

const pdfSourceFiles = fs.readdirSync('pdf-src').filter(name => name.endsWith('.mjs')).sort();
assert(pdfSourceFiles.length > 0 && pdfSourceFiles.includes('viewer.mjs'), 'Canonical PDF source inventory is invalid');
const expectedPdfSourceInputs = Object.fromEntries(pdfSourceFiles.map(name => [name, sha256(`pdf-src/${name}`)]));
assert(sameJson(pdfManifest.viewer_source_inputs || {}, expectedPdfSourceInputs), 'Prepared PDF modular-source checksum map mismatch');
const pdfSourceAggregate = sha256Bytes(Buffer.concat(pdfSourceFiles.map(name => readBytes(`pdf-src/${name}`))));
assert(pdfManifest.viewer_source_sha256 === pdfSourceAggregate, 'Prepared PDF modular-source aggregate checksum mismatch');

const fontNames = fs.readdirSync(`${runtime}/standard_fonts`).filter(name => /\.(?:pfb|ttf)$/i.test(name)).sort();
assert(fontNames.length >= 10, 'Prepared PDF runtime is missing Mozilla standard fonts');
const expectedFontHashes = Object.fromEntries(fontNames.map(name => [name, sha256(`${runtime}/standard_fonts/${name}`)]));
assert(sameJson(pdfManifest.standard_font_files_sha256 || {}, expectedFontHashes), 'Prepared PDF standard-font checksum map mismatch');
const licenseNames = readText(`${runtime}/STANDARD-FONT-LICENSES.txt`).split(/\r?\n/).filter(Boolean);
assert(licenseNames.length > 0, 'Prepared PDF standard-font licenses are missing');
for (const name of licenseNames) {
  assert(/^LICENSE_[A-Za-z0-9._-]+$/.test(name) && fs.existsSync(`${runtime}/${name}`), `Prepared PDF standard-font license is invalid or missing: ${name}`);
}

const wasmNames = fs.readdirSync(`${runtime}/wasm`).filter(name => /\.(?:wasm|js)$/i.test(name)).sort();
for (const required of ['openjpeg.wasm', 'openjpeg_nowasm_fallback.js', 'qcms_bg.wasm', 'jbig2.wasm', 'jbig2_nowasm_fallback.js']) {
  assert(wasmNames.includes(required), `Prepared PDF runtime is missing ${required}`);
}
assert(!wasmNames.some(name => name.startsWith('quickjs-eval.')), 'PDF scripting WASM assets must not be packaged while scripting is disabled');
const expectedWasmHashes = Object.fromEntries(wasmNames.map(name => [name, sha256(`${runtime}/wasm/${name}`)]));
assert(sameJson(pdfManifest.wasm_files_sha256 || {}, expectedWasmHashes), 'Prepared PDF WASM checksum map mismatch');

const generatedFontMap = readText(`${runtime}/standard_fonts.rs`);
assert(generatedFontMap.includes('pdf_standard_font_asset(name: &str)'), 'Generated PDF standard-font Rust map has the wrong API');
for (const name of fontNames) assert(generatedFontMap.includes(JSON.stringify(name)), `Generated PDF standard-font Rust map omits ${name}`);
const generatedWasmMap = readText(`${runtime}/wasm_assets.rs`);
assert(generatedWasmMap.includes('pdf_wasm_asset(name: &str)'), 'Generated PDF WASM Rust map has the wrong API');
for (const name of wasmNames) assert(generatedWasmMap.includes(JSON.stringify(name)), `Generated PDF WASM Rust map omits ${name}`);

const preparedViewer = readText(`${runtime}/viewer.mjs`);
for (const forbidden of ['rustnote-pdfjs-display', 'rustnote-pdfjs-viewer', '/rustnotepdf/native/', '/rustnotepdf/compat/', '6.2.108']) {
  assert(!preparedViewer.includes(forbidden), `Prepared PDF viewer retains obsolete/build-only runtime input: ${forbidden}`);
}

// Prepared ProseMirror artifact integrity.
const runtimeInputsPath = 'editor-src/runtime-inputs.json';
const runtimeInputsBytes = readBytes(runtimeInputsPath);
const runtimeInputs = JSON.parse(runtimeInputsBytes.toString('utf8'));
const editorSourceInputs = runtimeInputs.editor_source_inputs;
const vendoredRuntimeInputs = runtimeInputs.vendored_runtime_inputs;
for (const [kind, inputs] of [['editor source', editorSourceInputs], ['vendored runtime', vendoredRuntimeInputs]]) {
  assert(Array.isArray(inputs) && inputs.length > 0, `Canonical ${kind} runtime inputs are missing`);
  assert(new Set(inputs).size === inputs.length, `Canonical ${kind} runtime inputs contain duplicates`);
  assert(inputs.every(path => typeof path === 'string' && path.length > 0 && !path.includes('..') && fs.existsSync(path)), `Canonical ${kind} runtime inputs contain an invalid path`);
}
assert(editorSourceInputs[0] === 'editor-src/prosemirror-editor.js', 'Canonical editor entry is invalid');

const editorManifest = JSON.parse(readText('assets/prosemirror-editor-manifest.json'));
const documentSpec = JSON.parse(readText('editor-spec/document-spec.json'));
const editorPackage = JSON.parse(readText('editor-build/package.json'));
const expectedEditorSourceInputs = Object.fromEntries(editorSourceInputs.map(path => [path, sha256(path)]));
const expectedVendoredInputs = Object.fromEntries(vendoredRuntimeInputs.map(path => [path, sha256(path)]));
const editorRuntimeSourceSha = sha256Bytes(Buffer.concat([...editorSourceInputs, ...vendoredRuntimeInputs].map(readBytes)));
const expectedEditorMarker = `RUSTNOTE_PROSEMIRROR_${editorRuntimeSourceSha.slice(0, 24).toUpperCase()}`;

assert(editorManifest.bundle_kind === 'direct-prosemirror' && editorManifest.built === true, 'Prepared editor manifest kind is invalid');
assert(editorManifest.editor_engine === documentSpec.envelope.editor_engine, 'Prepared editor engine identifier is invalid');
assert(editorManifest.bridge_protocol_version === 1, 'Prepared editor bridge protocol version is invalid');
assert(editorManifest.runtime_marker === expectedEditorMarker, 'Prepared editor runtime marker mismatch');
assert(editorManifest.envelope_schema_version === documentSpec.envelope.schema_version, 'Prepared editor envelope schema version mismatch');
assert(editorManifest.document_schema === documentSpec.envelope.document_schema, 'Prepared editor document schema mismatch');
assert(editorManifest.document_schema_version === documentSpec.envelope.document_schema_version, 'Prepared editor document schema version mismatch');
assert(editorManifest.entry === 'editor-src/prosemirror-editor.js', 'Prepared editor entry path is invalid');
assert(editorManifest.output === 'assets/prosemirror-editor.js', 'Prepared editor output path is invalid');
assert(editorManifest.runtime_inputs === runtimeInputsPath, 'Prepared editor runtime-input authority is invalid');
assert(editorManifest.runtime_inputs_sha256 === sha256Bytes(runtimeInputsBytes), 'Prepared editor runtime-input manifest checksum mismatch');
assert(editorManifest.entry_sha256 === sha256('editor-src/prosemirror-editor.js'), 'Prepared editor entry checksum mismatch');
assert(editorManifest.runtime_source_sha256 === editorRuntimeSourceSha, 'Prepared editor aggregate source checksum mismatch');
assert(sameJson(editorManifest.editor_source_inputs || {}, expectedEditorSourceInputs), 'Prepared editor modular-source checksum map mismatch');
assert(sameJson(editorManifest.vendored_runtime_inputs || {}, expectedVendoredInputs), 'Prepared editor vendored-runtime checksum map mismatch');
assert(sameJson(editorManifest.dependencies || {}, editorPackage.dependencies || {}), 'Prepared editor dependency manifest mismatch');
assert(editorManifest.build_dependency === editorPackage.devDependencies.esbuild, 'Prepared editor esbuild pin mismatch');
assert(sameJson(editorManifest.browser_target, ['safari12']), 'Prepared editor browser target mismatch');
assert(editorManifest.delivery === 'dioxus-local-asset', 'Prepared editor delivery contract is invalid');
assert(editorManifest.output_sha256 === sha256('assets/prosemirror-editor.js'), 'Prepared editor bundle checksum mismatch');
assert(editorManifest.output_bytes === fs.statSync('assets/prosemirror-editor.js').size, 'Prepared editor bundle size mismatch');

const generatedEditorRust = readText('src/editor_runtime_generated.rs');
assert(generatedEditorRust.includes(`EDITOR_RUNTIME_MARKER: &str = ${JSON.stringify(expectedEditorMarker)}`), 'Generated Rust editor runtime marker mismatch');
assert(generatedEditorRust.includes(`EDITOR_RUNTIME_SHA256: &str = ${JSON.stringify(editorManifest.output_sha256)}`), 'Generated Rust editor runtime checksum mismatch');

const editorBundle = readText('assets/prosemirror-editor.js');
assert(editorBundle.includes(expectedEditorMarker), 'Prepared editor bundle is missing its source-derived runtime marker');
const bridgeHostSource = readText('editor-src/bridge/host.js');
const bridgeCommands = [...bridgeHostSource.matchAll(/case\s+['"]([^'"]+)['"]\s*:/g)].map(match => match[1]);
assert(bridgeCommands.length > 0, 'Canonical editor bridge exposes no commands');
for (const command of bridgeCommands) {
  assert(editorBundle.includes(`'${command}'`) || editorBundle.includes(`"${command}"`), `Prepared editor bundle is missing bridge command: ${command}`);
}
for (const legacy of ['__rustnoteInstallEditor', '__rustnoteSyncEditor', '__rustnoteSetAttachmentRegistry', '__rustnoteCompleteImageImport', '__rustnoteCompleteImageEdit', '__rustnoteFormatEditor', '__rustnoteEditorController']) {
  assert(!editorBundle.includes(legacy) && !bridgeHostSource.includes(legacy), `Legacy editor bridge global survived preparation: ${legacy}`);
}

const sandbox = {
  console: {log() {}, warn() {}, error() {}},
  document: {documentElement: {style: {}}},
  navigator: {userAgent: '', vendor: '', platform: ''},
  setTimeout,
  clearTimeout,
};
sandbox.window = sandbox;
vm.runInNewContext(editorBundle, sandbox, {filename: 'assets/prosemirror-editor.js', timeout: 5_000});
const editorBridge = sandbox.__rustnoteEditorBridge;
assert(editorBridge && typeof editorBridge.dispatch === 'function', 'Prepared editor runtime did not register __rustnoteEditorBridge.dispatch');
assert(editorBridge.version === 1 && editorBridge.marker === expectedEditorMarker, 'Prepared editor runtime registered an unexpected bridge version/marker');
assert(editorBridge.dispatch({version: 2, command: 'dispose', payload: {flush: false}}) === false, 'Prepared editor runtime accepted an invalid bridge version');
assert(editorBridge.dispatch({version: 1, command: 'future_command', payload: {}}) === false, 'Prepared editor runtime accepted an unknown command');
assert(editorBridge.dispatch({version: 1, command: 'dispose', payload: []}) === false, 'Prepared editor runtime accepted an invalid payload envelope');

console.log(`Prepared PDF.js ${expectedPdfVersion}: ${pdfSourceFiles.length} source modules, ${fontNames.length} fonts, ${wasmNames.length} WASM/runtime assets.`);
console.log(`Prepared editor: ${editorSourceInputs.length} source inputs, ${vendoredRuntimeInputs.length} vendored inputs, ${bridgeCommands.length} typed commands.`);
NODE

node --check "$PDF_RUNTIME/viewer.mjs"
node --check "$PDF_RUNTIME/pdf.worker.mjs"
node --check assets/prosemirror-editor.js

echo "Prepared Rustnote runtimes are valid."

import {createHash} from 'node:crypto';
import {readFile, writeFile} from 'node:fs/promises';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import vm from 'node:vm';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDirectory, '..');
const runtimeInputsPath = path.join(root, 'editor-src', 'runtime-inputs.json');
const runtimeInputsBytes = await readFile(runtimeInputsPath);
const runtimeInputsDocument = JSON.parse(runtimeInputsBytes.toString('utf8'));

function canonicalRuntimeInputs(name) {
  const inputs = runtimeInputsDocument[name];
  if (!Array.isArray(inputs) || inputs.length === 0) {
    throw new Error(`Editor runtime input manifest is missing a non-empty ${name} array`);
  }
  const seen = new Set();
  return inputs.map(relativePath => {
    if (typeof relativePath !== 'string' || relativePath.length === 0 || path.isAbsolute(relativePath)) {
      throw new Error(`Editor runtime input manifest contains an invalid ${name} path`);
    }
    if (relativePath.includes('..') || seen.has(relativePath)) {
      throw new Error(`Editor runtime input manifest contains an unsafe/duplicate ${name} path: ${relativePath}`);
    }
    seen.add(relativePath);
    return path.join(root, relativePath);
  });
}

const editorSourceInputs = canonicalRuntimeInputs('editor_source_inputs');
const vendoredRuntimeInputs = canonicalRuntimeInputs('vendored_runtime_inputs');
const entry = editorSourceInputs[0];
if (path.relative(root, entry) !== 'editor-src/prosemirror-editor.js') {
  throw new Error('Editor runtime input manifest must keep prosemirror-editor.js as the first entry');
}
const output = path.join(root, 'assets', 'prosemirror-editor.js');
const manifestPath = path.join(root, 'assets', 'prosemirror-editor-manifest.json');
const generatedRustPath = path.join(root, 'src', 'editor_runtime_generated.rs');
const buildProject = path.join(root, 'editor-build');
const buildNodeModules = path.join(buildProject, 'node_modules');
const packagePath = path.join(buildProject, 'package.json');
const requireFromBuildProject = createRequire(packagePath);
const {build} = requireFromBuildProject('esbuild');

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function smokePreparedEditorRuntime(bundleText, expectedMarker) {
  const sandbox = {
    console: {log() {}, warn() {}, error() {}},
    document: {documentElement: {style: {}}},
    navigator: {userAgent: '', vendor: '', platform: ''},
    setTimeout,
    clearTimeout,
  };
  sandbox.window = sandbox;
  vm.runInNewContext(bundleText, sandbox, {
    filename: 'assets/prosemirror-editor.js',
    timeout: 5_000,
  });
  const bridge = sandbox.__rustnoteEditorBridge;
  if (!bridge || typeof bridge.dispatch !== 'function') {
    throw new Error('Prepared editor runtime executed without registering __rustnoteEditorBridge.dispatch');
  }
  if (bridge.version !== 1 || bridge.marker !== expectedMarker) {
    throw new Error('Prepared editor runtime registered an unexpected bridge version/runtime marker');
  }
  if (bridge.dispatch({version: 2, command: 'dispose', payload: {flush: false}}) !== false
      || bridge.dispatch({version: 1, command: 'future_command', payload: {}}) !== false
      || bridge.dispatch({version: 1, command: 'dispose', payload: []}) !== false) {
    throw new Error('Prepared editor runtime accepted an invalid bridge envelope');
  }
}

const packageDocument = JSON.parse(await readFile(packagePath, 'utf8'));
const documentSpec = JSON.parse(await readFile(path.join(root, 'editor-spec', 'document-spec.json'), 'utf8'));
const documentEnvelope = documentSpec.envelope;
const editorSources = await Promise.all(editorSourceInputs.map(async input => ({
  path: path.relative(root, input),
  bytes: await readFile(input),
})));
const source = editorSources[0].bytes;
const sourceText = editorSources.map(input => input.bytes.toString('utf8')).join('\n');
const entrySha256 = sha256(source);
const vendoredRuntimeSources = await Promise.all(vendoredRuntimeInputs.map(async input => ({
  path: path.relative(root, input),
  bytes: await readFile(input),
})));
const runtimeSourceSha256 = sha256(Buffer.concat([
  ...editorSources.map(input => input.bytes),
  ...vendoredRuntimeSources.map(input => input.bytes),
]));
const runtimeMarker = `RUSTNOTE_PROSEMIRROR_${runtimeSourceSha256.slice(0, 24).toUpperCase()}`;
for (const dependency of Object.keys(packageDocument.dependencies ?? {})) {
  try {
    requireFromBuildProject.resolve(dependency);
  } catch (error) {
    throw new Error(`Installed ProseMirror dependency could not be resolved: ${dependency}`, {cause: error});
  }
}
await build({
  absWorkingDir: root,
  nodePaths: [buildNodeModules],
  entryPoints: [entry],
  outfile: output,
  bundle: true,
  format: 'iife',
  platform: 'browser',
  target: ['safari12'],
  charset: 'utf8',
  legalComments: 'inline',
  logLevel: 'info',
  minify: false,
  sourcemap: false,
  treeShaking: true,
  define: {
    __RUSTNOTE_RUNTIME_MARKER__: JSON.stringify(runtimeMarker),
  },
});

const bundle = await readFile(output);
const bundleText = bundle.toString('utf8');

const bridgeHostSource = editorSources
  .find(input => input.path === 'editor-src/bridge/host.js')
  ?.bytes.toString('utf8') || '';
const requiredBridgeCommands = [...bridgeHostSource.matchAll(/case\s+['"]([^'"]+)['"]\s*:/g)]
  .map(match => match[1]);
if (requiredBridgeCommands.length === 0) {
  throw new Error('Prepared editor source exposes no bridge commands');
}
for (const command of requiredBridgeCommands) {
  const singleQuoted = `'${command}'`;
  const doubleQuoted = `"${command}"`;
  if (!bundleText.includes(singleQuoted) && !bundleText.includes(doubleQuoted)) {
    throw new Error(`Prepared editor runtime is missing bridge command: ${command}`);
  }
}

for (const requiredContract of [
  runtimeMarker,
  '__rustnoteEditorBridge',
]) {
  if (!bundleText.includes(requiredContract)) {
    throw new Error(`Prepared editor runtime is missing core bridge contract: ${requiredContract}`);
  }
}
for (const legacyBridgeGlobal of [
  '__rustnoteInstallEditor',
  '__rustnoteSyncEditor',
  '__rustnoteSetAttachmentRegistry',
  '__rustnoteCompleteImageImport',
  '__rustnoteCompleteImageEdit',
  '__rustnoteFormatEditor',
  '__rustnoteEditorController',
]) {
  if (sourceText.includes(legacyBridgeGlobal) || bundleText.includes(legacyBridgeGlobal)) {
    throw new Error(`Legacy editor bridge global survived A2: ${legacyBridgeGlobal}`);
  }
}
smokePreparedEditorRuntime(bundleText, runtimeMarker);
const manifest = {
  bundle_kind: 'direct-prosemirror',
  built: true,
  editor_engine: documentEnvelope.editor_engine,
  bridge_protocol_version: 1,
  runtime_marker: runtimeMarker,
  envelope_schema_version: documentEnvelope.schema_version,
  document_schema: documentEnvelope.document_schema,
  document_schema_version: documentEnvelope.document_schema_version,
  entry: 'editor-src/prosemirror-editor.js',
  output: 'assets/prosemirror-editor.js',
  runtime_inputs: 'editor-src/runtime-inputs.json',
  runtime_inputs_sha256: sha256(runtimeInputsBytes),
  entry_sha256: entrySha256,
  runtime_source_sha256: runtimeSourceSha256,
  editor_source_inputs: Object.fromEntries(
    editorSources.map(input => [input.path, sha256(input.bytes)]),
  ),
  vendored_runtime_inputs: Object.fromEntries(
    vendoredRuntimeSources.map(input => [input.path, sha256(input.bytes)]),
  ),
  output_sha256: sha256(bundle),
  output_bytes: bundle.length,
  dependencies: packageDocument.dependencies,
  build_dependency: packageDocument.devDependencies.esbuild,
  browser_target: ['safari12'],
  delivery: 'dioxus-local-asset',
};
await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
const generatedRust = `// Generated by scripts/build-prosemirror-editor.mjs. Do not edit manually.\n`
  + `pub const EDITOR_RUNTIME_MARKER: &str = ${JSON.stringify(runtimeMarker)};\n`
  + `pub const EDITOR_RUNTIME_SHA256: &str = ${JSON.stringify(manifest.output_sha256)};\n`;
await writeFile(generatedRustPath, generatedRust, 'utf8');
console.log(`ProseMirror runtime: ${output}`);
console.log(`SHA-256: ${manifest.output_sha256}`);

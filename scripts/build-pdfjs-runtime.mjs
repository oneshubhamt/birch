import {createHash} from 'node:crypto';
import {readdir, readFile, writeFile} from 'node:fs/promises';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDirectory, '..');
const pdfjsBuildDirectory = path.join(root, 'target', 'rustnote-pdfjs');
const runtimeDirectory = path.join(pdfjsBuildDirectory, 'runtime');
const upstreamDirectory = path.join(pdfjsBuildDirectory, 'upstream');
const viewerSourceDirectory = path.join(root, 'pdf-src');
const viewerEntry = path.join(viewerSourceDirectory, 'viewer.mjs');
const upstreamDisplay = path.join(upstreamDirectory, 'build', 'pdf.mjs');
const upstreamWorker = path.join(upstreamDirectory, 'build', 'pdf.worker.mjs');
const upstreamViewer = path.join(upstreamDirectory, 'web', 'pdf_viewer.mjs');
const viewerOutput = path.join(runtimeDirectory, 'viewer.mjs');
const workerOutput = path.join(runtimeDirectory, 'pdf.worker.mjs');
const manifestOutput = path.join(runtimeDirectory, 'MANIFEST.json');
const buildPackagePath = path.join(root, 'editor-build', 'package.json');
const requireFromBuildProject = createRequire(buildPackagePath);
const {build} = requireFromBuildProject('esbuild');

const PDF_RUNTIME_PROFILE = 'legacy';
const PDFJS_VERSION = '5.7.284';
const PDF_RUNTIME_BASE_URL = `/rustnotepdf/runtime/${PDF_RUNTIME_PROFILE}`;
const WEBKIT_SYNTAX_TARGET = ['safari14'];

const pdfjsResolvers = {
  name: 'rustnote-pdfjs-runtime-inputs',
  setup(buildContext) {
    buildContext.onResolve(
      {filter: /^rustnote-pdfjs-display$/},
      () => ({path: upstreamDisplay}),
    );
    buildContext.onResolve(
      {filter: /^rustnote-pdfjs-viewer$/},
      () => ({path: upstreamViewer}),
    );
    // Mozilla's reusable viewer module imports the display layer through this
    // package-style specifier. Resolve it to the exact same pinned legacy
    // display module so versions can never be mixed.
    buildContext.onResolve(
      {filter: /^pdfjs-lib$/},
      () => ({path: upstreamDisplay}),
    );
  },
};

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

async function readTrimmed(filePath) {
  return (await readFile(filePath, 'utf8')).trim();
}

async function bundle(entryPoint, outfile, plugins = []) {
  const result = await build({
    absWorkingDir: root,
    entryPoints: [entryPoint],
    outfile,
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: WEBKIT_SYNTAX_TARGET,
    charset: 'utf8',
    legalComments: 'eof',
    logLevel: 'info',
    minify: true,
    sourcemap: false,
    treeShaking: true,
    metafile: true,
    define: {
      __RUSTNOTE_PDF_RUNTIME_PROFILE__: JSON.stringify(PDF_RUNTIME_PROFILE),
      __RUSTNOTE_PDF_RUNTIME_BASE_URL__: JSON.stringify(PDF_RUNTIME_BASE_URL),
      __RUSTNOTE_PDFJS_EXPECTED_VERSION__: JSON.stringify(PDFJS_VERSION),
    },
    plugins,
  });

  const output = Object.entries(result.metafile.outputs)
    .find(([reportedPath]) => path.resolve(root, reportedPath) === outfile)?.[1];
  if (!output) throw new Error(`esbuild did not report the expected output: ${outfile}`);
  if (output.imports.length !== 0) {
    throw new Error(`Prepared PDF runtime still contains external imports: ${output.imports.map(item => item.path).join(', ')}`);
  }
}

const viewerSourceNames = (await readdir(viewerSourceDirectory))
  .filter(name => name.endsWith('.mjs'))
  .sort();
const viewerSourceInputs = Object.fromEntries(await Promise.all(
  viewerSourceNames.map(async name => [name, await readFile(path.join(viewerSourceDirectory, name))]),
));
const viewerSource = viewerSourceInputs['viewer.mjs'];
if (!viewerSource) throw new Error('PDF viewer entry source is missing');

const [
  displaySource,
  workerSource,
  upstreamViewerSource,
  packageDocument,
] = await Promise.all([
  readFile(upstreamDisplay),
  readFile(upstreamWorker),
  readFile(upstreamViewer),
  readFile(buildPackagePath, 'utf8').then(JSON.parse),
]);

await bundle(viewerEntry, viewerOutput, [pdfjsResolvers]);
await bundle(upstreamWorker, workerOutput);

const viewerSourceText = Object.values(viewerSourceInputs).map(source => source.toString('utf8')).join('\n');
for (const requiredSourceContract of [
  'const PDF_RUNTIME_BASE_URL = __RUSTNOTE_PDF_RUNTIME_BASE_URL__',
  'const PDFJS_WORKER_URL = `${PDF_RUNTIME_BASE_URL}/pdf.worker.mjs`',
  'const PDFJS_STANDARD_FONT_DATA_URL = `${PDF_RUNTIME_BASE_URL}/standard_fonts/`',
  'const PDFJS_WASM_URL = `${PDF_RUNTIME_BASE_URL}/wasm/`',
  'GlobalWorkerOptions.workerSrc = PDFJS_WORKER_URL',
]) {
  if (!viewerSourceText.includes(requiredSourceContract)) {
    throw new Error(`PDF viewer source runtime contract is missing: ${requiredSourceContract}`);
  }
}

const [viewerBundle, workerBundle] = await Promise.all([
  readFile(viewerOutput),
  readFile(workerOutput),
]);
const viewerText = viewerBundle.toString('utf8');
// esbuild is free to keep the injected base URL in a temporary and concatenate
// suffixes at runtime, or to fold them into full literals. Validate semantic
// pieces rather than depending on one particular minified representation.
for (const requiredContract of [
  'WebKit-targeted bundled PDF.js runtime',
  PDF_RUNTIME_BASE_URL,
  'pdf.worker.mjs',
  'standard_fonts/',
  'wasm/',
  '/rustnotepdf/document/',
]) {
  if (!viewerText.includes(requiredContract)) {
    throw new Error(`Prepared PDF viewer runtime is missing: ${requiredContract}`);
  }
}
if (viewerText.includes('rustnote-pdfjs-display') || viewerText.includes('rustnote-pdfjs-viewer')) {
  throw new Error('Prepared PDF viewer still references a build-only PDF.js input');
}

const fontNames = (await readdir(path.join(runtimeDirectory, 'standard_fonts')))
  .filter(name => /\.(?:pfb|ttf)$/i.test(name))
  .sort();
if (fontNames.length < 10) {
  throw new Error('Prepared PDF runtime is missing Mozilla standard-font data');
}
const standardFontSha256 = Object.fromEntries(await Promise.all(
  fontNames.map(async name => [name, sha256(await readFile(path.join(runtimeDirectory, 'standard_fonts', name)))]),
));

const wasmNames = (await readdir(path.join(runtimeDirectory, 'wasm')))
  .filter(name => /\.(?:wasm|js)$/i.test(name))
  .sort();
for (const requiredWasm of ['openjpeg.wasm', 'qcms_bg.wasm', 'jbig2.wasm']) {
  if (!wasmNames.includes(requiredWasm)) {
    throw new Error(`Prepared PDF runtime is missing ${requiredWasm}`);
  }
}
const wasmSha256 = Object.fromEntries(await Promise.all(
  wasmNames.map(async name => [name, sha256(await readFile(path.join(runtimeDirectory, 'wasm', name)))]),
));

const manifest = {
  bundle_kind: 'rustnote-pdfjs-webkit-legacy',
  runtime_profile: PDF_RUNTIME_PROFILE,
  pdfjs_version: await readTrimmed(path.join(runtimeDirectory, 'VERSION')),
  pdfjs_distribution: await readTrimmed(path.join(runtimeDirectory, 'DISTRIBUTION')),
  pdfjs_package_sha512: await readTrimmed(path.join(runtimeDirectory, 'PACKAGE-SHA512')),
  javascript_syntax_target: WEBKIT_SYNTAX_TARGET,
  build_dependency: packageDocument.devDependencies.esbuild,
  worker_url: `${PDF_RUNTIME_BASE_URL}/pdf.worker.mjs`,
  standard_font_data_url: `${PDF_RUNTIME_BASE_URL}/standard_fonts/`,
  standard_font_files_sha256: standardFontSha256,
  wasm_url: `${PDF_RUNTIME_BASE_URL}/wasm/`,
  wasm_files_sha256: wasmSha256,
  viewer_source_sha256: sha256(Buffer.concat(viewerSourceNames.map(name => viewerSourceInputs[name]))),
  viewer_source_inputs: Object.fromEntries(viewerSourceNames.map(name => [name, sha256(viewerSourceInputs[name])])),
  upstream_display_sha256: sha256(displaySource),
  upstream_worker_sha256: sha256(workerSource),
  upstream_viewer_sha256: sha256(upstreamViewerSource),
  viewer_output_sha256: sha256(viewerBundle),
  worker_output_sha256: sha256(workerBundle),
  viewer_output_bytes: viewerBundle.length,
  worker_output_bytes: workerBundle.length,
};
await writeFile(manifestOutput, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');

console.log(`PDF.js ${manifest.pdfjs_version} legacy WebKit viewer: ${viewerOutput}`);
console.log(`PDF.js ${manifest.pdfjs_version} legacy WebKit worker: ${workerOutput}`);

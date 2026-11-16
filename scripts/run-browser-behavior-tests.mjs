import {existsSync, lstatSync, readFileSync, symlinkSync, unlinkSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const rootNodeModules = path.join(root, 'node_modules');
const buildNodeModules = path.join(root, 'editor-build', 'node_modules');
const dependencyOnly = process.argv.includes('--dependency-backed-only');
const dependencyMode = dependencyOnly || process.argv.includes('--require-editor-deps');
let temporaryLink = false;

if (!existsSync(rootNodeModules) && existsSync(buildNodeModules)) {
  symlinkSync(path.relative(root, buildNodeModules), rootNodeModules, 'dir');
  temporaryLink = true;
}

function pinnedEditorDependenciesAvailable() {
  if (!existsSync(rootNodeModules)) return false;
  const packageDocument = JSON.parse(readFileSync(path.join(root, 'editor-build', 'package.json'), 'utf8'));
  for (const [name, expected] of Object.entries(packageDocument.dependencies || {})) {
    const installed = path.join(rootNodeModules, name, 'package.json');
    if (!existsSync(installed)) return false;
    const actual = JSON.parse(readFileSync(installed, 'utf8')).version;
    if (actual !== expected) return false;
  }
  return true;
}

const prosemirrorAvailable = pinnedEditorDependenciesAvailable();
if (dependencyMode && !prosemirrorAvailable) {
  console.error('Dependency-backed editor behavior requires editor-build/node_modules. Run scripts/prepare-rustnote-mac.sh or npm ci in editor-build first.');
  process.exitCode = 1;
} else {
  const dependencyBackedTests = [
    path.join(root, 'tests', 'js', 'editor-prosemirror-behavior.test.mjs'),
    path.join(root, 'tests', 'js', 'editor-bridge-behavior.test.mjs'),
  ];
  const dependencyFreeTests = [
    path.join(root, 'tests', 'js', 'editor-ui-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'navigation-publication-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'note-local-privacy-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'multi-note-privacy-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'vault-action-privacy-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'library-wide-privacy-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'vault-profile-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'vault-no-vacuum-forensic-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'vault-cleanup-2b42-9-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'vault-independent-closure-2b42-10-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'note-list-resilience-contract.test.mjs'),
    path.join(root, 'tests', 'js', 'pdf-behavior.test.mjs'),
    path.join(root, 'tests', 'js', 'pdf-runtime-integrity.test.mjs'),
    path.join(root, 'tests', 'js', 'pdf-layout-integrity.test.mjs'),
  ];
  const tests = dependencyOnly ? [...dependencyBackedTests] : [...dependencyFreeTests];
  if (!dependencyOnly && prosemirrorAvailable) {
    tests.unshift(...dependencyBackedTests);
  } else if (!dependencyOnly && !prosemirrorAvailable) {
    console.log('Pinned editor dependencies absent; running dependency-free browser behavior only.');
  }
  const result = spawnSync(process.execPath, ['--test', ...tests], {cwd: root, stdio: 'inherit'});
  process.exitCode = result.status ?? 1;
}

if (temporaryLink) {
  try {
    if (lstatSync(rootNodeModules).isSymbolicLink()) unlinkSync(rootNodeModules);
  } catch {}
}

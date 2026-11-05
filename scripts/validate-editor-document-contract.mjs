import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {DOCUMENT_SPEC} from '../editor-src/document/spec.js';
import {validateSerializedEditorState} from '../editor-src/document/validation.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const corpus = JSON.parse(fs.readFileSync(path.join(root, 'editor-spec/validation-cases.json'), 'utf8'));
if (corpus.version !== 1 || !Array.isArray(corpus.cases) || corpus.cases.length === 0) {
  throw new Error('Rustnote editor document validation corpus is malformed');
}

const coveredNodes = new Set();
const coveredMarks = new Set();
function collectVocabulary(value) {
  if (!value || typeof value !== 'object') return;
  if (Array.isArray(value)) {
    value.forEach(collectVocabulary);
    return;
  }
  if (typeof value.type === 'string') {
    if (Object.prototype.hasOwnProperty.call(DOCUMENT_SPEC.nodes, value.type)) coveredNodes.add(value.type);
    if (Object.prototype.hasOwnProperty.call(DOCUMENT_SPEC.marks, value.type)) coveredMarks.add(value.type);
  }
  for (const child of Object.values(value)) collectVocabulary(child);
}

let failures = 0;
for (const testCase of corpus.cases) {
  let accepted = true;
  try {
    validateSerializedEditorState(JSON.stringify(testCase.state));
  } catch {
    accepted = false;
  }
  if (testCase.valid) collectVocabulary(testCase.state?.document);
  if (accepted !== testCase.valid) {
    console.error(`FAIL ${testCase.name}: expected ${testCase.valid ? 'accept' : 'reject'}, got ${accepted ? 'accept' : 'reject'}`);
    failures += 1;
  }
}
if (failures > 0) process.exit(1);
const missingNodes = Object.keys(DOCUMENT_SPEC.nodes).filter(name => !coveredNodes.has(name));
const missingMarks = Object.keys(DOCUMENT_SPEC.marks).filter(name => !coveredMarks.has(name));
if (missingNodes.length || missingMarks.length) {
  throw new Error(`Shared contract corpus lacks valid vocabulary coverage: nodes=[${missingNodes}], marks=[${missingMarks}]`);
}
console.log(`Editor document JS contract corpus passed (${corpus.cases.length} cases; ${coveredNodes.size} nodes; ${coveredMarks.size} marks).`);

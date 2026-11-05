import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const viewerSource = readFileSync(new URL('../../pdf-src/viewer.mjs', import.meta.url), 'utf8');
const transportSource = readFileSync(new URL('../../pdf-src/transport.mjs', import.meta.url), 'utf8');
const attachmentViewSource = readFileSync(new URL('../../editor-src/attachments/view.js', import.meta.url), 'utf8');

const {classifyPdfPreviewError} = await import('../../pdf-src/errors.mjs');
const {pdfTransportPlan} = await import('../../pdf-src/transport_policy.mjs');
const {clampPageNumber} = await import('../../pdf-src/virtualization.mjs');

function functionSource(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const open = source.indexOf('{', start);
  let depth = 0;
  let quote = '';
  let escaped = false;
  for (let index = open; index < source.length; index += 1) {
    const char = source[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      continue;
    }
    if (char === '{') depth += 1;
    else if (char === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(start, index + 1);
    }
  }
  throw new Error(`unterminated function ${name}`);
}

function methodSource(source, name) {
  const start = source.indexOf(`\n  ${name}(`);
  assert.notEqual(start, -1, `missing method ${name}`);
  const open = source.indexOf('{', start);
  let depth = 0;
  let quote = '';
  let escaped = false;
  for (let index = open; index < source.length; index += 1) {
    const char = source[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      continue;
    }
    if (char === '{') depth += 1;
    else if (char === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(start, index + 1);
    }
  }
  throw new Error(`unterminated method ${name}`);
}

test('PDF at or below whole-file threshold uses the one-shot transport path', () => {
  const threshold = 32 * 1024 * 1024;
  assert.deepEqual(pdfTransportPlan({
    length: threshold,
    wholeFileMaxBytes: threshold,
    blockBytes: 1024 * 1024,
  }), {mode: 'whole-file', length: threshold});
});

test('PDF above whole-file threshold uses range transport with one bounded initial block', () => {
  const threshold = 32 * 1024 * 1024;
  const length = threshold + 12 * 1024 * 1024;
  assert.deepEqual(pdfTransportPlan({
    length,
    wholeFileMaxBytes: threshold,
    blockBytes: 1024 * 1024,
  }), {mode: 'range', length, initialLength: 1024 * 1024});
});

test('transport module owns initial acquisition and viewer consumes one prepared document source', () => {
  assert.match(transportSource, /export async function preparePdfDocumentTransport\(/);
  assert.match(transportSource, /const initialData = await fetchPdfBlock\(/);
  assert.match(transportSource, /const range = new RustnotePdfRangeTransport\(/);
  assert.match(viewerSource, /await preparePdfDocumentTransport\(/);
  assert.doesNotMatch(viewerSource, /\bfetchPdfBlock\b/);
  assert.doesNotMatch(viewerSource, /\bfetchPdfWholeDocument\b/);
  assert.doesNotMatch(viewerSource, /new RustnotePdfRangeTransport\(/);
});

test('initial render memory accounting continues through PreviewRasterCache without stale budget orchestration', () => {
  const accounting = functionSource(viewerSource, 'updateMemoryAccounting');
  assert.match(accounting, /evictPreviewCacheToBudget\(\)/);
  assert.doesNotMatch(accounting, /updateAdaptivePreviewBudget/);
  assert.doesNotMatch(viewerSource, /\bupdateAdaptivePreviewBudget\b/);
});

test('Previous Next and direct page navigation clamp to one canonical page number', () => {
  assert.equal(clampPageNumber(0, 8), 1);
  assert.equal(clampPageNumber('5', 8), 5);
  assert.equal(clampPageNumber(99, 8), 8);
  assert.equal(clampPageNumber('bad', 8), 1);
  const navigation = functionSource(viewerSource, 'goToPage');
  assert.match(navigation, /const next = clampPageNumber\(value, pdf\.numPages\)/);
  assert.match(navigation, /geometryTopForNumber\(next\)/);
  assert.doesNotMatch(navigation, /\bpageNumber\b/);
  assert.match(viewerSource, /elements\.previous\.addEventListener\('click', \(\) => goToPage\(currentPage - 1\)\)/);
  assert.match(viewerSource, /elements\.next\.addEventListener\('click', \(\) => goToPage\(currentPage \+ 1\)\)/);
  assert.match(viewerSource, /elements\.pageNumber\.addEventListener\('change', \(\) => goToPage\(elements\.pageNumber\.value\)\)/);
});

test('viewer source has no known dangling viewer-owned PDF identifiers', () => {
  for (const identifier of ['fetchPdfBlock', 'fetchPdfWholeDocument', 'updateAdaptivePreviewBudget']) {
    assert.doesNotMatch(viewerSource, new RegExp(`\\b${identifier}\\b`));
  }
  assert.match(viewerSource, /import \{pdfDocumentMetadata, preparePdfDocumentTransport\} from '\.\/transport\.mjs';/);
  assert.match(viewerSource, /import \{classifyPdfPreviewError\} from '\.\/errors\.mjs';/);
  assert.match(viewerSource, /import \{FenwickTree, clampPageNumber\} from '\.\/virtualization\.mjs';/);
});

test('preview close releases range/session state so a later reopen starts cleanly', () => {
  assert.match(viewerSource, /window\.addEventListener\('pagehide'/);
  assert.match(viewerSource, /activePdfRangeTransport\?\.abort\(\)/);
  assert.match(viewerSource, /activePdfRangeTransport = null/);
  assert.match(viewerSource, /method: 'DELETE'/);
  assert.match(viewerSource, /pdfSessionReleaseUrl = ''/);
  assert.match(viewerSource, /async function start\(\)/);
  assert.match(viewerSource, /attachmentId = attachmentIdFromViewerParameters\(\)/);
  assert.match(viewerSource, /const metadata = await pdfDocumentMetadata\(/);

  const toggle = methodSource(attachmentViewSource, 'togglePdfPreview');
  const reset = methodSource(attachmentViewSource, 'resetPdfFrame');
  assert.match(toggle, /this\.currentPdfViewerKey = null/);
  assert.match(toggle, /this\.resetPdfFrame\(\)/);
  assert.match(reset, /this\.pdfFrame\.remove\(\)/);
  assert.match(reset, /this\.pdfFrame = this\.createPdfFrame\(\)/);
});

test('preview failures preserve password document transport and viewer categories', () => {
  assert.equal(classifyPdfPreviewError({name: 'PasswordException'}).category, 'password');
  assert.equal(classifyPdfPreviewError({name: 'InvalidPDFException'}).category, 'document');
  assert.equal(classifyPdfPreviewError({name: 'MissingPDFException'}).category, 'unavailable');
  assert.equal(classifyPdfPreviewError({name: 'UnexpectedResponseException'}).category, 'transport');
  assert.equal(classifyPdfPreviewError(new Error('PDF block request failed (500)')).category, 'transport');
  assert.equal(classifyPdfPreviewError(new ReferenceError('missing viewer symbol')).category, 'viewer');
  for (const error of [
    {name: 'PasswordException'},
    {name: 'InvalidPDFException'},
    {name: 'MissingPDFException'},
    {name: 'UnexpectedResponseException'},
    new ReferenceError('missing viewer symbol'),
  ]) {
    assert.doesNotMatch(classifyPdfPreviewError(error).message, /opened safely/i);
  }
  assert.ok((viewerSource.match(/reportPdfPreviewFailure\(error\)/g) ?? []).length >= 2,
    'startup and asynchronous range failures must share the same classifier');
});

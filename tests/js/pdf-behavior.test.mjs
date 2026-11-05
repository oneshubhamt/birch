import assert from 'node:assert/strict';
import test from 'node:test';

globalThis.__RUSTNOTE_PDF_RUNTIME_PROFILE__ = 'legacy';
globalThis.__RUSTNOTE_PDF_RUNTIME_BASE_URL__ = '/rustnotepdf/runtime/legacy';

const {FenwickTree} = await import('../../pdf-src/virtualization.mjs');
const {appendSearchFragment, searchRangeForOffsets, textLayerSearchMap} = await import('../../pdf-src/search.mjs');
const {
  annotationPdfRect,
  boxSizeFor,
  cloneAnnotations,
  colorFor,
  exportVectorPoints,
  normalizedPoint,
  portableAnnotationValue,
} = await import('../../pdf-src/annotations.mjs');
const {ENABLE_PDF_SCRIPTING} = await import('../../pdf-src/config.mjs');

test('Fenwick page-height index maintains sums and lower bounds after updates', () => {
  const tree = new FenwickTree(5);
  [100, 200, 50, 400, 250].forEach((value, index) => tree.set(index + 1, value));
  assert.equal(tree.total(), 1000);
  assert.equal(tree.prefixSum(3), 350);
  assert.equal(tree.lowerBound(1), 1);
  assert.equal(tree.lowerBound(100), 1);
  assert.equal(tree.lowerBound(101), 2);
  assert.equal(tree.lowerBound(351), 4);
  tree.set(2, 50);
  assert.equal(tree.total(), 850);
  assert.equal(tree.lowerBound(151), 3);
});

test('PDF text search map preserves readable separation and exact DOM offsets', () => {
  globalThis.Node = {TEXT_NODE: 3};
  const first = {nodeType: 3, length: 5};
  const second = {nodeType: 3, length: 5};
  const view = {textLayerObject: {
    textDivs: [{firstChild: first}, {firstChild: second}],
    textContentItemsStr: ['Alpha', 'Beta '],
  }};
  const map = textLayerSearchMap(view);
  assert.equal(map.text, 'Alpha Beta ');
  assert.equal(map.segments.length, 2);

  const rangeCalls = [];
  globalThis.document = {createRange() {
    return {
      setStart(node, offset) { rangeCalls.push(['start', node, offset]); },
      setEnd(node, offset) { rangeCalls.push(['end', node, offset]); },
    };
  }};
  assert.ok(searchRangeForOffsets(map, 1, 8));
  assert.deepEqual(rangeCalls.map(call => [call[0], call[2]]), [['start', 1], ['end', 2]]);

  const state = {text: '', segments: []};
  appendSearchFragment(state, 'One');
  appendSearchFragment(state, 'Two');
  assert.equal(state.text, 'One Two');
});

test('annotation geometry is clamped and portable export rejects empty text', () => {
  const viewport = {
    width: 100,
    height: 200,
    rotation: 0,
    convertToPdfPoint(x, y) { return [x, 200 - y]; },
  };
  assert.deepEqual(annotationPdfRect(viewport, [-1, 0.5, 2, 1]), [0, 0, 100, 100]);
  assert.equal(portableAnnotationValue({kind: 'text', text: '   ', color: 'red'}, 0, viewport), null);

  const exported = portableAnnotationValue({
    kind: 'text', text: 'Note', color: 'blue', x: 0.1, y: 0.2, width: 0.3, height: 0.1, size: 'large',
  }, 2, viewport);
  assert.equal(exported.pageIndex, 2);
  assert.equal(exported.annotationType, 3);
  assert.equal(exported.fontSize, 19);
  assert.deepEqual(exported.rect, [10, 140, 40, 160]);
});

test('vector annotation behavior and session copies are deterministic', () => {
  assert.deepEqual(exportVectorPoints({kind: 'shape', shape: 'rectangle', points: [[0.1, 0.2], [0.4, 0.6]]}), [
    [[0.1, 0.2], [0.4, 0.2], [0.4, 0.6], [0.1, 0.6], [0.1, 0.2]],
  ]);
  const source = [{kind: 'note', text: 'A'}];
  const clone = cloneAnnotations(source);
  clone[0].text = 'B';
  assert.equal(source[0].text, 'A');
  assert.equal(colorFor({color: 'unknown'}), '#ffd84d');
  assert.deepEqual(boxSizeFor({medium: [1, 2], large: [3, 4]}, 'missing'), [1, 2]);

  const point = normalizedPoint({shell: {getBoundingClientRect: () => ({left: 10, top: 20, width: 100, height: 200})}}, 500, -50);
  assert.deepEqual(point, [1, 0]);
  assert.equal(ENABLE_PDF_SCRIPTING, false, 'PDF scripting must remain disabled');
});

const {PreviewRasterCache} = await import('../../pdf-src/preview_cache.mjs');
const {
  annotationCounterLabel,
  annotationSummary,
  annotationWindowBounds,
  initialAnnotationIndex,
  thumbnailWindowBounds,
} = await import('../../pdf-src/sidebar.mjs');
const {
  nextRenderAction,
  pageRenderPriority,
  renderTargetPageNumbers,
} = await import('../../pdf-src/render_scheduler.mjs');

test('preview raster cache owns LRU accounting, protection, and hard-pressure eviction', () => {
  const cache = new PreviewRasterCache({
    hardLimitBytes: 200,
    minimumBudgetBytes: 50,
    canvasMinimumOperatingBytes: 100,
    canvasHardLimitBytes: 300,
  });
  const first = {width: 5, height: 5};
  const second = {width: 5, height: 5};
  cache.store(1, {canvas: first, baseWidth: 100, baseHeight: 100});
  cache.store(2, {canvas: second, baseWidth: 100, baseHeight: 100});
  assert.equal(cache.bytes, 200);
  assert.equal(cache.get(1).canvas, first, 'cache reads must touch LRU order');

  cache.evictToBudget({
    retainedCanvasBytes: 100,
    auxiliaryRasterBytes: 0,
    protectedPages: new Set([2]),
    forcedBudget: 100,
  });
  assert.equal(cache.has(2), true, 'protected preview survives ordinary pressure');
  assert.equal(cache.has(1), false);
  assert.equal(first.width, 1, 'evicted canvas backing store is released');

  cache.evictToBudget({
    retainedCanvasBytes: 250,
    auxiliaryRasterBytes: 0,
    protectedPages: new Set([2]),
    forcedBudget: 0,
  });
  assert.equal(cache.size, 0, 'hard pressure may evict protected optional previews');
  assert.equal(second.height, 1);
});

test('sidebar virtualization and annotation navigation helpers preserve window semantics', () => {
  assert.deepEqual(thumbnailWindowBounds({
    scrollTop: 200,
    viewportHeight: 300,
    pageCount: 20,
    rowHeight: 100,
    overscanRows: 2,
  }), {visibleStart: 3, visibleEnd: 5, start: 1, end: 7, center: 4});

  assert.deepEqual(annotationWindowBounds({
    scrollTop: 200,
    viewportHeight: 300,
    itemCount: 20,
    rowHeight: 100,
    overscanRows: 2,
  }), {start: 0, end: 8}, 'annotation window keeps the original extra trailing row');

  const tools = new Set(['highlight', 'underline', 'strike']);
  const summary = annotationSummary(4, {id: 'ann_1', kind: 'highlight', rects: [[1.2, -0.2, 1, 1]], text: '  A   note  '}, tools);
  assert.deepEqual(summary, {id: 'ann_1', kind: 'highlight', pageNumber: 4, x: 1, y: 0, text: 'A note'});
  const items = [{pageNumber: 1}, {pageNumber: 4}, {pageNumber: 8}];
  assert.equal(initialAnnotationIndex({items, currentPage: 3, direction: 1}), 1);
  assert.equal(initialAnnotationIndex({items, currentPage: 7, direction: -1}), 1);
  assert.equal(annotationCounterLabel({count: 8, activeIndex: 2, truncated: true}), '3 / 8+');
});

test('render target planning respects reading direction and the adaptive memory ceiling', () => {
  const bytes = new Map([[4, 100], [5, 100], [6, 100], [3, 100], [7, 100]]);
  const targets = renderTargetPageNumbers({
    pageCount: 10,
    visiblePageNumbers: [4, 5],
    currentPage: 5,
    scrollDirection: 1,
    readAheadForward: 2,
    estimatePageBytes: page => bytes.get(page) ?? 100,
    minimumOperatingBytes: 300,
    hardLimitBytes: 400,
  });
  assert.deepEqual([...targets].sort((a, b) => a - b), [3, 4, 5, 6]);
  assert.ok(pageRenderPriority({
    pageNumber: 5,
    visiblePages: [{pageNumber: 5, percent: 0.8}],
    currentPage: 5,
    scrollDirection: 1,
    readAheadForward: 2,
  }) < pageRenderPriority({
    pageNumber: 6,
    visiblePages: [{pageNumber: 5, percent: 0.8}],
    currentPage: 5,
    scrollDirection: 1,
    readAheadForward: 2,
  }));
});

test('render scheduler evaluates the single raster lane in strict priority order', () => {
  const calls = [];
  const action = nextRenderAction({
    takeVisiblePage: () => { calls.push('visible'); return null; },
    visiblePagesNeedRaster: () => { calls.push('blank'); return false; },
    takeCriticalDetail: () => { calls.push('critical'); return null; },
    takeNextReadingPage: () => { calls.push('next'); return {id: 'next'}; },
    takeReadAheadPage: () => { calls.push('ahead'); return {id: 'ahead'}; },
    takeSettledDetail: () => { calls.push('detail'); return {id: 'detail'}; },
    takeVisiblePreview: () => { calls.push('preview'); return {id: 'preview'}; },
  });
  assert.deepEqual(action, {kind: 'page', item: {id: 'next'}});
  assert.deepEqual(calls, ['visible', 'blank', 'critical', 'next'], 'lower-priority work must be lazy');

  const blockedCalls = [];
  assert.equal(nextRenderAction({
    takeVisiblePage: () => null,
    visiblePagesNeedRaster: () => true,
    takeCriticalDetail: () => { blockedCalls.push('critical'); return null; },
    takeNextReadingPage: () => { blockedCalls.push('next'); return null; },
    takeReadAheadPage: () => null,
    takeSettledDetail: () => null,
    takeVisiblePreview: () => null,
  }), null);
  assert.deepEqual(blockedCalls, []);
});

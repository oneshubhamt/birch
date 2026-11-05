// WebKit-targeted bundled PDF.js runtime. The build pipeline resolves these
// private specifiers to one selected, pinned PDF.js display/viewer profile and
// bundles them with Rustnote's shared viewer architecture so WKWebView never
// parses raw upstream modules directly.
import * as pdfjsLib from 'rustnote-pdfjs-display';
import {RenderingStates} from 'rustnote-pdfjs-viewer';

import * as config from './config.mjs';
import {
  annotationId,
  boxSizeFor,
  cloneAnnotations,
  colorFor,
  normalizedPoint,
  portableAnnotationValue,
} from './annotations.mjs';
import {
  PDFJS_TEXT_LAYER_DISABLED,
  RustnotePdfPageView,
  pdfjsPageEventBus,
  pdfjsPageL10n,
  pdfjsPageLayerProperties,
} from './rendering.mjs';
import {appendSearchFragment, searchRangeForOffsets, textLayerSearchMap} from './search.mjs';
import {pdfDocumentMetadata, preparePdfDocumentTransport} from './transport.mjs';
import {elements, setStatus} from './ui.mjs';
import {PreviewRasterCache, canvasBackingBytes} from './preview_cache.mjs';
import {classifyPdfPreviewError} from './errors.mjs';
import {
  annotationCounterLabel,
  annotationSummary as summarizeSidebarAnnotation,
  annotationWindowBounds as calculateAnnotationWindowBounds,
  createAnnotationRowElement,
  createThumbnailCanvas,
  createThumbnailRowElement,
  defaultSidebarOpenForPageCount,
  initialAnnotationIndex as findInitialAnnotationIndex,
  previewDimensions as calculatePreviewDimensions,
  renderOutlineItems,
  thumbnailWindowBounds as calculateThumbnailWindowBounds,
} from './sidebar.mjs';
import {
  memoryOperatingBudget as calculateMemoryOperatingBudget,
  medianPositive,
  nextRenderAction,
  pageRenderPriority as calculatePageRenderPriority,
  previewJobPriority as calculatePreviewJobPriority,
  readingWindow as calculateReadingWindow,
  renderTargetPageNumbers as planRenderTargetPageNumbers,
} from './render_scheduler.mjs';
import {FenwickTree, clampPageNumber} from './virtualization.mjs';

const {
  ADAPTIVE_RETENTION_MAX_DISTANCE,
  ANNOTATION_OVERSCAN_ROWS,
  ANNOTATION_ROW_HEIGHT,
  CANVAS_MEMORY_HARD_LIMIT_BYTES,
  CANVAS_MEMORY_MIN_OPERATING_BYTES,
  ENABLE_PDF_SCRIPTING,
  MAX_ANNOTATION_INDEX_ENTRIES,
  MAX_MOUNTED_PAGE_VIEWS,
  MAX_OUTLINE_ITEMS,
  MAX_PREVIEW_PIXELS,
  MAX_RENDER_CANDIDATE_DISTANCE,
  MAX_SEARCH_HIGHLIGHTS_PER_PAGE,
  MAX_SEARCH_HIGHLIGHT_RECTS_PER_PAGE,
  MAX_SEARCH_MATCHES,
  MAX_SEARCH_TEXT_BYTES,
  MAX_SEARCH_TEXT_CHARS_PER_PAGE,
  MAX_SCALE,
  MIN_SCALE,
  NOTE_SIZES,
  PAGE_DOM_MIN_OVERSCAN_PX,
  PAGE_DOM_OVERSCAN_VIEWPORTS,
  PAGE_GAP_PX,
  PAGE_READ_AHEAD_FORWARD,
  PDFJS_BASE_CANVAS_MAX_PIXELS,
  PDFJS_IMAGE_CANVAS_MAX_AREA_BYTES,
  PDFJS_MAX_SOURCE_IMAGE_PIXELS,
  PDFJS_MIN_CANVAS_UPDATE_MS,
  PDFJS_STANDARD_FONT_DATA_URL,
  PDFJS_WASM_URL,
  PDFJS_WORKER_URL,
  PDF_RUNTIME_PROFILE,
  PREVIEW_CACHE_HARD_LIMIT_BYTES,
  PREVIEW_CACHE_MIN_BUDGET_BYTES,
  PREVIEW_MAX_HEIGHT,
  PREVIEW_WIDTH,
  RASTER_BYTES_PER_PIXEL,
  RESIZE_DEBOUNCE_MS,
  RESTRICTED_PAGE_BASE_AREA_FACTOR,
  SCALE_STEP,
  SCROLL_SETTLE_MS,
  STROKE_WIDTHS,
  TEXT_ANNOTATION_SAVE_DELAY_MS,
  TEXT_BOX_SIZES,
  TEXT_FONT_SIZES,
  TEXT_MARK_TOOLS,
  THUMBNAIL_MAX_DISPLAY_HEIGHT,
  THUMBNAIL_MAX_DISPLAY_WIDTH,
  THUMBNAIL_OVERSCAN_ROWS,
  THUMBNAIL_ROW_HEIGHT,
} = config;

const {
  AnnotationMode,
  GlobalWorkerOptions,
  PDFDataRangeTransport,
  PixelsPerInch,
  TextLayer,
  getDocument,
} = pdfjsLib;

const PDFJS_EXPECTED_VERSION = __RUSTNOTE_PDFJS_EXPECTED_VERSION__;
if (pdfjsLib.version !== PDFJS_EXPECTED_VERSION) {
  throw new Error(`PDF.js runtime version mismatch: expected ${PDFJS_EXPECTED_VERSION}, got ${pdfjsLib.version}`);
}
GlobalWorkerOptions.workerSrc = PDFJS_WORKER_URL;

const viewerParameters = new URL(import.meta.url).searchParams;
const READ_ONLY = viewerParameters.get('readonly') === '1';



let pdf = null;
let currentPage = 1;
let scale = 1;
let fitMode = 'width';
let activeTool = 'select';
let attachmentId = '';
let currentVector = null;
let textMarkColor = 'yellow';
let noteColor = 'yellow';
let noteSize = 'medium';
let penColor = 'red';
let penWidth = 'medium';
let shapeType = 'rectangle';
let shapeColor = 'red';
let shapeWidth = 'medium';
let textColor = 'black';
let textSize = 'medium';
let searchSerial = 0;
let searchMatches = [];
let searchMatchIndex = -1;
let searchTextBytes = 0;
let searchResultsTruncated = false;
let searchRunning = false;
let lastSearchQuery = '';
let renderGeneration = 0;
let activePageRenders = 0;
let renderQueue = [];
let scrollFrame = 0;
let resizeTimer = 0;
let hostRenderedNotified = false;
let defaultPageMetrics = {width: 612, height: 792};
let sidebarOpen = false;
let sidebarMode = 'thumbnails';
let thumbnailFrame = 0;
let previewGeneration = 0;
let previewRenderQueue = [];
let activePreviewRender = null;
let previewRenderSerial = 0;
let retainedCanvasBytes = 0;
let canvasOperatingBudgetBytes = CANVAS_MEMORY_MIN_OPERATING_BYTES;
let visiblePageSnapshot = [];
let activeDetailRender = null;
let thumbnailBrowseCenter = 1;
let outlineLoaded = false;
let outlineLoadPromise = null;
let annotationIndex = [];
let annotationIndexLoaded = false;
let annotationIndexLoading = null;
let annotationIndexController = null;
let annotationIndexSerial = 0;
let annotationIndexTruncated = false;
let annotationIndexError = false;
let activeAnnotationIndex = -1;
let annotationFrame = 0;
let selectionActionFrame = 0;
let fitOperationSerial = 0;
let scrollDirection = 1;
let lastScrollY = window.scrollY;
let scrollSettled = true;
let scrollSettleTimer = 0;
let exportBusy = false;
let exportRequestSerial = 0;
let focusActive = false;
let activePdfRangeTransport = null;
let pdfSessionReleaseUrl = '';
let geometryScrollCompensationY = null;
let pageHeightIndex = null;
let maxBasePageWidth = 1;

const pageGeometries = [null];
const mountedPageViews = new Map();
const retainedPageNumbers = new Set();
const activePageRenderViews = new Set();
const thumbnailRows = new Map();
const previewQueuedPages = new Set();
const previewCache = new PreviewRasterCache({
  hardLimitBytes: PREVIEW_CACHE_HARD_LIMIT_BYTES,
  minimumBudgetBytes: PREVIEW_CACHE_MIN_BUDGET_BYTES,
  canvasMinimumOperatingBytes: CANVAS_MEMORY_MIN_OPERATING_BYTES,
  canvasHardLimitBytes: CANVAS_MEMORY_HARD_LIMIT_BYTES,
});
const previewFailedPages = new Set();
const annotationRows = new Map();
const annotationPages = new Map();
const annotationLoads = new Map();
const annotationSaveChains = new Map();
const textAnnotationSaveTimers = new Map();
const pageTextCache = new Map();
const undoStack = [];
const redoStack = [];


function attachmentIdFromViewerParameters() {
  const value = viewerParameters.get('attachment') ?? '';
  return value.startsWith('att_') && value.length <= 128
    && /^[A-Za-z0-9_-]+$/.test(value) ? value : '';
}

function notifyHost(stage, detail = '') {
  try {
    window.parent?.postMessage({
      type: 'rustnote-pdf-viewer',
      attachmentId,
      stage,
      detail: String(detail ?? '').slice(0, 240),
    }, '*');
  } catch {
    // Host status reporting is best-effort and must never affect PDF rendering.
  }
}

function reportPdfPreviewFailure(error) {
  const failure = classifyPdfPreviewError(error);
  setStatus(failure.message, true);
  const detail = String(error?.message ?? failure.message);
  notifyHost('error', `${failure.category}: ${detail}`);
  return failure;
}



function clearSearchErrorStatus() {
  if (elements.status.textContent === 'Search could not be completed for this PDF.') setStatus('');
}

function clearRenderedSearchHighlights() {
  if (!pdf) return;
  const start = Math.max(1, currentPage - MAX_RENDER_CANDIDATE_DISTANCE);
  const end = Math.min(pdf.numPages, currentPage + MAX_RENDER_CANDIDATE_DISTANCE);
  for (let pageNumber = start; pageNumber <= end; pageNumber += 1) {
    pageViewForNumber(pageNumber)?.searchLayer?.replaceChildren();
  }
}

function cancelSearchWork(discardPartial = false) {
  searchSerial += 1;
  searchRunning = false;
  if (discardPartial) {
    searchMatches = [];
    searchMatchIndex = -1;
    searchResultsTruncated = false;
    elements.searchCount.textContent = '';
    elements.searchCount.title = '';
  }
}

function setSearchOpen(open, focus = false) {
  const nextOpen = Boolean(open);
  elements.searchPanel.hidden = !nextOpen;
  elements.searchToggle.classList.toggle('is-active', nextOpen);
  elements.searchToggle.setAttribute('aria-expanded', String(nextOpen));
  if (!nextOpen) {
    if (searchRunning) cancelSearchWork(true);
    clearRenderedSearchHighlights();
  } else if (searchMatches.length > 0 && lastSearchQuery) {
    refreshRenderedSearchHighlights();
  }
  requestAnimationFrame(() => {
    syncViewerChromeGeometry();
    if (pdf && fitMode === 'page') void fitPage();
    else scheduleViewportUpdate();
    if (nextOpen && focus) {
      elements.search.focus();
      elements.search.select();
    }
  });
}

function setMarkupOpen(open) {
  const nextOpen = !READ_ONLY && Boolean(open);
  elements.markupToolbar.hidden = !nextOpen;
  elements.markupToggle.classList.toggle('is-active', nextOpen);
  elements.markupToggle.setAttribute('aria-expanded', String(nextOpen));
  if (!nextOpen && activeTool !== 'select') setTool('select');
  requestAnimationFrame(() => {
    syncViewerChromeGeometry();
    if (pdf && fitMode === 'page') void fitPage();
    else scheduleViewportUpdate();
  });
}

function sidebarThumbnailsActive() {
  return Boolean(pdf) && sidebarOpen && sidebarMode === 'thumbnails';
}

function sidebarAnnotationsActive() {
  return Boolean(pdf) && sidebarOpen && sidebarMode === 'annotations';
}

function syncViewerChromeGeometry() {
  const toolbarHeight = Math.max(0, elements.toolbar?.getBoundingClientRect().height ?? 0);
  const chromeHeight = toolbarBottom();
  document.documentElement.style.setProperty('--viewer-toolbar-height', `${toolbarHeight}px`);
  document.documentElement.style.setProperty('--viewer-chrome-height', `${chromeHeight}px`);
}

function clearThumbnailVirtualWindow() {
  if (thumbnailFrame) cancelAnimationFrame(thumbnailFrame);
  thumbnailFrame = 0;
  previewRenderQueue = previewRenderQueue.filter(item => {
    if (item.kind !== 'visible' && item.kind !== 'overscan') return true;
    previewQueuedPages.delete(item.pageNumber);
    item.resolve(previewCache.has(item.pageNumber));
    return false;
  });
  if (activePreviewRender
      && (activePreviewRender.kind === 'visible' || activePreviewRender.kind === 'overscan')) {
    cancelActivePreviewRender();
  }
  thumbnailRows.clear();
  elements.thumbnailContent.replaceChildren();
}

function setSidebarOpen(open, {refit = true} = {}) {
  const nextOpen = Boolean(pdf) && Boolean(open);
  if (sidebarOpen === nextOpen) {
    elements.sidebarToggle.disabled = !pdf;
    return;
  }
  sidebarOpen = nextOpen;
  elements.sidebar.hidden = !nextOpen;
  elements.sidebarToggle.classList.toggle('is-active', nextOpen);
  elements.sidebarToggle.setAttribute('aria-expanded', String(nextOpen));
  elements.sidebarToggle.setAttribute('aria-label', nextOpen ? 'Hide sidebar' : 'Show sidebar');
  elements.sidebarToggle.title = nextOpen ? 'Hide sidebar' : 'Show sidebar';
  if (!nextOpen) {
    clearThumbnailVirtualWindow();
    cancelAnnotationIndexLoad();
    clearAnnotationVirtualWindow();
  } else {
    syncViewerChromeGeometry();
    if (sidebarMode === 'thumbnails') scheduleThumbnailVirtualization();
    else if (sidebarMode === 'outline') void ensureOutlineLoaded();
    else void ensureAnnotationIndexLoaded();
  }
  requestAnimationFrame(() => {
    syncViewerChromeGeometry();
    if (!refit) {
      scheduleViewportUpdate();
      return;
    }
    if (pdf && fitMode) void refitCurrentMode();
    else scheduleViewportUpdate();
  });
}

function setSidebarMode(mode) {
  const nextMode = mode === 'outline' || mode === 'annotations' ? mode : 'thumbnails';
  if (sidebarMode === nextMode) return;
  sidebarMode = nextMode;
  const thumbnailsActive = nextMode === 'thumbnails';
  const outlineActive = nextMode === 'outline';
  const annotationsActive = nextMode === 'annotations';
  elements.sidebarThumbnailsTab.classList.toggle('is-active', thumbnailsActive);
  elements.sidebarThumbnailsTab.setAttribute('aria-selected', String(thumbnailsActive));
  elements.sidebarOutlineTab.classList.toggle('is-active', outlineActive);
  elements.sidebarOutlineTab.setAttribute('aria-selected', String(outlineActive));
  elements.sidebarAnnotationsTab.classList.toggle('is-active', annotationsActive);
  elements.sidebarAnnotationsTab.setAttribute('aria-selected', String(annotationsActive));
  elements.thumbnailPanel.hidden = !thumbnailsActive;
  elements.outlinePanel.hidden = !outlineActive;
  elements.annotationPanel.hidden = !annotationsActive;
  clearThumbnailVirtualWindow();
  if (!annotationsActive) cancelAnnotationIndexLoad();
  clearAnnotationVirtualWindow();
  if (!sidebarOpen) return;
  if (thumbnailsActive) scheduleThumbnailVirtualization();
  else if (outlineActive) void ensureOutlineLoaded();
  else void ensureAnnotationIndexLoaded();
}

function errorDetail(error) {
  const name = typeof error?.name === 'string' && error.name ? error.name : 'Error';
  const message = typeof error?.message === 'string' && error.message
    ? error.message
    : String(error ?? 'Unknown error');
  return `${name}: ${message}`.replace(/\s+/g, ' ').slice(0, 220);
}


function pageViewForNumber(pageNumber) {
  return mountedPageViews.get(pageNumber) ?? null;
}

function pageGeometryForNumber(pageNumber) {
  return pageGeometries[pageNumber] ?? null;
}

function geometryTopForNumber(pageNumber) {
  if (!pageHeightIndex || pageNumber <= 1) return 0;
  return pageHeightIndex.prefixSum(pageNumber - 1);
}

function pagesDocumentTop() {
  return window.scrollY + elements.pages.getBoundingClientRect().top;
}

function pageNumberAtLocalY(localY) {
  if (!pdf || !pageHeightIndex || pageGeometries.length <= 1) return currentPage;
  const y = Math.max(0, Number(localY) || 0);
  const candidate = pageHeightIndex.lowerBound(y + 0.0001);
  const geometry = pageGeometryForNumber(candidate);
  if (!geometry) return Math.max(1, Math.min(pdf.numPages, candidate || 1));
  const top = geometryTopForNumber(candidate);
  const next = pageGeometryForNumber(candidate + 1);
  if (next && y > top + geometry.height) {
    const previousDistance = y - (top + geometry.height);
    const nextDistance = geometryTopForNumber(candidate + 1) - y;
    if (nextDistance < previousDistance) return candidate + 1;
  }
  return Math.max(1, Math.min(pdf.numPages, candidate));
}

function pageNumberAtDocumentY(documentY) {
  return pageNumberAtLocalY(documentY - pagesDocumentTop());
}

function captureGeometryAnchor() {
  if (!pdf || elements.canvasShell.hidden || pageGeometries.length <= 1) return null;
  const documentY = window.scrollY + viewportContentCenterY();
  const pageNumber = pageNumberAtDocumentY(documentY);
  const geometry = pageGeometryForNumber(pageNumber);
  if (!geometry) return null;
  const localY = documentY - pagesDocumentTop();
  const top = geometryTopForNumber(pageNumber);
  return {
    pageNumber,
    yRatio: Math.max(0, Math.min(1, (localY - top) / Math.max(1, geometry.height))),
    documentY,
  };
}

function restoreGeometryAnchor(anchor) {
  if (!anchor) return;
  const geometry = pageGeometryForNumber(anchor.pageNumber);
  if (!geometry) return;
  const nextDocumentY = pagesDocumentTop()
    + geometryTopForNumber(anchor.pageNumber)
    + geometry.height * anchor.yRatio;
  const deltaY = nextDocumentY - anchor.documentY;
  if (Number.isFinite(deltaY) && Math.abs(deltaY) > 0.5) {
    window.scrollBy(0, deltaY);
    geometryScrollCompensationY = window.scrollY;
  }
}

function updateGeometryContainerSize() {
  const totalHeight = pageHeightIndex?.total() ?? 1;
  elements.pages.style.height = `${Math.max(1, totalHeight)}px`;
  elements.pages.style.width = `${Math.max(1, maxBasePageWidth * scale)}px`;
}

function rebuildPageGeometryIndex(anchor = null) {
  if (!pdf || pageGeometries.length <= 1) return;
  pageHeightIndex = new FenwickTree(pdf.numPages);
  maxBasePageWidth = 1;
  for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
    const geometry = pageGeometryForNumber(pageNumber);
    if (!geometry) continue;
    geometry.width = Math.max(1, geometry.baseWidth * scale);
    geometry.height = Math.max(1, geometry.baseHeight * scale);
    pageHeightIndex.set(
      pageNumber,
      geometry.height + (pageNumber < pdf.numPages ? PAGE_GAP_PX : 0),
    );
    maxBasePageWidth = Math.max(maxBasePageWidth, geometry.baseWidth);
  }
  updateGeometryContainerSize();
  for (const view of mountedPageViews.values()) view.updateSize();
  restoreGeometryAnchor(anchor);
}

function updatePageGeometryMetrics(pageNumber, width, height) {
  const geometry = pageGeometryForNumber(pageNumber);
  if (!geometry || !Number.isFinite(width) || width <= 0 || !Number.isFinite(height) || height <= 0) return;
  const changed = Math.abs(geometry.baseWidth - width) > 0.01 || Math.abs(geometry.baseHeight - height) > 0.01;
  if (!changed && geometry.known) return;
  const anchor = changed ? captureGeometryAnchor() : null;
  geometry.baseWidth = width;
  geometry.baseHeight = height;
  geometry.width = Math.max(1, width * scale);
  geometry.height = Math.max(1, height * scale);
  geometry.known = true;
  maxBasePageWidth = Math.max(maxBasePageWidth, width);
  pageHeightIndex?.set(
    pageNumber,
    geometry.height + (pageNumber < pdf.numPages ? PAGE_GAP_PX : 0),
  );
  updateGeometryContainerSize();
  // Only mounted pages need their absolute positions updated. The Fenwick index
  // already moved every later numerical page offset in O(log N).
  for (const view of mountedPageViews.values()) view.updateSize();
  restoreGeometryAnchor(anchor);
}

function geometryWindowForViewport(overscanPixels = 0) {
  if (!pdf || !pageHeightIndex || pageGeometries.length <= 1) return {start: 1, end: 0};
  const pagesTop = pagesDocumentTop();
  const viewportTop = window.scrollY + toolbarBottom();
  const viewportBottom = window.scrollY + window.innerHeight;
  const localTop = Math.max(0, viewportTop - pagesTop - Math.max(0, overscanPixels));
  const localBottom = Math.max(localTop, viewportBottom - pagesTop + Math.max(0, overscanPixels));
  let start = pageNumberAtLocalY(localTop);
  while (start > 1) {
    const previous = pageGeometryForNumber(start - 1);
    if (!previous || geometryTopForNumber(start - 1) + previous.height < localTop) break;
    start -= 1;
  }
  let end = Math.max(start, pageNumberAtLocalY(localBottom));
  while (end < pdf.numPages) {
    const next = pageGeometryForNumber(end + 1);
    if (!next || geometryTopForNumber(end + 1) > localBottom) break;
    end += 1;
  }
  return {start: Math.max(1, start), end: Math.min(pdf.numPages, end)};
}

function activeInteractionPageNumbers() {
  const protectedPages = new Set();
  if (currentVector?.pageNumber) protectedPages.add(currentVector.pageNumber);
  const focusedShell = document.activeElement?.closest?.('.page-shell');
  if (focusedShell?.dataset.pageNumber) protectedPages.add(Number(focusedShell.dataset.pageNumber));

  const selection = document.getSelection();
  if (selection && selection.rangeCount > 0 && !selection.isCollapsed) {
    const range = selection.getRangeAt(0);
    const endpointNodes = [range.startContainer, range.endContainer];
    for (const node of endpointNodes) {
      const element = node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement;
      const shell = element?.closest?.('.page-shell');
      if (shell?.dataset.pageNumber) protectedPages.add(Number(shell.dataset.pageNumber));
    }
    // A cross-page selection often has the whole viewer as common ancestor.
    // Intersect the bounded mounted set instead; this is O(MAX_MOUNTED_PAGE_VIEWS)
    // and protects every page actually participating in the live selection.
    for (const [pageNumber, view] of mountedPageViews) {
      try {
        if (range.intersectsNode(view.shell)) protectedPages.add(pageNumber);
      } catch {
        // Selection protection is best-effort for detached/transient DOM nodes.
      }
    }
  }
  return protectedPages;
}

function mountedPagePriority(pageNumber) {
  const geometry = pageGeometryForNumber(pageNumber);
  if (!geometry) return Number.POSITIVE_INFINITY;
  const pageCenter = geometryTopForNumber(pageNumber) + geometry.height / 2;
  const viewportCenter = window.scrollY + viewportContentCenterY() - pagesDocumentTop();
  return Math.abs(pageCenter - viewportCenter);
}

function evictOneMountedPageFor(incomingPageNumber) {
  if (mountedPageViews.size < MAX_MOUNTED_PAGE_VIEWS) return true;
  const protectedPages = activeInteractionPageNumbers();
  protectedPages.add(currentPage);
  for (const view of activePageRenderViews) protectedPages.add(view.pageNumber);
  if (activeDetailRender?.view) protectedPages.add(activeDetailRender.view.pageNumber);
  protectedPages.add(incomingPageNumber);
  const candidates = [...mountedPageViews.keys()]
    .filter(pageNumber => !protectedPages.has(pageNumber))
    .sort((a, b) => mountedPagePriority(b) - mountedPagePriority(a));
  if (candidates.length === 0) return false;
  unmountPageView(candidates[0]);
  return true;
}

function mountPageView(pageNumber) {
  if (!pdf || pageNumber < 1 || pageNumber > pdf.numPages) return null;
  const existing = pageViewForNumber(pageNumber);
  if (existing) return existing;
  const geometry = pageGeometryForNumber(pageNumber);
  if (!geometry) return null;
  if (!evictOneMountedPageFor(pageNumber)) return null;
  const view = new PageView(pageNumber, geometry);
  mountedPageViews.set(pageNumber, view);
  elements.pages.append(view.slot);
  view.shell.classList.toggle('is-current', pageNumber === currentPage);
  return view;
}

function removeQueuedPageRenderForView(view) {
  renderQueue = renderQueue.filter(item => {
    if (item.view !== view) return true;
    if (view.renderJobId === item.jobId) {
      view.renderPromise = null;
      view.queuedGeneration = -1;
    }
    item.resolve(false);
    return false;
  });
}

function unmountPageView(pageNumber) {
  const view = pageViewForNumber(pageNumber);
  if (!view) return;
  removeQueuedPageRenderForView(view);
  view.releaseLayers();
  view.slot.remove();
  mountedPageViews.delete(pageNumber);
}

function appendVirtualCandidate(order, seen, pageNumber) {
  if (!pdf || pageNumber < 1 || pageNumber > pdf.numPages || seen.has(pageNumber)) return;
  seen.add(pageNumber);
  order.push(pageNumber);
}

function desiredVirtualPageNumbers(extraPages = []) {
  if (!pdf) return new Set();
  const order = [];
  const seen = new Set();
  // Never virtualize a page out from under live input or an active renderer.
  // These sets are tiny in normal use and take precedence over the pathological
  // case where more than MAX_MOUNTED_PAGE_VIEWS miniature pages are physically
  // visible at once. Ordinary documents still mount every actually visible page.
  for (const pageNumber of activeInteractionPageNumbers()) appendVirtualCandidate(order, seen, pageNumber);
  for (const view of activePageRenderViews) appendVirtualCandidate(order, seen, view.pageNumber);
  if (activeDetailRender?.view) appendVirtualCandidate(order, seen, activeDetailRender.view.pageNumber);

  const clampedCurrent = Math.max(1, Math.min(pdf.numPages, currentPage));
  appendVirtualCandidate(order, seen, clampedCurrent);

  const visibleWindow = geometryWindowForViewport(0);
  const visibleCandidates = [];
  for (let pageNumber = visibleWindow.start; pageNumber <= visibleWindow.end; pageNumber += 1) {
    visibleCandidates.push(pageNumber);
  }
  visibleCandidates.sort((a, b) => mountedPagePriority(a) - mountedPagePriority(b));
  for (const pageNumber of visibleCandidates) appendVirtualCandidate(order, seen, pageNumber);
  for (const pageNumber of extraPages) appendVirtualCandidate(order, seen, pageNumber);
  for (const offset of [scrollDirection, -scrollDirection, scrollDirection * 2]) {
    appendVirtualCandidate(order, seen, clampedCurrent + offset);
  }

  const overscan = Math.max(PAGE_DOM_MIN_OVERSCAN_PX, window.innerHeight * PAGE_DOM_OVERSCAN_VIEWPORTS);
  const overscanWindow = geometryWindowForViewport(overscan);
  const overscanCandidates = [];
  for (let pageNumber = overscanWindow.start; pageNumber <= overscanWindow.end; pageNumber += 1) {
    if (!seen.has(pageNumber)) overscanCandidates.push(pageNumber);
  }
  overscanCandidates.sort((a, b) => mountedPagePriority(a) - mountedPagePriority(b));
  for (const pageNumber of overscanCandidates) appendVirtualCandidate(order, seen, pageNumber);

  return new Set(order.slice(0, MAX_MOUNTED_PAGE_VIEWS));
}

function updatePageVirtualWindow(extraPages = []) {
  if (!pdf || elements.canvasShell.hidden) return;
  const desired = desiredVirtualPageNumbers(extraPages);
  // Shrink first so mounting the new window cannot transiently exceed the cap.
  for (const pageNumber of [...mountedPageViews.keys()]) {
    if (!desired.has(pageNumber)) unmountPageView(pageNumber);
  }
  for (const pageNumber of desired) mountPageView(pageNumber);
  if (mountedPageViews.size > MAX_MOUNTED_PAGE_VIEWS) {
    throw new Error(`PDF DOM virtualization exceeded ${MAX_MOUNTED_PAGE_VIEWS} mounted pages`);
  }
}

function initializePageGeometry() {
  for (const pageNumber of [...mountedPageViews.keys()]) unmountPageView(pageNumber);
  elements.pages.replaceChildren();
  retainedPageNumbers.clear();
  activePageRenderViews.clear();
  mountedPageViews.clear();
  pageGeometries.length = 1;
  for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
    pageGeometries.push({
      pageNumber,
      baseWidth: defaultPageMetrics.width,
      baseHeight: defaultPageMetrics.height,
      width: defaultPageMetrics.width * scale,
      height: defaultPageMetrics.height * scale,
      known: pageNumber === 1,
    });
  }
  rebuildPageGeometryIndex();
}

class PageView {
  constructor(pageNumber, geometry) {
    this.pageNumber = pageNumber;
    this.geometry = geometry;
    this.renderedScale = 0;
    this.renderState = 'idle';
    this.renderToken = 0;
    this.textLayerObject = null;
    this.textLayerAvailable = false;
    this.renderPromise = null;
    this.enhancementPromise = null;
    this.queuedGeneration = -1;
    this.renderJobId = 0;
    this.canvasBytes = 0;
    this.maxCanvasPixels = PDFJS_BASE_CANVAS_MAX_PIXELS;
    this.requiresCriticalDetail = false;

    this.slot = document.createElement('section');
    this.slot.className = 'page-slot';
    this.slot.dataset.pageNumber = String(pageNumber);
    this.slot.setAttribute('aria-label', `PDF page ${pageNumber}`);

    this.shell = document.createElement('div');
    this.shell.className = 'page-shell';
    this.shell.dataset.pageNumber = String(pageNumber);
    this.shell.dataset.tool = activeTool;
    this.shell.dataset.renderState = 'idle';
    this.slot.append(this.shell);

    this.previewCanvas = null;
    this.nativePageHost = null;
    this.nativePageView = null;
    this.searchLayer = null;
    this.textLayerElement = null;
    this.annotationLayer = null;
    this.vectorLayer = null;
    this.updateSize();
  }

  get canvas() {
    return this.nativePageView?.canvas ?? null;
  }

  get detailCanvas() {
    return this.nativePageView?.detailView?.canvas ?? null;
  }

  get baseWidth() {
    return this.geometry?.baseWidth ?? defaultPageMetrics.width;
  }

  get baseHeight() {
    return this.geometry?.baseHeight ?? defaultPageMetrics.height;
  }

  ensurePreviewLayer() {
    if (this.previewCanvas) return;
    const previewCanvas = document.createElement('canvas');
    previewCanvas.className = 'page-preview-canvas';
    previewCanvas.hidden = true;
    previewCanvas.setAttribute('aria-hidden', 'true');
    this.previewCanvas = previewCanvas;
    retainedPageNumbers.add(this.pageNumber);
    if (!this.nativePageHost) this.shell.replaceChildren(previewCanvas);
  }

  ensureLayers() {
    if (this.nativePageHost) return;
    this.ensurePreviewLayer();

    const nativePageHost = document.createElement('div');
    nativePageHost.className = 'pdfjs-native-page-host';
    nativePageHost.setAttribute('aria-label', `Rendered PDF page ${this.pageNumber}`);

    const searchLayer = document.createElement('div');
    searchLayer.className = 'search-layer';
    searchLayer.setAttribute('aria-hidden', 'true');

    const textLayerElement = document.createElement('div');
    textLayerElement.className = 'textLayer';
    textLayerElement.setAttribute('aria-label', `Selectable PDF text on page ${this.pageNumber}`);

    const annotationLayer = document.createElement('div');
    annotationLayer.className = 'sidecar-layer';
    annotationLayer.setAttribute('aria-label', `Rustnote PDF annotations on page ${this.pageNumber}`);

    const vectorLayer = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    vectorLayer.classList.add('vector-layer');
    vectorLayer.setAttribute('aria-hidden', 'true');

    this.nativePageHost = nativePageHost;
    this.searchLayer = searchLayer;
    this.textLayerElement = textLayerElement;
    this.annotationLayer = annotationLayer;
    this.vectorLayer = vectorLayer;
    this.shell.replaceChildren(
      this.previewCanvas,
      nativePageHost,
      searchLayer,
      textLayerElement,
      annotationLayer,
      vectorLayer,
    );
    this.showCachedPreview();
    this.applyToolState();
    this.updateSize();
  }

  ensureNativePageView(page, baseViewport, maxCanvasPixels = PDFJS_BASE_CANVAS_MAX_PIXELS) {
    this.maxCanvasPixels = Math.max(1, Math.min(PDFJS_BASE_CANVAS_MAX_PIXELS, Math.floor(maxCanvasPixels)));
    if (this.nativePageView?.pdfPage === page && this.renderedScale === scale) {
      this.nativePageView.maxCanvasPixels = this.maxCanvasPixels;
      return this.nativePageView;
    }
    if (this.nativePageView) {
      try { this.nativePageView.destroy(); } catch { /* best-effort native teardown */ }
      this.nativePageView = null;
      this.nativePageHost?.replaceChildren();
    }
    const nativeScale = scale / PixelsPerInch.PDF_TO_CSS_UNITS;
    const nativePageView = new RustnotePdfPageView({
      container: this.nativePageHost,
      eventBus: pdfjsPageEventBus,
      id: this.pageNumber,
      scale: nativeScale,
      defaultViewport: baseViewport,
      textLayerMode: PDFJS_TEXT_LAYER_DISABLED,
      annotationMode: AnnotationMode.DISABLE,
      enableDetailCanvas: true,
      // Use Mozilla's normal 32 MP page ceiling. Ordinary fit-width pages
      // therefore render once at their requested display resolution; only
      // genuinely oversized/high-zoom pages enter the restricted/detail path.
      maxCanvasPixels: this.maxCanvasPixels,
      capCanvasAreaFactor: -1,
      enableOptimizedPartialRendering: true,
      imagesRightClickMinSize: -1,
      enableAutoLinking: false,
      minDurationToUpdateCanvas: PDFJS_MIN_CANVAS_UPDATE_MS,
      l10n: pdfjsPageL10n,
      layerProperties: pdfjsPageLayerProperties,
    });
    nativePageView.setPdfPage(page);
    this.nativePageView = nativePageView;
    return nativePageView;
  }

  showCachedPreview() {
    if (!this.previewCanvas || this.renderedScale === scale) return false;
    const entry = getPreviewCacheEntry(this.pageNumber);
    if (!entry) {
      this.hideCachedPreview();
      return false;
    }
    this.previewCanvas.width = entry.canvas.width;
    this.previewCanvas.height = entry.canvas.height;
    const context = this.previewCanvas.getContext('2d', {alpha: false});
    if (!context) return false;
    context.drawImage(entry.canvas, 0, 0);
    this.previewCanvas.hidden = false;
    this.shell.classList.add('has-cached-preview');
    return true;
  }

  hideCachedPreview() {
    if (this.previewCanvas) {
      this.previewCanvas.hidden = true;
      this.previewCanvas.width = 1;
      this.previewCanvas.height = 1;
    }
    this.shell.classList.remove('has-cached-preview');
  }

  refreshCanvasBytes() {
    let bytes = 0;
    for (const canvas of [this.canvas, this.detailCanvas]) {
      if (canvas) bytes += Math.max(0, canvas.width * canvas.height * 4);
    }
    this.canvasBytes = bytes;
    return bytes;
  }

  applyToolState() {
    this.shell.dataset.tool = activeTool;
    this.textLayerElement?.classList.toggle('marking', TEXT_MARK_TOOLS.has(activeTool));
    if (this.annotationLayer) this.annotationLayer.dataset.tool = activeTool;
    if (this.vectorLayer) this.vectorLayer.dataset.tool = activeTool;
  }

  setBaseMetrics(width, height) {
    updatePageGeometryMetrics(this.pageNumber, width, height);
  }

  updateSize() {
    const geometry = this.geometry ?? pageGeometryForNumber(this.pageNumber);
    const width = Math.max(1, geometry?.width ?? this.baseWidth * scale);
    const height = Math.max(1, geometry?.height ?? this.baseHeight * scale);
    this.slot.style.top = `${Math.max(0, geometryTopForNumber(this.pageNumber))}px`;
    this.slot.style.height = `${height}px`;
    this.shell.style.width = `${width}px`;
    this.shell.style.height = `${height}px`;
    this.shell.style.setProperty('--scale-factor', String(scale));
    this.shell.style.setProperty('--user-unit', '1');
    this.shell.style.setProperty('--total-scale-factor', String(scale));
    if (this.nativePageHost) {
      this.nativePageHost.style.width = `${width}px`;
      this.nativePageHost.style.height = `${height}px`;
    }
  }

  setRenderState(state, message = '') {
    this.renderState = state;
    this.shell.dataset.renderState = state;
    if (message) this.shell.dataset.message = message;
    else delete this.shell.dataset.message;
  }

  cancelRendering(clearPromise = true) {
    this.renderToken += 1;
    try { this.nativePageView?.cancelRendering?.(); } catch { /* already complete */ }
    try { this.nativePageView?.detailView?.cancelRendering?.(); } catch { /* already complete */ }
    this.textLayerObject?.cancel?.();
    this.textLayerObject = null;
    if (clearPromise) {
      this.renderPromise = null;
      this.enhancementPromise = null;
      this.queuedGeneration = -1;
    }
  }

  invalidateScale() {
    if (activeDetailRender?.view === this) cancelActiveDetailRender();
    this.cancelRendering();
    activePageRenderViews.delete(this);
    this.renderedScale = 0;
    this.textLayerAvailable = false;
    this.hideCachedPreview();
    this.searchLayer?.replaceChildren();
    this.textLayerElement?.replaceChildren();
    this.annotationLayer?.replaceChildren();
    this.vectorLayer?.replaceChildren();
    try { this.nativePageView?.destroy(); } catch { /* best-effort native teardown */ }
    this.nativePageView = null;
    this.nativePageHost?.replaceChildren();
    this.canvasBytes = 0;
    this.maxCanvasPixels = PDFJS_BASE_CANVAS_MAX_PIXELS;
    this.requiresCriticalDetail = false;
    this.updateSize();
    if (this.previewCanvas) {
      this.previewCanvas.width = 1;
      this.previewCanvas.height = 1;
    }
    this.setRenderState('idle');
    this.showCachedPreview();
  }

  releasePreviewLayer() {
    if (this.nativePageHost || !this.previewCanvas) return;
    this.previewCanvas.width = 1;
    this.previewCanvas.height = 1;
    this.shell.replaceChildren();
    this.previewCanvas = null;
    retainedPageNumbers.delete(this.pageNumber);
    this.setRenderState('idle');
  }

  releaseLayers() {
    if (activeDetailRender?.view === this) cancelActiveDetailRender();
    this.cancelRendering();
    activePageRenderViews.delete(this);
    retainedPageNumbers.delete(this.pageNumber);
    if (this.previewCanvas) {
      this.previewCanvas.width = 1;
      this.previewCanvas.height = 1;
    }
    try { this.nativePageView?.destroy(); } catch { /* best-effort native teardown */ }
    this.nativePageView = null;
    this.canvasBytes = 0;
    this.maxCanvasPixels = PDFJS_BASE_CANVAS_MAX_PIXELS;
    this.requiresCriticalDetail = false;
    this.shell.replaceChildren();
    this.previewCanvas = null;
    this.nativePageHost = null;
    this.searchLayer = null;
    this.textLayerElement = null;
    this.annotationLayer = null;
    this.vectorLayer = null;
    this.renderedScale = 0;
    this.textLayerAvailable = false;
    this.setRenderState('idle');
  }
}

function thumbnailWindowBounds() {
  return calculateThumbnailWindowBounds({
    scrollTop: elements.thumbnailViewport.scrollTop,
    viewportHeight: elements.thumbnailViewport.clientHeight,
    pageCount: pdf.numPages,
    rowHeight: THUMBNAIL_ROW_HEIGHT,
    overscanRows: THUMBNAIL_OVERSCAN_ROWS,
  });
}

function previewDimensions(baseWidth, baseHeight) {
  return calculatePreviewDimensions({
    baseWidth,
    baseHeight,
    width: PREVIEW_WIDTH,
    maxHeight: PREVIEW_MAX_HEIGHT,
    maxPixels: MAX_PREVIEW_PIXELS,
  });
}

function clearPreviewCache() {
  previewCache.clear();
}

function getPreviewCacheEntry(pageNumber) {
  return previewCache.get(pageNumber);
}

function auxiliaryRasterSurfaceBytes(excludingPageNumber = 0) {
  let bytes = 0;
  for (const [pageNumber, view] of mountedPageViews) {
    if (pageNumber === excludingPageNumber) continue;
    if (view.previewCanvas && !view.previewCanvas.hidden) bytes += canvasBackingBytes(view.previewCanvas, RASTER_BYTES_PER_PIXEL);
  }
  for (const row of thumbnailRows.values()) {
    const canvas = row.preview?.firstElementChild;
    if (canvas?.tagName === 'CANVAS') bytes += canvasBackingBytes(canvas, RASTER_BYTES_PER_PIXEL);
  }
  return bytes;
}

function totalTrackedRasterBytes(excludingPageNumber = 0) {
  return totalRetainedCanvasBytes(excludingPageNumber)
    + auxiliaryRasterSurfaceBytes(excludingPageNumber)
    + previewCache.bytes;
}

function evictPreviewCacheToBudget(forcedBudget = null) {
  const protectedPages = new Set(visiblePageSnapshot.map(item => item.pageNumber));
  protectedPages.add(currentPage);
  protectedPages.add(currentPage + scrollDirection);
  previewCache.evictToBudget({
    retainedCanvasBytes,
    auxiliaryRasterBytes: auxiliaryRasterSurfaceBytes(),
    protectedPages,
    forcedBudget,
  });
}

function resolveQueuedPreviewPage(pageNumber, result) {
  previewRenderQueue = previewRenderQueue.filter(item => {
    if (item.pageNumber !== pageNumber) return true;
    previewQueuedPages.delete(pageNumber);
    item.resolve(result);
    return false;
  });
}

function storePreviewCacheEntry(pageNumber, canvas, baseWidth, baseHeight) {
  const entry = previewCache.store(pageNumber, {canvas, baseWidth, baseHeight});
  if (!entry) return null;
  previewFailedPages.delete(pageNumber);
  retainedCanvasBytes = totalRetainedCanvasBytes();
  evictPreviewCacheToBudget();
  resolveQueuedPreviewPage(pageNumber, true);
  materializeCachedThumbnailRow(pageNumber);
  const view = pageViewForNumber(pageNumber);
  if (view && pageShouldDisplayPreview(pageNumber)
      && !pageHasReadableRaster(view) && view.renderState !== 'rendering') {
    view.ensurePreviewLayer();
    view.showCachedPreview();
  }
  return entry;
}

function seedPreviewFromPageView(view) {
  if (!view?.canvas || view.canvas.width < 2 || view.canvas.height < 2) return false;
  const dimensions = previewDimensions(view.baseWidth, view.baseHeight);
  const canvas = document.createElement('canvas');
  canvas.width = dimensions.pixelWidth;
  canvas.height = dimensions.pixelHeight;
  const context = canvas.getContext('2d', {alpha: false});
  if (!context) return false;
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = 'medium';
  context.drawImage(view.canvas, 0, 0, canvas.width, canvas.height);
  storePreviewCacheEntry(view.pageNumber, canvas, view.baseWidth, view.baseHeight);
  return true;
}

function cachedThumbnailCanvas(entry) {
  return createThumbnailCanvas(document, entry, {
    maxWidth: THUMBNAIL_MAX_DISPLAY_WIDTH,
    maxHeight: THUMBNAIL_MAX_DISPLAY_HEIGHT,
  });
}

function materializeCachedThumbnailRow(pageNumber) {
  const row = thumbnailRows.get(pageNumber);
  if (!row) return false;
  const entry = getPreviewCacheEntry(pageNumber);
  if (!entry) return false;
  if (row.cacheEntry !== entry || row.preview.childElementCount === 0) {
    row.preview.replaceChildren(cachedThumbnailCanvas(entry));
    row.cacheEntry = entry;
  }
  row.button.dataset.renderState = 'ready';
  return true;
}

function createThumbnailRow(pageNumber) {
  const row = createThumbnailRowElement(document, {
    pageNumber,
    rowHeight: THUMBNAIL_ROW_HEIGHT,
    currentPage,
    onNavigate: goToPage,
  });
  thumbnailRows.set(pageNumber, row);
  elements.thumbnailContent.append(row.button);
  materializeCachedThumbnailRow(pageNumber);
  return row;
}

function removeThumbnailRow(pageNumber) {
  const row = thumbnailRows.get(pageNumber);
  if (!row) return;
  row.button.remove();
  thumbnailRows.delete(pageNumber);
}

function cancelActivePreviewRender() {
  const active = activePreviewRender;
  if (!active) return;
  active.cancelled = true;
  if (active.task) {
    try { active.task.cancel(); } catch { /* already complete */ }
  }
}

function cancelBackgroundPreviewRender() {
  if (!activePreviewRender) return;
  cancelActivePreviewRender();
}

function scheduleThumbnailVirtualization() {
  if (!sidebarThumbnailsActive() || thumbnailFrame) return;
  thumbnailFrame = requestAnimationFrame(() => {
    thumbnailFrame = 0;
    updateThumbnailVirtualWindow();
  });
}

function previewKindRank(kind) {
  return kind === 'visible' ? 0 : 1;
}

function previewJobPriority(pageNumber, kind, center = thumbnailBrowseCenter) {
  return calculatePreviewJobPriority({pageNumber, kind, center, currentPage});
}

function seedPreviewFromReadablePage(pageNumber) {
  const view = pageViewForNumber(pageNumber);
  if (!pageHasReadableRaster(view)) return false;
  if (previewCache.has(pageNumber)) return true;
  return seedPreviewFromPageView(view);
}

function previewJobStillNeeded(item) {
  if (!pdf || item.generation !== previewGeneration || previewCache.has(item.pageNumber)) return false;
  // Independent PDF interpretation is reserved for a thumbnail the user can
  // actually see. Overscan rows can consume an already-cached/pixel-derived
  // preview but never start their own PDF render.
  return item.kind === 'visible' && sidebarThumbnailsActive() && thumbnailRows.has(item.pageNumber);
}

function prunePreviewRenderQueue() {
  previewRenderQueue = previewRenderQueue.filter(item => {
    const valid = previewJobStillNeeded(item);
    if (valid) {
      item.priority = previewJobPriority(item.pageNumber, item.kind, thumbnailBrowseCenter);
      return true;
    }
    previewQueuedPages.delete(item.pageNumber);
    item.resolve(previewCache.has(item.pageNumber));
    return false;
  });
  previewRenderQueue.sort((a, b) => a.priority - b.priority);
}

function updateThumbnailVirtualWindow() {
  if (!sidebarThumbnailsActive()) return;
  elements.thumbnailContent.style.height = `${pdf.numPages * THUMBNAIL_ROW_HEIGHT}px`;
  const {visibleStart, visibleEnd, start, end, center} = thumbnailWindowBounds();
  thumbnailBrowseCenter = Math.max(1, Math.min(pdf.numPages, Math.round(center)));
  for (const pageNumber of [...thumbnailRows.keys()]) {
    if (pageNumber < start || pageNumber > end) removeThumbnailRow(pageNumber);
  }
  for (let pageNumber = start; pageNumber <= end; pageNumber += 1) {
    const row = thumbnailRows.get(pageNumber) ?? createThumbnailRow(pageNumber);
    row.button.classList.toggle('is-current', pageNumber === currentPage);
    if (!materializeCachedThumbnailRow(pageNumber)) {
      seedPreviewFromReadablePage(pageNumber);
      if (materializeCachedThumbnailRow(pageNumber)) continue;
      row.preview.replaceChildren();
      row.cacheEntry = null;
      if (previewFailedPages.has(pageNumber)) {
        row.button.dataset.renderState = 'error';
      } else {
        const rendering = previewQueuedPages.has(pageNumber)
          || activePreviewRender?.pageNumber === pageNumber;
        row.button.dataset.renderState = rendering ? 'rendering' : 'idle';
        if (pageNumber >= visibleStart && pageNumber <= visibleEnd) {
          void enqueuePreviewRender(pageNumber, 'visible', center);
        }
      }
    }
  }
  for (const item of previewRenderQueue) {
    if ((item.kind === 'visible' || item.kind === 'overscan') && thumbnailRows.has(item.pageNumber)) {
      if (item.pageNumber >= visibleStart && item.pageNumber <= visibleEnd) {
        item.kind = 'visible';
        item.priority = previewJobPriority(item.pageNumber, item.kind, center);
      } else {
        item.kind = 'overscan';
      }
    }
  }
  prunePreviewRenderQueue();
  if (activePreviewRender && (activePreviewRender.kind === 'visible' || activePreviewRender.kind === 'overscan')) {
    const stillVisible = thumbnailRows.has(activePreviewRender.pageNumber);
    if (!stillVisible) {
      cancelActivePreviewRender();
    } else {
      activePreviewRender.kind = activePreviewRender.pageNumber >= visibleStart
        && activePreviewRender.pageNumber <= visibleEnd ? 'visible' : 'overscan';
    }
  }
  pumpRenderScheduler();
}

function enqueuePreviewRender(pageNumber, kind, center = thumbnailBrowseCenter) {
  if (!pdf || pageNumber < 1 || pageNumber > pdf.numPages || previewCache.has(pageNumber)
      || previewFailedPages.has(pageNumber)) {
    return Promise.resolve(previewCache.has(pageNumber));
  }
  if (seedPreviewFromReadablePage(pageNumber)) return Promise.resolve(true);
  if (kind !== 'visible') return Promise.resolve(false);
  if (activePreviewRender?.pageNumber === pageNumber) {
    if (previewKindRank(kind) < previewKindRank(activePreviewRender.kind)) activePreviewRender.kind = kind;
    return activePreviewRender.promise ?? Promise.resolve(false);
  }
  const queued = previewRenderQueue.find(item => item.pageNumber === pageNumber);
  if (queued) {
    if (previewKindRank(kind) < previewKindRank(queued.kind)) queued.kind = kind;
    queued.priority = previewJobPriority(pageNumber, queued.kind, center);
    previewRenderQueue.sort((a, b) => a.priority - b.priority);
    return queued.promise;
  }
  let resolvePromise;
  const promise = new Promise(resolve => { resolvePromise = resolve; });
  previewQueuedPages.add(pageNumber);
  previewRenderQueue.push({
    pageNumber,
    kind,
    generation: previewGeneration,
    priority: previewJobPriority(pageNumber, kind, center),
    resolve: resolvePromise,
    promise,
  });
  previewRenderQueue.sort((a, b) => a.priority - b.priority);
  pumpRenderScheduler();
  return promise;
}

async function renderPreviewPage(pageNumber, generation, kind, serial) {
  const item = {pageNumber, generation, kind};
  if (!previewJobStillNeeded(item)) return previewCache.has(pageNumber);
  const row = thumbnailRows.get(pageNumber);
  if (row && kind === 'visible') row.button.dataset.renderState = 'rendering';
  let page = null;
  try {
    page = await acquirePdfPage(pageNumber);
    if (!activePreviewRender || activePreviewRender.serial !== serial || activePreviewRender.cancelled
        || !previewJobStillNeeded({pageNumber, generation, kind: activePreviewRender.kind})) return false;
    const baseViewport = page.getViewport({scale: 1});
    updatePageGeometryMetrics(pageNumber, baseViewport.width, baseViewport.height);
    const dimensions = previewDimensions(baseViewport.width, baseViewport.height);
    const viewport = page.getViewport({scale: dimensions.pageScale});
    const canvas = document.createElement('canvas');
    canvas.width = dimensions.pixelWidth;
    canvas.height = dimensions.pixelHeight;
    const context = canvas.getContext('2d', {alpha: false});
    if (!context) throw new Error('Preview canvas unavailable');
    const transform = dimensions.outputScale === 1
      ? null
      : [dimensions.outputScale, 0, 0, dimensions.outputScale, 0, 0];
    const task = page.render({
      canvasContext: context,
      viewport,
      transform,
      annotationMode: AnnotationMode.ENABLE,
    });
    activePreviewRender.task = task;
    await task.promise;
    activePreviewRender.task = null;
    if (!activePreviewRender || activePreviewRender.serial !== serial || activePreviewRender.cancelled
        || !previewJobStillNeeded({pageNumber, generation, kind: activePreviewRender.kind})) return false;
    storePreviewCacheEntry(pageNumber, canvas, baseViewport.width, baseViewport.height);
    return true;
  } catch (error) {
    if (error?.name === 'RenderingCancelledException' || error?.name === 'AbortException') {
      return false;
    }
    previewFailedPages.add(pageNumber);
    const failedRow = thumbnailRows.get(pageNumber);
    if (failedRow) failedRow.button.dataset.renderState = 'error';
    console.error(`Rustnote PDF preview ${pageNumber} failed`, error);
    return false;
  } finally {
    if (page) releasePdfPage(pageNumber, page);
  }
}

function syncVisibleThumbnailSelection() {
  if (!sidebarThumbnailsActive()) return;
  for (const row of thumbnailRows.values()) {
    row.button.classList.toggle('is-current', row.pageNumber === currentPage);
  }
}

async function destinationPageNumber(destination) {
  let explicitDestination = destination;
  if (typeof explicitDestination === 'string') {
    explicitDestination = await pdf.getDestination(explicitDestination);
  }
  if (!Array.isArray(explicitDestination) || explicitDestination.length === 0) return null;
  const reference = explicitDestination[0];
  if (Number.isInteger(reference)) return reference + 1;
  if (!reference || typeof reference !== 'object') return null;
  try {
    return (await pdf.getPageIndex(reference)) + 1;
  } catch {
    return null;
  }
}

async function ensureOutlineLoaded() {
  if (!pdf || !sidebarOpen || sidebarMode !== 'outline' || outlineLoaded) return;
  if (outlineLoadPromise) return outlineLoadPromise;
  elements.outlineContent.textContent = 'Loading contents…';
  outlineLoadPromise = pdf.getOutline()
    .then(items => {
      outlineLoaded = true;
      elements.outlineContent.replaceChildren();
      if (!Array.isArray(items) || items.length === 0) {
        const message = document.createElement('p');
        message.className = 'sidebar-empty';
        message.textContent = 'No table of contents';
        elements.outlineContent.append(message);
        return;
      }
      const {root, truncated} = renderOutlineItems(document, items, {
        maxItems: MAX_OUTLINE_ITEMS,
        onNavigateDestination: async destination => {
          const pageNumber = await destinationPageNumber(destination);
          if (pageNumber) goToPage(pageNumber);
        },
      });
      elements.outlineContent.append(root);
      if (truncated) {
        const message = document.createElement('p');
        message.className = 'sidebar-empty';
        message.textContent = 'Additional outline entries were omitted.';
        elements.outlineContent.append(message);
      }
    })
    .catch(error => {
      console.error('Rustnote PDF outline load failed', error);
      elements.outlineContent.textContent = 'Contents unavailable';
    })
    .finally(() => { outlineLoadPromise = null; });
  return outlineLoadPromise;
}


function annotationSummary(pageNumber, annotation) {
  return summarizeSidebarAnnotation(pageNumber, annotation, TEXT_MARK_TOOLS);
}

function sortAnnotationIndex() {
  annotationIndex.sort((a, b) => a.pageNumber - b.pageNumber || a.y - b.y || a.x - b.x || a.id.localeCompare(b.id));
}

function refreshAnnotationIndexPage(pageNumber) {
  if (!annotationIndexLoaded) return;
  const activeId = annotationIndex[activeAnnotationIndex]?.id ?? null;
  annotationIndex = annotationIndex.filter(item => item.pageNumber !== pageNumber);
  for (const annotation of pageAnnotations(pageNumber)) {
    annotationIndex.push(annotationSummary(pageNumber, annotation));
  }
  if (annotationIndex.length > MAX_ANNOTATION_INDEX_ENTRIES) {
    annotationIndex.length = MAX_ANNOTATION_INDEX_ENTRIES;
    annotationIndexTruncated = true;
  }
  sortAnnotationIndex();
  activeAnnotationIndex = activeId ? annotationIndex.findIndex(item => item.id === activeId) : -1;
  updateAnnotationControls();
  clearAnnotationVirtualWindow();
  scheduleAnnotationVirtualization();
}

function cancelAnnotationIndexLoad() {
  annotationIndexSerial += 1;
  if (annotationIndexController) annotationIndexController.abort();
  annotationIndexController = null;
  annotationIndexLoading = null;
}

function clearAnnotationVirtualWindow() {
  if (annotationFrame) cancelAnimationFrame(annotationFrame);
  annotationFrame = 0;
  annotationRows.clear();
  elements.annotationContent.replaceChildren();
  elements.annotationContent.style.height = '0px';
}

function annotationWindowBounds() {
  return calculateAnnotationWindowBounds({
    scrollTop: elements.annotationViewport.scrollTop,
    viewportHeight: elements.annotationViewport.clientHeight,
    itemCount: annotationIndex.length,
    rowHeight: ANNOTATION_ROW_HEIGHT,
    overscanRows: ANNOTATION_OVERSCAN_ROWS,
  });
}

function createAnnotationRow(index) {
  const item = annotationIndex[index];
  if (!item) return null;
  const button = createAnnotationRowElement(document, {
    item,
    index,
    rowHeight: ANNOTATION_ROW_HEIGHT,
    isCurrent: index === activeAnnotationIndex,
    onActivate: rowIndex => void jumpToAnnotation(rowIndex),
  });
  elements.annotationContent.append(button);
  annotationRows.set(index, button);
  return button;
}

function scheduleAnnotationVirtualization() {
  if (!sidebarAnnotationsActive() || !annotationIndexLoaded || annotationFrame) return;
  annotationFrame = requestAnimationFrame(() => {
    annotationFrame = 0;
    updateAnnotationVirtualWindow();
  });
}

function updateAnnotationVirtualWindow() {
  if (!sidebarAnnotationsActive() || !annotationIndexLoaded) return;
  if (annotationIndex.length === 0) {
    elements.annotationContent.style.height = '100%';
    if (!elements.annotationContent.querySelector('.sidebar-empty')) {
      const message = document.createElement('p');
      message.className = 'sidebar-empty';
      message.textContent = annotationIndexError ? 'Annotations unavailable' : 'No annotations';
      elements.annotationContent.replaceChildren(message);
    }
    return;
  }
  const {start, end} = annotationWindowBounds();
  elements.annotationContent.style.height = `${annotationIndex.length * ANNOTATION_ROW_HEIGHT}px`;
  for (const [index, row] of [...annotationRows.entries()]) {
    if (index < start || index >= end) {
      row.remove();
      annotationRows.delete(index);
    }
  }
  for (let index = start; index < end; index += 1) {
    const row = annotationRows.get(index) ?? createAnnotationRow(index);
    row?.classList.toggle('is-current', index === activeAnnotationIndex);
  }
}

function updateAnnotationControls() {
  const count = annotationIndex.length;
  elements.annotationCount.textContent = annotationCounterLabel({
    count,
    activeIndex: activeAnnotationIndex,
    truncated: annotationIndexTruncated,
  });
  elements.annotationPrevious.disabled = count === 0 || activeAnnotationIndex === 0;
  elements.annotationNext.disabled = count === 0 || activeAnnotationIndex === count - 1;
}

function initialAnnotationIndex(direction = 1) {
  return findInitialAnnotationIndex({items: annotationIndex, currentPage, direction});
}

async function ensureAnnotationIndexLoaded() {
  if (!pdf || !sidebarAnnotationsActive() || annotationIndexLoaded) {
    if (annotationIndexLoaded) scheduleAnnotationVirtualization();
    return;
  }
  if (annotationIndexLoading) return annotationIndexLoading;
  elements.annotationCount.textContent = '…';
  elements.annotationPrevious.disabled = true;
  elements.annotationNext.disabled = true;
  const serial = ++annotationIndexSerial;
  const controller = new AbortController();
  annotationIndexController = controller;
  annotationIndexLoading = fetch(`/rustnotepdf/annotation-index/${encodeURIComponent(attachmentId)}`, {
    method: 'GET',
    cache: 'no-store',
    credentials: 'omit',
    signal: controller.signal,
  })
    .then(async response => {
      if (serial !== annotationIndexSerial || !sidebarAnnotationsActive()) return;
      if (!response.ok) throw new Error(`Annotation index load failed (${response.status})`);
      const payload = await response.json();
      if (serial !== annotationIndexSerial || !sidebarAnnotationsActive()) return;
      const items = Array.isArray(payload?.annotations) ? payload.annotations : [];
      annotationIndex = items.slice(0, MAX_ANNOTATION_INDEX_ENTRIES).map(item => ({
        id: String(item?.id ?? ''),
        kind: String(item?.kind ?? ''),
        pageNumber: Math.max(1, Number(item?.pageIndex ?? 0) + 1),
        x: Math.max(0, Math.min(1, Number(item?.x) || 0)),
        y: Math.max(0, Math.min(1, Number(item?.y) || 0)),
        text: String(item?.text ?? '').replace(/\s+/g, ' ').trim().slice(0, 160),
      })).filter(item => item.id.startsWith('ann_') && item.pageNumber <= pdf.numPages);
      annotationIndexTruncated = Boolean(payload?.truncated) || items.length > MAX_ANNOTATION_INDEX_ENTRIES;
      annotationIndexError = false;

      // Loaded page sidecars are authoritative for this session and may contain a
      // just-created annotation whose queued database save has not completed yet.
      for (const [pageNumber, annotations] of annotationPages.entries()) {
        annotationIndex = annotationIndex.filter(item => item.pageNumber !== pageNumber);
        for (const annotation of annotations) annotationIndex.push(annotationSummary(pageNumber, annotation));
      }
      if (annotationIndex.length > MAX_ANNOTATION_INDEX_ENTRIES) {
        annotationIndex.length = MAX_ANNOTATION_INDEX_ENTRIES;
        annotationIndexTruncated = true;
      }
      sortAnnotationIndex();
      annotationIndexLoaded = true;
      activeAnnotationIndex = -1;
      updateAnnotationControls();
      scheduleAnnotationVirtualization();
    })
    .catch(error => {
      if (error?.name === 'AbortError' || serial !== annotationIndexSerial) return;
      console.error('Rustnote PDF annotation index load failed', error);
      annotationIndex = [];
      annotationIndexLoaded = true;
      annotationIndexError = true;
      activeAnnotationIndex = -1;
      updateAnnotationControls();
      const message = document.createElement('p');
      message.className = 'sidebar-empty';
      message.textContent = 'Annotations unavailable';
      elements.annotationContent.replaceChildren(message);
    })
    .finally(() => {
      if (serial === annotationIndexSerial) {
        annotationIndexLoading = null;
        annotationIndexController = null;
      }
    });
  return annotationIndexLoading;
}

function flashAnnotation(view, annotationId) {
  if (!view || !annotationId) return;
  const selector = `[data-annotation-id="${CSS.escape(annotationId)}"]`;
  for (const node of view.shell.querySelectorAll(selector)) {
    node.classList.remove('is-jump-target');
    void node.getBoundingClientRect();
    node.classList.add('is-jump-target');
    node.addEventListener('animationend', () => node.classList.remove('is-jump-target'), {once: true});
  }
}

function ensureActiveAnnotationVisible(index) {
  if (!sidebarAnnotationsActive() || index < 0) return;
  const top = index * ANNOTATION_ROW_HEIGHT;
  const bottom = top + ANNOTATION_ROW_HEIGHT;
  const viewportTop = elements.annotationViewport.scrollTop;
  const viewportBottom = viewportTop + elements.annotationViewport.clientHeight;
  if (top < viewportTop) elements.annotationViewport.scrollTop = top;
  else if (bottom > viewportBottom) {
    elements.annotationViewport.scrollTop = Math.max(0, bottom - elements.annotationViewport.clientHeight);
  }
}

async function jumpToAnnotation(index) {
  if (!pdf || index < 0 || index >= annotationIndex.length) return;
  const item = annotationIndex[index];
  activeAnnotationIndex = index;
  updateAnnotationControls();
  ensureActiveAnnotationVisible(index);
  scheduleAnnotationVirtualization();
  updatePageVirtualWindow([item.pageNumber]);
  const view = pageViewForNumber(item.pageNumber) ?? mountPageView(item.pageNumber);
  if (!view) return;
  const rect = view.shell.getBoundingClientRect();
  const targetY = window.scrollY + rect.top + rect.height * item.y;
  const desiredY = toolbarBottom() + Math.max(70, (window.innerHeight - toolbarBottom()) * 0.28);
  window.scrollTo(0, Math.max(0, targetY - desiredY));
  visiblePageSnapshot = [];
  setCurrentPage(item.pageNumber);
  const targetX = rect.left + rect.width * item.x;
  const desiredX = viewportContentCenterX();
  if (Number.isFinite(targetX) && Math.abs(targetX - desiredX) > 1) {
    elements.canvasShell.scrollLeft += targetX - desiredX;
  }
  await awaitPageReady(view);
  renderSidecars(item.pageNumber);
  requestAnimationFrame(() => flashAnnotation(view, item.id));
}

function navigateAnnotation(direction) {
  if (annotationIndex.length === 0) return;
  const next = activeAnnotationIndex < 0
    ? initialAnnotationIndex(direction)
    : Math.max(0, Math.min(annotationIndex.length - 1, activeAnnotationIndex + direction));
  void jumpToAnnotation(next);
}

function toolbarBottom() {
  const toolbarBottom = elements.toolbar?.getBoundingClientRect().bottom ?? 0;
  const markupBottom = elements.markupToolbar.hidden
    ? toolbarBottom
    : elements.markupToolbar.getBoundingClientRect().bottom;
  return Math.max(0, markupBottom);
}

function viewportContentCenterY() {
  const top = toolbarBottom();
  return top + Math.max(1, window.innerHeight - top) / 2;
}

function viewportContentCenterX() {
  const rect = elements.canvasShell.getBoundingClientRect();
  const left = Math.max(0, rect.left);
  const right = Math.min(window.innerWidth, rect.right);
  if (right <= left) return window.innerWidth / 2;
  return left + (right - left) / 2;
}

function pageViewAtPoint(clientX, clientY) {
  const safeX = Math.max(1, Math.min(window.innerWidth - 1, clientX));
  const safeY = Math.max(toolbarBottom() + 1, Math.min(window.innerHeight - 1, clientY));
  const shell = document.elementFromPoint(safeX, safeY)?.closest?.('.page-shell');
  if (shell?.dataset.pageNumber) return pageViewForNumber(Number(shell.dataset.pageNumber));
  const pageNumber = pageNumberAtDocumentY(window.scrollY + safeY);
  return pageViewForNumber(pageNumber) ?? mountPageView(pageNumber);
}

function captureScrollAnchor(clientX = viewportContentCenterX(), clientY = viewportContentCenterY()) {
  const view = pageViewAtPoint(clientX, clientY);
  if (!view) return null;
  const rect = view.shell.getBoundingClientRect();
  return {
    pageNumber: view.pageNumber,
    xRatio: Math.max(0, Math.min(1, (clientX - rect.left) / Math.max(1, rect.width))),
    yRatio: Math.max(0, Math.min(1, (clientY - rect.top) / Math.max(1, rect.height))),
    clientX,
    clientY,
  };
}

function restoreScrollAnchor(anchor) {
  if (!anchor) return;
  requestAnimationFrame(() => {
    const view = pageViewForNumber(anchor.pageNumber) ?? mountPageView(anchor.pageNumber);
    if (!view) return;
    const rect = view.shell.getBoundingClientRect();
    const deltaX = rect.left + rect.width * anchor.xRatio - anchor.clientX;
    const deltaY = rect.top + rect.height * anchor.yRatio - anchor.clientY;
    if (Number.isFinite(deltaX) && Math.abs(deltaX) > 0.5) {
      elements.canvasShell.scrollLeft += deltaX;
    }
    if (Number.isFinite(deltaY) && Math.abs(deltaY) > 0.5) window.scrollBy(0, deltaY);
    scheduleViewportUpdate();
  });
}

function setCurrentPage(pageNumber) {
  if (!pdf) return;
  const next = Math.max(1, Math.min(pdf.numPages, Math.trunc(Number(pageNumber) || 1)));
  if (next === currentPage) {
    updateControls();
    return;
  }
  pageViewForNumber(currentPage)?.shell.classList.remove('is-current');
  scrollDirection = next > currentPage ? 1 : -1;
  currentPage = next;
  const currentView = pageViewForNumber(currentPage) ?? mountPageView(currentPage);
  currentView?.shell.classList.add('is-current');
  const needsSharpRender = currentView
    && !pageHasReadableRaster(currentView);
  if (needsSharpRender) {
    currentView.ensurePreviewLayer();
    currentView.showCachedPreview();
    const visibleSet = visiblePageNumberSet();
    for (const view of [...activePageRenderViews]) {
      if (!visibleSet.has(view.pageNumber) && view.pageNumber !== currentPage) view.releaseLayers();
    }
  }
  cancelBackgroundPreviewRender();
  syncVisibleThumbnailSelection();
  updateControls();
  renderAroundCurrentPage();
}

function findPageAtViewportCenter() {
  return pageNumberAtDocumentY(window.scrollY + viewportContentCenterY());
}

function visiblePageDescriptors() {
  if (!pdf || elements.canvasShell.hidden) return [];
  const viewportTop = Math.max(0, toolbarBottom());
  const viewportBottom = window.innerHeight;
  const canvasShellRect = elements.canvasShell.getBoundingClientRect();
  const viewportLeft = Math.max(0, canvasShellRect.left);
  const viewportRight = Math.min(window.innerWidth, canvasShellRect.right);
  const descriptors = [];
  const {start, end} = geometryWindowForViewport(0);
  for (let pageNumber = start; pageNumber <= end; pageNumber += 1) {
    // The virtual window owns mounting. Descriptor collection must never mount
    // opportunistically, otherwise an unusually dense viewport can churn the
    // strict DOM cap while retaining descriptors for views that were evicted.
    const view = pageViewForNumber(pageNumber);
    if (!view) continue;
    const rect = view.shell.getBoundingClientRect();
    const left = Math.max(viewportLeft, rect.left);
    const right = Math.min(viewportRight, rect.right);
    const top = Math.max(viewportTop, rect.top);
    const bottom = Math.min(viewportBottom, rect.bottom);
    if (right <= left || bottom <= top) continue;
    const visibleArea = Math.max(1, (right - left) * (bottom - top));
    const pageArea = Math.max(1, rect.width * rect.height);
    descriptors.push({
      pageNumber,
      view,
      percent: visibleArea / pageArea,
      visibleArea: {
        minX: Math.max(0, left - rect.left),
        minY: Math.max(0, top - rect.top),
        maxX: Math.min(rect.width, right - rect.left),
        maxY: Math.min(rect.height, bottom - rect.top),
      },
    });
  }
  descriptors.sort((a, b) => b.percent - a.percent || a.pageNumber - b.pageNumber);
  return descriptors;
}

function visiblePageNumberSet() {
  return new Set(visiblePageSnapshot.map(item => item.pageNumber));
}

function pageIsVisible(pageNumber) {
  return visiblePageSnapshot.some(item => item.pageNumber === pageNumber);
}

function pageHasReadableRaster(view) {
  return Boolean(view) && view.renderedScale === scale
    && (view.renderState === 'raster-ready' || view.renderState === 'ready');
}

function visiblePagesNeedRaster() {
  return visiblePageSnapshot.some(item => !pageHasReadableRaster(item.view));
}

function updateNativeVisibleAreas() {
  const visibleByPage = new Map(visiblePageSnapshot.map(item => [item.pageNumber, item]));
  for (const pageNumber of retainedPageNumbers) {
    const view = pageViewForNumber(pageNumber);
    const nativePageView = view?.nativePageView;
    if (!nativePageView) continue;
    const descriptor = visibleByPage.get(pageNumber);
    try { nativePageView.updateVisibleArea(descriptor?.visibleArea ?? null); } catch { /* detail rendering is optional */ }
  }
}

function updateCurrentPageFromViewport() {
  if (!pdf || elements.canvasShell.hidden) return;
  updatePageVirtualWindow();
  visiblePageSnapshot = visiblePageDescriptors();
  const mostVisible = visiblePageSnapshot[0]?.pageNumber ?? findPageAtViewportCenter();
  setCurrentPage(mostVisible);
  updateNativeVisibleAreas();
}

function scheduleViewportUpdate() {
  if (scrollFrame) return;
  scrollFrame = requestAnimationFrame(() => {
    scrollFrame = 0;
    updateCurrentPageFromViewport();
    renderAroundCurrentPage();
  });
}

function pageShouldDisplayPreview(pageNumber) {
  if (!pdf || pageNumber < 1 || pageNumber > pdf.numPages) return false;
  if (pageIsVisible(pageNumber)) return true;
  return Math.abs(pageNumber - currentPage) <= 2;
}

function materializeNearViewportPreviews() {
  if (!pdf) return;
  const candidates = new Set(visiblePageSnapshot.map(item => item.pageNumber));
  for (let pageNumber = Math.max(1, currentPage - 2);
    pageNumber <= Math.min(pdf.numPages, currentPage + 2);
    pageNumber += 1) candidates.add(pageNumber);
  for (const pageNumber of candidates) {
    const view = pageViewForNumber(pageNumber);
    if (!view || pageHasReadableRaster(view) || !previewCache.has(pageNumber)) continue;
    view.ensurePreviewLayer();
    view.showCachedPreview();
  }
  for (const pageNumber of [...retainedPageNumbers]) {
    const view = pageViewForNumber(pageNumber);
    if (view?.previewCanvas && !view.nativePageHost && !pageShouldDisplayPreview(pageNumber)) {
      view.releasePreviewLayer();
    }
  }
}

function pageOwnedByLiveView(pageNumber, page) {
  const view = pageViewForNumber(pageNumber);
  return Boolean(view?.nativePageView?.pdfPage === page);
}

async function acquirePdfPage(pageNumber) {
  // PDF.js memoizes PDFPageProxy objects per document. Avoid a second Rustnote
  // page-proxy cache; Rustnote only owns the heavyweight render surfaces.
  return pdf.getPage(pageNumber);
}

function releasePdfPage(pageNumber, page) {
  if (!page || pageOwnedByLiveView(pageNumber, page)) return;
  try { page.cleanup(); } catch { /* cleanup is best-effort */ }
}

function readingWindow() {
  return calculateReadingWindow({
    visiblePageNumbers: visiblePageSnapshot.map(item => item.pageNumber),
    currentPage,
    scrollDirection,
  });
}

function estimatedPageRasterBytes(pageNumber) {
  const view = pageViewForNumber(pageNumber);
  if (view?.canvasBytes > 0) return view.canvasBytes;
  const geometry = pageGeometryForNumber(pageNumber);
  if (!geometry) return 0;
  const dpr = Math.max(1, Number(window.devicePixelRatio) || 1);
  const requestedPixels = Math.max(1, geometry.width * geometry.height * dpr ** 2);
  return Math.min(requestedPixels, PDFJS_BASE_CANVAS_MAX_PIXELS) * 4;
}

function renderTargetPageNumbers() {
  if (!pdf) return new Set();
  return planRenderTargetPageNumbers({
    pageCount: pdf.numPages,
    visiblePageNumbers: visiblePageSnapshot.map(item => item.pageNumber),
    currentPage,
    scrollDirection,
    readAheadForward: PAGE_READ_AHEAD_FORWARD,
    estimatePageBytes: estimatedPageRasterBytes,
    minimumOperatingBytes: CANVAS_MEMORY_MIN_OPERATING_BYTES,
    hardLimitBytes: CANVAS_MEMORY_HARD_LIMIT_BYTES,
  });
}

function pageRenderPriority(pageNumber) {
  return calculatePageRenderPriority({
    pageNumber,
    visiblePages: visiblePageSnapshot,
    currentPage,
    scrollDirection,
    readAheadForward: PAGE_READ_AHEAD_FORWARD,
  });
}

function pageShouldRender(pageNumber) {
  return renderTargetPageNumbers().has(pageNumber);
}

function adaptiveCanvasBudget(targetPages, views) {
  const visibleSet = visiblePageNumberSet();
  const visibleBytes = views
    .filter(view => visibleSet.has(view.pageNumber))
    .reduce((total, view) => total + Math.max(0, view.canvasBytes), 0);
  const typicalPageBytes = medianPositive(views.map(view => view.canvasBytes));
  const desiredWarmPages = Math.max(1, targetPages.size - visibleSet.size);
  return calculateMemoryOperatingBudget({
    visibleBytes,
    typicalPageBytes,
    desiredWarmPages,
    minimumBytes: CANVAS_MEMORY_MIN_OPERATING_BYTES,
    hardLimitBytes: CANVAS_MEMORY_HARD_LIMIT_BYTES,
  });
}

function releaseDetailCanvas(view, allowVisible = false) {
  const detail = view?.nativePageView?.detailView;
  const canvas = detail?.canvas;
  if (!canvas || canvas.width < 1 || canvas.height < 1) return 0;
  if (!allowVisible && pageIsVisible(view.pageNumber)) return 0;
  const before = view.refreshCanvasBytes();
  try {
    detail.reset({keepCanvas: false});
  } catch {
    return 0;
  }
  view.requiresCriticalDetail = Boolean(view.nativePageView?.detailView) && pageIsVisible(view.pageNumber);
  const after = view.refreshCanvasBytes();
  const released = Math.max(0, before - after);
  return released;
}

function retainedCanvasViews() {
  return [...retainedPageNumbers]
    .map(pageViewForNumber)
    .filter(view => view?.canvas);
}

function totalRetainedCanvasBytes(excludingPageNumber = 0) {
  return retainedCanvasViews().reduce((total, view) => {
    if (view.pageNumber === excludingPageNumber) return total;
    return total + Math.max(0, view.refreshCanvasBytes());
  }, 0);
}

function updateMemoryAccounting(views, operatingBudget) {
  retainedCanvasBytes = views.reduce((total, view) => total + Math.max(0, view.refreshCanvasBytes()), 0);
  canvasOperatingBudgetBytes = operatingBudget;
  evictPreviewCacheToBudget();
}

function reclaimRasterSpaceForPage(view, desiredBytes) {
  const visibleSet = visiblePageNumberSet();
  const interactionPages = activeInteractionPageNumbers();
  const protectedPages = new Set([...visibleSet, ...interactionPages, view.pageNumber]);

  // Preview pixels and off-screen detail are the cheapest surfaces to surrender.
  retainedCanvasBytes = totalRetainedCanvasBytes(view.pageNumber);
  evictPreviewCacheToBudget(0);

  // Reserve every currently allocated Rustnote-owned raster surface. The
  // incoming page's preview copy is excluded because a successful base render
  // immediately replaces it.
  let currentBytes = totalRetainedCanvasBytes(view.pageNumber)
    + auxiliaryRasterSurfaceBytes(view.pageNumber) + previewCache.bytes;
  const detailCandidates = retainedCanvasViews()
    .filter(candidate => candidate.pageNumber !== view.pageNumber && candidate.detailCanvas)
    .sort((a, b) => {
      const aProtected = protectedPages.has(a.pageNumber) ? 1 : 0;
      const bProtected = protectedPages.has(b.pageNumber) ? 1 : 0;
      if (aProtected !== bProtected) return aProtected - bProtected;
      return pageRenderPriority(b.pageNumber) - pageRenderPriority(a.pageNumber);
    });
  for (const candidate of detailCandidates) {
    if (currentBytes + desiredBytes <= CANVAS_MEMORY_HARD_LIMIT_BYTES) break;
    releaseDetailCanvas(candidate, true);
    currentBytes = totalRetainedCanvasBytes(view.pageNumber)
      + auxiliaryRasterSurfaceBytes(view.pageNumber) + previewCache.bytes;
  }

  // Then discard non-visible base rasters, farthest/lowest-priority first. They
  // retain their tiny cached preview and can be recreated if navigation returns.
  const baseCandidates = retainedCanvasViews()
    .filter(candidate => candidate.pageNumber !== view.pageNumber && !protectedPages.has(candidate.pageNumber))
    .sort((a, b) => pageRenderPriority(b.pageNumber) - pageRenderPriority(a.pageNumber));
  for (const candidate of baseCandidates) {
    if (currentBytes + desiredBytes <= CANVAS_MEMORY_HARD_LIMIT_BYTES) break;
    candidate.releaseLayers();
    currentBytes = totalRetainedCanvasBytes(view.pageNumber)
      + auxiliaryRasterSurfaceBytes(view.pageNumber) + previewCache.bytes;
  }
  return Math.max(0, CANVAS_MEMORY_HARD_LIMIT_BYTES - currentBytes);
}

function adaptiveMaxCanvasPixelsForRender(view, requestedPixels) {
  const desiredPixels = Math.min(PDFJS_BASE_CANVAS_MAX_PIXELS, Math.max(1, requestedPixels));
  const ordinaryBytes = desiredPixels * RASTER_BYTES_PER_PIXEL;
  const restrictedBytes = desiredPixels * RASTER_BYTES_PER_PIXEL * (1 + RESTRICTED_PAGE_BASE_AREA_FACTOR);
  const desiredBytes = requestedPixels <= PDFJS_BASE_CANVAS_MAX_PIXELS ? ordinaryBytes : restrictedBytes;
  const availableBytes = reclaimRasterSpaceForPage(view, desiredBytes);

  if (requestedPixels <= PDFJS_BASE_CANVAS_MAX_PIXELS && ordinaryBytes <= availableBytes) {
    return PDFJS_BASE_CANVAS_MAX_PIXELS;
  }
  if (requestedPixels > PDFJS_BASE_CANVAS_MAX_PIXELS && restrictedBytes <= availableBytes) {
    return PDFJS_BASE_CANVAS_MAX_PIXELS;
  }

  // A restricted page uses at most maxCanvasPixels for its detail surface and
  // roughly 1/16 that area for the optimized low-resolution base. Cap both with
  // one allocation so a visible page remains readable without exceeding the
  // absolute tracked-raster ceiling.
  const bytesPerRestrictedPixel = RASTER_BYTES_PER_PIXEL * (1 + RESTRICTED_PAGE_BASE_AREA_FACTOR);
  const limitedPixels = Math.max(1, Math.floor(availableBytes / bytesPerRestrictedPixel));
  const result = Math.max(1, Math.min(PDFJS_BASE_CANVAS_MAX_PIXELS, limitedPixels));
  return result;
}

function pageVisibilityPercent(pageNumber) {
  return visiblePageSnapshot.find(item => item.pageNumber === pageNumber)?.percent ?? 0;
}

function downgradeRasterToCachedPreview(view) {
  if (!view?.canvas) return false;
  seedPreviewFromPageView(view);
  view.releaseLayers();
  view.ensurePreviewLayer();
  view.showCachedPreview();
  return true;
}

function enforceAbsoluteRasterCeiling(protectedSet) {
  retainedCanvasBytes = totalRetainedCanvasBytes();
  evictPreviewCacheToBudget();
  let trackedBytes = totalTrackedRasterBytes();
  if (trackedBytes <= CANVAS_MEMORY_HARD_LIMIT_BYTES) return false;

  // This is an emergency path for unusual layouts (e.g. several enormous pages
  // simultaneously visible). Preserve the current/most-visible page and reduce
  // the least-visible protected base to its tiny cached preview. Its next render
  // receives an adaptive smaller maxCanvasPixels cap, so the hard ceiling stays
  // enforced instead of repeatedly overflowing.
  const candidates = retainedCanvasViews()
    .filter(view => protectedSet.has(view.pageNumber) && view.pageNumber !== currentPage)
    .sort((a, b) => {
      const visibilityDelta = pageVisibilityPercent(a.pageNumber) - pageVisibilityPercent(b.pageNumber);
      if (Math.abs(visibilityDelta) > 0.0001) return visibilityDelta;
      return pageRenderPriority(b.pageNumber) - pageRenderPriority(a.pageNumber);
    });
  let downgraded = false;
  for (const view of candidates) {
    if (trackedBytes <= CANVAS_MEMORY_HARD_LIMIT_BYTES) break;
    if (!downgradeRasterToCachedPreview(view)) continue;
    downgraded = true;
    retainedCanvasBytes = totalRetainedCanvasBytes();
    evictPreviewCacheToBudget();
    trackedBytes = totalTrackedRasterBytes();
  }
  return downgraded;
}

function enforceCanvasMemoryBudget(targetPages) {
  const visibleSet = visiblePageNumberSet();
  const protectedSet = new Set([...visibleSet, ...activeInteractionPageNumbers()]);
  let views = retainedCanvasViews();
  for (const view of views) view.refreshCanvasBytes();
  const operatingBudget = adaptiveCanvasBudget(targetPages, views);

  let totalBytes = views.reduce((total, view) => total + Math.max(0, view.canvasBytes), 0);
  const detailCandidates = views
    .filter(view => view.detailCanvas)
    .sort((a, b) => {
      const aProtected = protectedSet.has(a.pageNumber) ? 1 : 0;
      const bProtected = protectedSet.has(b.pageNumber) ? 1 : 0;
      if (aProtected !== bProtected) return aProtected - bProtected;
      return pageRenderPriority(b.pageNumber) - pageRenderPriority(a.pageNumber);
    });
  for (const view of detailCandidates) {
    if (totalBytes <= operatingBudget
        && totalBytes + auxiliaryRasterSurfaceBytes() + previewCache.bytes <= CANVAS_MEMORY_HARD_LIMIT_BYTES) break;
    // Visible critical detail survives the adaptive operating budget, but not a
    // true hard-limit breach caused by a newly visible/interaction surface.
    const hardPressure = totalBytes + auxiliaryRasterSurfaceBytes()
      + previewCache.bytes > CANVAS_MEMORY_HARD_LIMIT_BYTES;
    if (protectedSet.has(view.pageNumber) && !hardPressure) continue;
    totalBytes = Math.max(0, totalBytes - releaseDetailCanvas(view, hardPressure));
  }

  views = retainedCanvasViews();
  for (const view of views) view.refreshCanvasBytes();
  views.sort((a, b) => {
    const aProtected = protectedSet.has(a.pageNumber) ? 0 : 1;
    const bProtected = protectedSet.has(b.pageNumber) ? 0 : 1;
    if (aProtected !== bProtected) return aProtected - bProtected;
    const aTarget = targetPages.has(a.pageNumber) ? 0 : 1;
    const bTarget = targetPages.has(b.pageNumber) ? 0 : 1;
    if (aTarget !== bTarget) return aTarget - bTarget;
    return pageRenderPriority(a.pageNumber) - pageRenderPriority(b.pageNumber);
  });

  let keptBytes = 0;
  for (const view of views) {
    const bytes = Math.max(0, view.canvasBytes);
    if (bytes === 0) continue;
    const protectedPage = protectedSet.has(view.pageNumber);
    const target = targetPages.has(view.pageNumber);
    const distance = Math.abs(view.pageNumber - currentPage);
    const withinAdaptiveWindow = distance <= ADAPTIVE_RETENTION_MAX_DISTANCE;
    if (protectedPage || ((target || withinAdaptiveWindow) && keptBytes + bytes <= operatingBudget)) {
      keptBytes += bytes;
      continue;
    }
    view.releaseLayers();
  }

  let retainedViews = retainedCanvasViews();
  updateMemoryAccounting(retainedViews, operatingBudget);
  const downgraded = enforceAbsoluteRasterCeiling(protectedSet);
  if (downgraded) {
    retainedViews = retainedCanvasViews();
    updateMemoryAccounting(retainedViews, operatingBudget);
    queueMicrotask(() => {
      if (pdf) renderAroundCurrentPage();
    });
  }
}

function enqueuePageRender(view) {
  if (!pdf || !view || !pageShouldRender(view.pageNumber)) return Promise.resolve(false);
  if (pageHasReadableRaster(view)) return Promise.resolve(true);
  if (view.renderPromise && view.queuedGeneration === renderGeneration) return view.renderPromise;

  let resolvePromise;
  const promise = new Promise(resolve => { resolvePromise = resolve; });
  const jobId = ++view.renderJobId;
  view.renderPromise = promise;
  view.queuedGeneration = renderGeneration;
  renderQueue = renderQueue.filter(item => item.view !== view);
  renderQueue.push({
    view,
    generation: renderGeneration,
    jobId,
    priority: pageRenderPriority(view.pageNumber),
    resolve: resolvePromise,
  });
  renderQueue.sort((a, b) => a.priority - b.priority);
  // Any real page raster outranks preview/detail work. If lower-priority raster
  // work happens to be active when navigation/read-ahead creates a page job,
  // cancel it and return the single raster lane to page readiness.
  cancelBackgroundPreviewRender();
  cancelActiveDetailRender();
  if (pageIsVisible(view.pageNumber) || view.pageNumber === currentPage) preemptForVisiblePages();
  pumpRenderScheduler();
  return promise;
}

function prunePageRenderQueue() {
  renderQueue = renderQueue.filter(item => {
    const valid = item.generation === renderGeneration && pageShouldRender(item.view.pageNumber);
    if (valid) return true;
    if (item.view.renderJobId === item.jobId) {
      item.view.renderPromise = null;
      item.view.queuedGeneration = -1;
    }
    item.resolve(false);
    return false;
  });
  for (const item of renderQueue) item.priority = pageRenderPriority(item.view.pageNumber);
  renderQueue.sort((a, b) => a.priority - b.priority);
}

function takePageRender(visibleOnly = false) {
  prunePageRenderQueue();
  const index = renderQueue.findIndex(item => !visibleOnly || pageIsVisible(item.view.pageNumber));
  if (index < 0) return null;
  return renderQueue.splice(index, 1)[0];
}

function takeNextReadingPageRender() {
  prunePageRenderQueue();
  const {forwardEdge} = readingWindow();
  const nextPage = forwardEdge + scrollDirection;
  const index = renderQueue.findIndex(item => item.view.pageNumber === nextPage);
  if (index < 0) return null;
  return renderQueue.splice(index, 1)[0];
}

function takeReadAheadPageRender() {
  prunePageRenderQueue();
  const index = renderQueue.findIndex(item => !pageIsVisible(item.view.pageNumber));
  if (index < 0) return null;
  return renderQueue.splice(index, 1)[0];
}

function takePreviewRender(kind) {
  prunePreviewRenderQueue();
  const index = previewRenderQueue.findIndex(item => item.kind === kind);
  if (index < 0) return null;
  const [item] = previewRenderQueue.splice(index, 1);
  previewQueuedPages.delete(item.pageNumber);
  return item;
}

function cancelActiveDetailRender() {
  const active = activeDetailRender;
  if (!active || active.cancelRequested) return false;
  active.cancelRequested = true;
  try {
    // reset() is preferable to a bare renderTask cancellation here: it returns
    // Mozilla's detail view to INITIAL so it can be cleanly rendered again once
    // the viewport settles, while preserving any already-complete detail canvas.
    active.detail.reset({keepCanvas: true});
  } catch {
    try { active.detail.cancelRendering(); } catch { /* already complete */ }
  }
  return true;
}

function preemptForVisiblePages() {
  const visibleSet = visiblePageNumberSet();
  if (visiblePagesNeedRaster()) {
    cancelActiveDetailRender();
    cancelBackgroundPreviewRender();
    for (const activeView of [...activePageRenderViews]) {
      if (!visibleSet.has(activeView.pageNumber)) activeView.releaseLayers();
    }
    return;
  }
  if (activeDetailRender && !visibleSet.has(activeDetailRender.view.pageNumber)) {
    cancelActiveDetailRender();
  }
}

function startPageRender(item) {
  activePageRenders += 1;
  activePageRenderViews.add(item.view);
  void renderPageView(item.view, item.generation)
    .then(item.resolve, error => {
      console.error('Rustnote PDF page render queue failed', error);
      item.resolve(false);
    })
    .finally(() => {
      if (item.view.renderJobId === item.jobId) {
        item.view.renderPromise = null;
        item.view.queuedGeneration = -1;
      }
      activePageRenderViews.delete(item.view);
      activePageRenders = Math.max(0, activePageRenders - 1);
      enforceCanvasMemoryBudget(renderTargetPageNumbers());
      updateNativeVisibleAreas();
      pumpRenderScheduler();
    });
}

function estimatedDetailRasterBytes(descriptor) {
  const area = descriptor?.visibleArea;
  const view = descriptor?.view;
  if (!area || !view) return 0;
  const dpr = Math.max(1, Number(window.devicePixelRatio) || 1);
  const visiblePixels = Math.max(1, (area.maxX - area.minX) * (area.maxY - area.minY) * dpr ** 2);
  const maxDetailPixels = Math.max(1, view.maxCanvasPixels || PDFJS_BASE_CANVAS_MAX_PIXELS);
  // PDF.js pads the detail region by up to one visible width/height on each
  // side (3×3 area). If the viewport itself is larger than maxCanvasPixels,
  // upstream intentionally renders that visible area anyway; account for that
  // rare case rather than underestimating the hard-limit preflight.
  const estimatedPixels = visiblePixels > maxDetailPixels
    ? visiblePixels
    : Math.min(maxDetailPixels, visiblePixels * 9);
  return estimatedPixels * RASTER_BYTES_PER_PIXEL;
}

function ensureDetailRasterCapacity(descriptor) {
  const view = descriptor?.view;
  if (!view) return false;
  const estimatedBytes = estimatedDetailRasterBytes(descriptor);
  if (estimatedBytes <= 0) return true;

  let currentBytes = totalRetainedCanvasBytes() + auxiliaryRasterSurfaceBytes();
  if (currentBytes + previewCache.bytes + estimatedBytes <= CANVAS_MEMORY_HARD_LIMIT_BYTES) return true;

  // Detail is more important than previews, but less important than a blank
  // visible base. Collapse preview cache and ordinary retained surfaces first.
  retainedCanvasBytes = currentBytes;
  evictPreviewCacheToBudget(0);
  enforceCanvasMemoryBudget(renderTargetPageNumbers());
  currentBytes = totalRetainedCanvasBytes() + auxiliaryRasterSurfaceBytes();
  return currentBytes + previewCache.bytes + estimatedBytes <= CANVAS_MEMORY_HARD_LIMIT_BYTES;
}

function visibleDetailCandidate({criticalOnly = false} = {}) {
  // Restricted pages deliberately use a very small low-resolution base so the
  // first complete page arrives quickly while PDF.js records operation bounds.
  // Their first visible detail is therefore reading-critical and may run before
  // next-page read-ahead. Ordinary/quality-only detail remains settled-only.
  if (visiblePagesNeedRaster()) return null;
  if (!criticalOnly && !scrollSettled) return null;
  updateNativeVisibleAreas();
  for (const descriptor of visiblePageSnapshot) {
    const view = descriptor.view;
    if (!pageHasReadableRaster(view)) continue;
    const detail = view.nativePageView?.detailView;
    if (!detail) continue;
    if (criticalOnly && !view.requiresCriticalDetail) continue;
    if (!criticalOnly && view.requiresCriticalDetail) continue;
    if (detail.renderingState !== RenderingStates.INITIAL) continue;
    if (!ensureDetailRasterCapacity(descriptor)) {
      continue;
    }
    return {view, detail};
  }
  return null;
}

function startDetailRender(candidate) {
  const {view, detail} = candidate;
  const critical = view.requiresCriticalDetail;
  const active = {view, detail, critical, cancelRequested: false};
  activeDetailRender = active;
  void detail.draw()
    .catch(error => {
      if (error?.name !== 'RenderingCancelledException' && error?.name !== 'AbortException') {
        console.error('Rustnote PDF detail render failed', error);
      }
    })
    .finally(() => {
      if (!active.cancelRequested && critical) view.requiresCriticalDetail = false;
      view.refreshCanvasBytes();
      if (activeDetailRender === active) activeDetailRender = null;
      enforceCanvasMemoryBudget(renderTargetPageNumbers());
      pumpRenderScheduler();
    });
}

function startPreviewRender(item) {
  const serial = ++previewRenderSerial;
  const active = {
    pageNumber: item.pageNumber,
    kind: item.kind,
    generation: item.generation,
    serial,
    task: null,
    cancelled: false,
    promise: item.promise,
  };
  activePreviewRender = active;
  void renderPreviewPage(item.pageNumber, item.generation, item.kind, serial)
    .then(item.resolve, error => {
      console.error('Rustnote PDF preview render queue failed', error);
      item.resolve(false);
    })
    .finally(() => {
      if (activePreviewRender?.serial === serial) activePreviewRender = null;
      scheduleThumbnailVirtualization();
      pumpRenderScheduler();
    });
}

function pumpRenderScheduler() {
  if (!pdf || activePageRenders > 0 || activePreviewRender || activeDetailRender) return;
  const action = nextRenderAction({
    takeVisiblePage: () => takePageRender(true),
    visiblePagesNeedRaster,
    takeCriticalDetail: () => visibleDetailCandidate({criticalOnly: true}),
    takeNextReadingPage: takeNextReadingPageRender,
    takeReadAheadPage: takeReadAheadPageRender,
    takeSettledDetail: () => visibleDetailCandidate({criticalOnly: false}),
    takeVisiblePreview: () => takePreviewRender('visible'),
  });
  if (!action) return;
  if (action.kind === 'page') startPageRender(action.item);
  else if (action.kind === 'detail') startDetailRender(action.item);
  else startPreviewRender(action.item);
}

function renderAroundCurrentPage() {
  if (!pdf || pageGeometries.length <= 1) return;
  updatePageVirtualWindow();
  if (visiblePageSnapshot.length === 0) visiblePageSnapshot = visiblePageDescriptors();
  const targetPages = renderTargetPageNumbers();
  updatePageVirtualWindow(targetPages);
  materializeNearViewportPreviews();
  const targetViews = [...targetPages]
    .map(pageNumber => pageViewForNumber(pageNumber) ?? mountPageView(pageNumber))
    .filter(Boolean)
    .sort((a, b) => pageRenderPriority(a.pageNumber) - pageRenderPriority(b.pageNumber));
  for (const view of targetViews) void enqueuePageRender(view);
  for (const descriptor of visiblePageSnapshot) {
    if (pageHasReadableRaster(descriptor.view) && descriptor.view.renderState !== 'ready') {
      void ensurePageEnhancements(descriptor.view);
    }
  }

  prunePageRenderQueue();
  for (const view of [...activePageRenderViews]) {
    if (!targetPages.has(view.pageNumber)) view.releaseLayers();
  }
  enforceCanvasMemoryBudget(targetPages);
  scheduleThumbnailVirtualization();
  preemptForVisiblePages();
  updateNativeVisibleAreas();
  pumpRenderScheduler();
}

async function ensurePageMetrics(pageNumber) {
  const geometry = pageGeometryForNumber(pageNumber);
  if (!geometry) return defaultPageMetrics;
  if (geometry.known) return {width: geometry.baseWidth, height: geometry.baseHeight};
  const page = await acquirePdfPage(pageNumber);
  try {
    const viewport = page.getViewport({scale: 1});
    updatePageGeometryMetrics(pageNumber, viewport.width, viewport.height);
    return {width: viewport.width, height: viewport.height};
  } finally {
    const view = pageViewForNumber(pageNumber);
    if (!view || view.renderState !== 'rendering') releasePdfPage(pageNumber, page);
  }
}

function applyScale(nextScale, anchor = captureScrollAnchor()) {
  const clamped = Math.max(MIN_SCALE, Math.min(MAX_SCALE, Number(nextScale) || scale));
  if (Math.abs(clamped - scale) < 0.0001) {
    renderAroundCurrentPage();
    return;
  }
  scale = clamped;
  renderGeneration += 1;
  cancelBackgroundPreviewRender();
  cancelActiveDetailRender();
  for (const item of renderQueue) item.resolve(false);
  renderQueue = [];
  rebuildPageGeometryIndex();
  for (const view of mountedPageViews.values()) view.invalidateScale();
  updatePageVirtualWindow();
  restoreScrollAnchor(anchor);
  renderAroundCurrentPage();
  updateControls();
}

async function applyFitMode(mode) {
  if (!pdf) return false;
  const targetMode = mode === 'page' ? 'page' : 'width';
  const operation = ++fitOperationSerial;
  const targetPage = currentPage;
  const anchor = captureScrollAnchor();
  try {
    const metrics = await ensurePageMetrics(targetPage);
    if (operation !== fitOperationSerial || !pdf) return false;
    const availableWidth = Math.max(240, elements.canvasShell.clientWidth - 28);
    const widthScale = availableWidth / Math.max(1, metrics.width);
    const nextScale = targetMode === 'page'
      ? Math.min(
          widthScale,
          Math.max(180, window.innerHeight - toolbarBottom() - 36) / Math.max(1, metrics.height),
        )
      : widthScale;
    fitMode = targetMode;
    applyScale(nextScale, anchor);
    updateControls();
    return await enqueuePageRender(pageViewForNumber(targetPage) ?? mountPageView(targetPage));
  } catch (error) {
    console.error(`Rustnote PDF fit-${targetMode} failed`, error);
    const detail = errorDetail(error);
    setStatus(`PDF page setup failed: ${detail}. Use Open externally.`, true);
    notifyHost('page-error', detail);
    return false;
  }
}

function fitWidth() {
  return applyFitMode('width');
}

function fitPage() {
  return applyFitMode('page');
}

function refitCurrentMode() {
  if (fitMode === 'width') return fitWidth();
  if (fitMode === 'page') return fitPage();
  scheduleViewportUpdate();
  return Promise.resolve(false);
}

function goToPage(value) {
  if (!pdf) return;
  const next = clampPageNumber(value, pdf.numPages);
  updatePageVirtualWindow([next]);
  const geometry = pageGeometryForNumber(next);
  const view = pageViewForNumber(next) ?? mountPageView(next);
  if (!geometry || !view) return;
  const top = pagesDocumentTop() + geometryTopForNumber(next) - toolbarBottom() - 12;
  window.scrollTo(0, Math.max(0, top));
  visiblePageSnapshot = [];
  setCurrentPage(next);
  scheduleViewportUpdate();
  void enqueuePageRender(view);
}

function setMarkupOptionGroup(tool) {
  const group = TEXT_MARK_TOOLS.has(tool)
    ? elements.textMarkOptions
    : tool === 'note'
      ? elements.noteOptions
      : tool === 'pen'
        ? elements.penOptions
        : tool === 'shape'
          ? elements.shapeOptions
          : tool === 'text'
            ? elements.textOptions
            : null;
  for (const optionGroup of [
    elements.textMarkOptions,
    elements.noteOptions,
    elements.penOptions,
    elements.shapeOptions,
    elements.textOptions,
  ]) {
    optionGroup.hidden = optionGroup !== group;
  }
  elements.markupOptions.hidden = !group;
}

function setPressedOption(groupName, value) {
  for (const button of elements.markupOptions.querySelectorAll(`[data-option-group="${groupName}"]`)) {
    const active = button.dataset.optionValue === value;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  }
}

function applyMarkupOption(button) {
  const group = button.dataset.optionGroup;
  const value = button.dataset.optionValue;
  if (!group || !value) return;
  switch (group) {
    case 'text-mark-color': textMarkColor = value; break;
    case 'note-color': noteColor = value; break;
    case 'note-size': noteSize = value; break;
    case 'pen-color': penColor = value; break;
    case 'pen-width': penWidth = value; break;
    case 'shape-type': shapeType = value; break;
    case 'shape-color': shapeColor = value; break;
    case 'shape-width': shapeWidth = value; break;
    case 'text-color': textColor = value; break;
    case 'text-size': textSize = value; break;
    default: return;
  }
  setPressedOption(group, value);
}

function setTool(tool) {
  if (READ_ONLY && tool !== 'select') tool = 'select';
  const activeVectorTool = currentVector?.kind === 'ink' ? 'pen' : currentVector?.kind === 'shape' ? 'shape' : null;
  if (currentVector && tool !== activeVectorTool) {
    const view = pageViewForNumber(currentVector.pageNumber);
    currentVector = null;
    if (view?.vectorLayer) renderVectorLayer(view, pageAnnotations(view.pageNumber));
  }
  activeTool = tool;
  if (tool !== 'select') hideSelectionActions();
  for (const [name, button] of [
    ['select', elements.toolSelect],
    ['highlight', elements.toolHighlight],
    ['underline', elements.toolUnderline],
    ['strike', elements.toolStrike],
    ['note', elements.toolNote],
    ['pen', elements.toolPen],
    ['shape', elements.toolShape],
    ['text', elements.toolText],
    ['erase', elements.toolErase],
  ]) {
    const active = name === tool;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-pressed', String(active));
  }
  setMarkupOptionGroup(tool);
  for (const view of mountedPageViews.values()) {
    view.applyToolState();
    if (view.canvas && view.renderedScale === scale) renderSidecars(view.pageNumber);
  }
  updateControls();
}

function updateControls() {
  const loaded = Boolean(pdf);
  elements.sidebarToggle.disabled = !loaded;
  elements.previous.disabled = !loaded || currentPage <= 1;
  elements.next.disabled = !loaded || currentPage >= (pdf?.numPages ?? 1);
  elements.pageNumber.disabled = !loaded;
  elements.pageNumber.max = loaded ? String(pdf.numPages) : '1';
  if (document.activeElement !== elements.pageNumber) elements.pageNumber.value = String(currentPage);
  elements.pageCount.textContent = loaded ? `/ ${pdf.numPages}` : '/ —';
  elements.zoomOut.disabled = !loaded || scale <= MIN_SCALE;
  elements.zoomIn.disabled = !loaded || scale >= MAX_SCALE;
  elements.fitWidth.disabled = !loaded;
  elements.fitPage.disabled = !loaded;
  elements.fitWidth.classList.toggle('is-active', loaded && fitMode === 'width');
  elements.fitPage.classList.toggle('is-active', loaded && fitMode === 'page');
  elements.fitWidth.setAttribute('aria-pressed', String(loaded && fitMode === 'width'));
  elements.fitPage.setAttribute('aria-pressed', String(loaded && fitMode === 'page'));
  elements.searchToggle.disabled = !loaded;
  elements.search.disabled = !loaded;
  elements.markupDivider.hidden = READ_ONLY;
  elements.markupToggle.hidden = READ_ONLY;
  elements.markupToggle.disabled = READ_ONLY || !loaded;
  elements.exportPdf.disabled = !loaded || exportBusy;
  elements.annotationUndo.disabled = READ_ONLY || undoStack.length === 0;
  elements.annotationRedo.disabled = READ_ONLY || redoStack.length === 0;
  const textMarksUnavailable = READ_ONLY || !pageViewForNumber(currentPage)?.textLayerAvailable;
  elements.toolHighlight.disabled = textMarksUnavailable;
  elements.toolUnderline.disabled = textMarksUnavailable;
  elements.toolStrike.disabled = textMarksUnavailable;
  elements.toolNote.disabled = READ_ONLY;
  elements.toolPen.disabled = READ_ONLY;
  elements.toolShape.disabled = READ_ONLY;
  elements.toolText.disabled = READ_ONLY;
  elements.toolErase.disabled = READ_ONLY;
  elements.searchPrevious.disabled = searchMatches.length === 0;
  elements.searchNext.disabled = searchMatches.length === 0;
  updateAnnotationControls();
}


async function flushAnnotationWrites() {
  for (const [pageNumber, timer] of textAnnotationSaveTimers) {
    clearTimeout(timer);
    textAnnotationSaveTimers.delete(pageNumber);
    await queueAnnotationSave(pageNumber);
  }
  await Promise.all([...annotationSaveChains.values()].map(promise => promise.catch(() => undefined)));
}

async function annotationPageNumbersForExport() {
  const response = await fetch(`/rustnotepdf/annotation-pages/${encodeURIComponent(attachmentId)}`, {
    method: 'GET', cache: 'no-store', credentials: 'omit',
  });
  if (!response.ok) throw new Error(`Annotation page list failed (${response.status})`);
  const payload = await response.json();
  const pages = new Set(Array.isArray(payload?.pages)
    ? payload.pages.map(value => Number(value) + 1).filter(value => Number.isInteger(value) && value >= 1 && value <= pdf.numPages)
    : []);
  for (const [pageNumber, annotations] of annotationPages) {
    if (annotations.length > 0) pages.add(pageNumber);
  }
  return [...pages].sort((a, b) => a - b);
}

async function stageAnnotatedPdfExport() {
  await flushAnnotationWrites();
  const pageNumbers = await annotationPageNumbersForExport();
  const storage = pdf.annotationStorage;
  const keys = [];
  try {
    for (const pageNumber of pageNumbers) {
      const annotations = await loadAnnotations(pageNumber);
      if (annotations.length === 0) continue;
      const page = await pdf.getPage(pageNumber);
      try {
        const viewport = page.getViewport({scale: 1});
        for (const annotation of annotations) {
          const value = portableAnnotationValue(annotation, pageNumber - 1, viewport);
          if (!value) continue;
          const key = `pdfjs_internal_editor_rustnote_${pageNumber}_${String(annotation.id || annotationId())}`;
          storage.setValue(key, value);
          keys.push(key);
        }
      } finally {
        try { page.cleanup(); } catch { /* best-effort */ }
      }
    }
    const bytes = await pdf.saveDocument();
    const response = await fetch(`/rustnotepdf/export/${encodeURIComponent(attachmentId)}`, {
      method: 'POST', cache: 'no-store', credentials: 'omit',
      headers: {'Content-Type': 'application/pdf'}, body: bytes,
    });
    if (!response.ok) throw new Error(`Export staging failed (${response.status})`);
    const payload = await response.json();
    const token = String(payload?.token ?? '');
    if (!/^[A-Fa-f0-9]{32}$/.test(token)) throw new Error('Export staging token is invalid');
    return token;
  } finally {
    for (const key of keys) storage.remove(key);
  }
}

async function requestAnnotatedPdfExport() {
  if (!pdf || exportBusy) return;
  exportBusy = true;
  updateControls();
  setStatus('Preparing annotated PDF…');
  const requestId = `pdf_export_${attachmentId}_${Date.now().toString(36)}_${++exportRequestSerial}`;
  try {
    const token = await stageAnnotatedPdfExport();
    notifyHost('export-ready', JSON.stringify({requestId, token}));
    setStatus('Choose where to save the annotated PDF…');
  } catch (error) {
    exportBusy = false;
    updateControls();
    const detail = errorDetail(error);
    setStatus(`Could not prepare annotated PDF: ${detail}`, true);
    notifyHost('export-error', detail);
  }
}

function completeAnnotatedPdfExport(requestId, success, message) {
  if (!String(requestId || '').startsWith('pdf_export_')) return;
  exportBusy = false;
  updateControls();
  if (success) setStatus(message || 'Annotated PDF saved.');
  else if (message && message !== 'cancelled') setStatus(message, true);
  else setStatus('');
}


function pageAnnotations(pageNumber) {
  return annotationPages.get(pageNumber) ?? [];
}

function annotationPayload(pageNumber) {
  return JSON.stringify({version: 1, annotations: pageAnnotations(pageNumber)});
}

async function loadAnnotations(pageNumber) {
  if (annotationPages.has(pageNumber)) return pageAnnotations(pageNumber);
  if (annotationLoads.has(pageNumber)) return annotationLoads.get(pageNumber);
  const promise = fetch(`/rustnotepdf/annotations/${encodeURIComponent(attachmentId)}/${pageNumber - 1}`, {
    method: 'GET',
    cache: 'no-store',
    credentials: 'omit',
  })
    .then(async response => {
      if (!response.ok) throw new Error(`Annotation load failed (${response.status})`);
      const payload = await response.json();
      const annotations = Array.isArray(payload?.annotations) ? payload.annotations : [];
      annotationPages.set(pageNumber, annotations);
      return annotations;
    })
    .catch(error => {
      console.error('Rustnote PDF annotation load failed', error);
      annotationPages.set(pageNumber, []);
      setStatus('PDF loaded, but annotations could not be read.', true);
      return [];
    })
    .finally(() => annotationLoads.delete(pageNumber));
  annotationLoads.set(pageNumber, promise);
  return promise;
}

function queueAnnotationSave(pageNumber) {
  if (READ_ONLY) return Promise.resolve();
  const payload = annotationPayload(pageNumber);
  const prior = annotationSaveChains.get(pageNumber) ?? Promise.resolve();
  const next = prior
    .catch(() => undefined)
    .then(async () => {
      const response = await fetch(`/rustnotepdf/annotations/${encodeURIComponent(attachmentId)}/${pageNumber - 1}`, {
        method: 'POST',
        cache: 'no-store',
        credentials: 'omit',
        headers: {'Content-Type': 'application/json'},
        body: payload,
      });
      if (!response.ok) throw new Error(`Annotation save failed (${response.status})`);
    })
    .catch(error => {
      console.error('Rustnote PDF annotation save failed', error);
      setStatus('An annotation could not be saved.', true);
    });
  annotationSaveChains.set(pageNumber, next);
  return next;
}

function scheduleTextAnnotationSave(pageNumber) {
  if (READ_ONLY) return;
  clearTimeout(textAnnotationSaveTimers.get(pageNumber));
  textAnnotationSaveTimers.set(pageNumber, setTimeout(() => {
    textAnnotationSaveTimers.delete(pageNumber);
    void queueAnnotationSave(pageNumber);
  }, TEXT_ANNOTATION_SAVE_DELAY_MS));
}

function pushHistory(pageNumber, before, after) {
  if (READ_ONLY) return;
  const beforeJson = JSON.stringify(before);
  const afterJson = JSON.stringify(after);
  if (beforeJson === afterJson) return;
  undoStack.push({pageNumber, before: beforeJson, after: afterJson});
  if (undoStack.length > 200) undoStack.shift();
  redoStack.length = 0;
  updateControls();
}

function commitPageMutation(pageNumber, before) {
  if (READ_ONLY) return;
  const after = cloneAnnotations(pageAnnotations(pageNumber));
  pushHistory(pageNumber, before, after);
  void queueAnnotationSave(pageNumber);
  refreshAnnotationIndexPage(pageNumber);
  renderSidecars(pageNumber);
}

async function applyHistory(entry, useAfter) {
  if (READ_ONLY) return;
  const annotations = JSON.parse(useAfter ? entry.after : entry.before);
  annotationPages.set(entry.pageNumber, annotations);
  await queueAnnotationSave(entry.pageNumber);
  refreshAnnotationIndexPage(entry.pageNumber);
  if (entry.pageNumber !== currentPage) goToPage(entry.pageNumber);
  await enqueuePageRender(pageViewForNumber(entry.pageNumber) ?? mountPageView(entry.pageNumber));
  renderSidecars(entry.pageNumber);
}

async function undoAnnotation() {
  if (READ_ONLY) return;
  const entry = undoStack.pop();
  if (!entry) return;
  redoStack.push(entry);
  updateControls();
  await applyHistory(entry, false);
}

async function redoAnnotation() {
  if (READ_ONLY) return;
  const entry = redoStack.pop();
  if (!entry) return;
  undoStack.push(entry);
  updateControls();
  await applyHistory(entry, true);
}


function selectedPageView() {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  const ancestor = range.commonAncestorContainer.nodeType === Node.ELEMENT_NODE
    ? range.commonAncestorContainer
    : range.commonAncestorContainer.parentElement;
  const layer = ancestor?.closest?.('.textLayer');
  if (!layer) return null;
  const shell = layer.closest('.page-shell');
  return shell?.dataset.pageNumber ? pageViewForNumber(Number(shell.dataset.pageNumber)) : null;
}


function hideSelectionActions() {
  if (selectionActionFrame) cancelAnimationFrame(selectionActionFrame);
  selectionActionFrame = 0;
  elements.selectionActions.hidden = true;
}

function selectionContext() {
  if (!pdf || activeTool !== 'select') return null;
  const selection = window.getSelection();
  const view = selectedPageView();
  if (!selection || !view || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const text = selection.toString();
  if (!text.trim()) return null;
  const range = selection.getRangeAt(0);
  if (!view.textLayerElement?.contains(range.commonAncestorContainer)
      && range.commonAncestorContainer !== view.textLayerElement) return null;
  const rects = [...range.getClientRects()].filter(rect => rect.width > 1 && rect.height > 1);
  if (rects.length === 0) return null;
  return {selection, view, range, text, rects};
}

function updateSelectionActions() {
  if (selectionActionFrame) return;
  selectionActionFrame = requestAnimationFrame(() => {
    selectionActionFrame = 0;
    const context = selectionContext();
    if (!context) {
      elements.selectionActions.hidden = true;
      return;
    }
    const first = context.rects[0];
    const last = context.rects[context.rects.length - 1];
    elements.selectionHighlight.hidden = READ_ONLY;
    elements.selectionNote.hidden = READ_ONLY;
    elements.selectionActions.hidden = false;
    const bounds = elements.selectionActions.getBoundingClientRect();
    const preferredLeft = (first.left + last.right) / 2 - bounds.width / 2;
    const left = Math.max(8, Math.min(window.innerWidth - bounds.width - 8, preferredLeft));
    const above = first.top - bounds.height - 8;
    const top = above > toolbarBottom() + 4
      ? above
      : Math.min(window.innerHeight - bounds.height - 8, last.bottom + 8);
    elements.selectionActions.style.left = `${left}px`;
    elements.selectionActions.style.top = `${Math.max(toolbarBottom() + 4, top)}px`;
  });
}

async function copySelectedText() {
  const context = selectionContext();
  if (!context) return false;
  let copied = false;
  try {
    copied = document.execCommand('copy');
  } catch {
    copied = false;
  }
  if (!copied && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(context.text);
      copied = true;
    } catch {
      copied = false;
    }
  }
  if (!copied) {
    setStatus('Selected PDF text could not be copied.', true);
    return false;
  }
  context.selection.removeAllRanges();
  hideSelectionActions();
  return true;
}

function createNoteFromSelection() {
  if (READ_ONLY) return false;
  const context = selectionContext();
  if (!context) return false;
  const last = context.rects[context.rects.length - 1];
  const x = Math.min(last.right, context.view.shell.getBoundingClientRect().right - 4);
  const y = Math.min(last.bottom + 4, context.view.shell.getBoundingClientRect().bottom - 4);
  context.selection.removeAllRanges();
  hideSelectionActions();
  createNote(context.view, x, y);
  return true;
}

function createTextMarkFromSelection(kind = 'highlight') {
  if (READ_ONLY || !TEXT_MARK_TOOLS.has(kind)) return false;
  const selection = window.getSelection();
  const view = selectedPageView();
  if (!selection || !view || !view.textLayerElement) return false;
  const range = selection.getRangeAt(0);
  if (!view.textLayerElement.contains(range.commonAncestorContainer)
      && range.commonAncestorContainer !== view.textLayerElement) return false;

  setCurrentPage(view.pageNumber);
  const pageBounds = view.shell.getBoundingClientRect();
  const rects = [...range.getClientRects()]
    .filter(rect => rect.width > 1 && rect.height > 1)
    .slice(0, 64)
    .map(rect => {
      const x = Math.min(1, Math.max(0, (rect.left - pageBounds.left) / pageBounds.width));
      const y = Math.min(1, Math.max(0, (rect.top - pageBounds.top) / pageBounds.height));
      return [
        x,
        y,
        Math.min(1 - x, Math.max(0, rect.width / pageBounds.width)),
        Math.min(1 - y, Math.max(0, rect.height / pageBounds.height)),
      ];
    });
  if (rects.length === 0) return false;

  const before = cloneAnnotations(pageAnnotations(view.pageNumber));
  const now = Date.now();
  pageAnnotations(view.pageNumber).push({
    id: annotationId(),
    kind,
    createdAt: now,
    updatedAt: now,
    rects,
    text: selection.toString().slice(0, 8192),
    color: textMarkColor,
  });
  selection.removeAllRanges();
  commitPageMutation(view.pageNumber, before);
  return true;
}


function createNote(view, clientX, clientY) {
  if (READ_ONLY || !view.annotationLayer) return;
  setCurrentPage(view.pageNumber);
  const before = cloneAnnotations(pageAnnotations(view.pageNumber));
  const [rawX, rawY] = normalizedPoint(view, clientX, clientY);
  const [width, height] = boxSizeFor(NOTE_SIZES, noteSize);
  const now = Date.now();
  const note = {
    id: annotationId(), kind: 'note', createdAt: now, updatedAt: now,
    x: Math.min(1 - width, rawX), y: Math.min(1 - height, rawY),
    width, height, text: '', color: noteColor,
  };
  pageAnnotations(view.pageNumber).push(note);
  commitPageMutation(view.pageNumber, before);
  requestAnimationFrame(() => {
    const textarea = view.annotationLayer?.querySelector(`[data-annotation-id="${CSS.escape(note.id)}"] textarea`);
    textarea?.focus();
  });
}

function createTextBox(view, clientX, clientY) {
  if (READ_ONLY || !view.annotationLayer) return;
  setCurrentPage(view.pageNumber);
  const before = cloneAnnotations(pageAnnotations(view.pageNumber));
  const [rawX, rawY] = normalizedPoint(view, clientX, clientY);
  const [width, height] = boxSizeFor(TEXT_BOX_SIZES, textSize);
  const now = Date.now();
  const annotation = {
    id: annotationId(), kind: 'text', createdAt: now, updatedAt: now,
    x: Math.min(1 - width, rawX), y: Math.min(1 - height, rawY),
    width, height, text: '', color: textColor, size: textSize,
  };
  pageAnnotations(view.pageNumber).push(annotation);
  commitPageMutation(view.pageNumber, before);
  requestAnimationFrame(() => {
    const textarea = view.annotationLayer?.querySelector(`[data-annotation-id="${CSS.escape(annotation.id)}"] textarea`);
    textarea?.focus();
  });
}

function removeAnnotation(pageNumber, id) {
  if (READ_ONLY) return;
  const annotations = pageAnnotations(pageNumber);
  const index = annotations.findIndex(annotation => annotation.id === id);
  if (index < 0) return;
  const before = cloneAnnotations(annotations);
  annotations.splice(index, 1);
  commitPageMutation(pageNumber, before);
}

function renderTextMarks(view, annotations) {
  for (const annotation of annotations.filter(item => TEXT_MARK_TOOLS.has(item.kind))) {
    for (const rect of annotation.rects ?? []) {
      const mark = document.createElement('button');
      mark.type = 'button';
      mark.className = 'sidecar-text-mark';
      mark.dataset.annotationId = annotation.id;
      mark.dataset.markKind = annotation.kind;
      mark.title = annotation.text || annotation.kind;
      mark.style.left = `${rect[0] * 100}%`;
      mark.style.top = `${rect[1] * 100}%`;
      mark.style.width = `${rect[2] * 100}%`;
      mark.style.height = `${rect[3] * 100}%`;
      mark.style.setProperty('--annotation-color', colorFor(annotation));
      mark.tabIndex = activeTool === 'erase' ? 0 : -1;
      if (!READ_ONLY && activeTool === 'erase') {
        mark.addEventListener('click', () => removeAnnotation(view.pageNumber, annotation.id));
      }
      view.annotationLayer.append(mark);
    }
  }
}

function attachEditableTextBox(view, annotation, className, placeholder, fontSize = null) {
  const pageNumber = view.pageNumber;
  const wrapper = document.createElement('div');
  wrapper.className = className;
  wrapper.dataset.annotationId = annotation.id;
  wrapper.style.left = `${annotation.x * 100}%`;
  wrapper.style.top = `${annotation.y * 100}%`;
  wrapper.style.width = `${annotation.width * 100}%`;
  wrapper.style.minHeight = `${annotation.height * 100}%`;
  wrapper.style.setProperty('--annotation-color', colorFor(annotation));

  if (!READ_ONLY && activeTool === 'erase') {
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'sidecar-box-erase';
    remove.textContent = `Remove ${annotation.kind === 'note' ? 'note' : 'text'}`;
    remove.addEventListener('click', () => removeAnnotation(pageNumber, annotation.id));
    wrapper.append(remove);
  } else {
    const textarea = document.createElement('textarea');
    textarea.value = annotation.text ?? '';
    textarea.maxLength = 16384;
    textarea.placeholder = placeholder;
    textarea.setAttribute('aria-label', annotation.kind === 'note' ? 'PDF note' : 'PDF text box');
    textarea.readOnly = READ_ONLY;
    if (fontSize) textarea.style.fontSize = `${fontSize}px`;
    if (!READ_ONLY) {
      let beforeFocus = null;
      textarea.addEventListener('focus', () => {
        beforeFocus = cloneAnnotations(pageAnnotations(pageNumber));
      });
      textarea.addEventListener('input', () => {
        annotation.text = textarea.value;
        annotation.updatedAt = Date.now();
        scheduleTextAnnotationSave(pageNumber);
      });
      textarea.addEventListener('blur', () => {
        clearTimeout(textAnnotationSaveTimers.get(pageNumber));
        textAnnotationSaveTimers.delete(pageNumber);
        const before = beforeFocus ?? cloneAnnotations(pageAnnotations(pageNumber));
        pushHistory(pageNumber, before, cloneAnnotations(pageAnnotations(pageNumber)));
        void queueAnnotationSave(pageNumber);
        refreshAnnotationIndexPage(pageNumber);
        beforeFocus = null;
      });
    }
    wrapper.append(textarea);
  }
  view.annotationLayer.append(wrapper);
}

function renderNotes(view, annotations) {
  for (const annotation of annotations.filter(item => item.kind === 'note')) {
    attachEditableTextBox(view, annotation, 'sidecar-note', 'Note…');
  }
}

function renderTextBoxes(view, annotations) {
  for (const annotation of annotations.filter(item => item.kind === 'text')) {
    const baseSize = TEXT_FONT_SIZES[annotation.size] ?? TEXT_FONT_SIZES.medium;
    attachEditableTextBox(view, annotation, 'sidecar-text-box', 'Text…', baseSize * scale);
  }
}

function renderVectorLayer(view, annotations, preview = currentVector?.pageNumber === view.pageNumber ? currentVector : null) {
  if (!view.vectorLayer) return;
  view.vectorLayer.replaceChildren();
  const bounds = view.shell.getBoundingClientRect();
  const width = Math.max(1, bounds.width);
  const height = Math.max(1, bounds.height);
  view.vectorLayer.setAttribute('viewBox', `0 0 ${width} ${height}`);
  view.vectorLayer.setAttribute('width', String(width));
  view.vectorLayer.setAttribute('height', String(height));

  const vectors = annotations.filter(item => item.kind === 'ink' || item.kind === 'shape');
  if (preview) vectors.push(preview);
  for (const annotation of vectors) {
    let vector = null;
    if (annotation.kind === 'ink') {
      const polyline = document.createElementNS('http://www.w3.org/2000/svg', 'polyline');
      polyline.setAttribute('fill', 'none');
      polyline.setAttribute('stroke-linecap', 'round');
      polyline.setAttribute('stroke-linejoin', 'round');
      polyline.setAttribute('points', (annotation.points ?? [])
        .map(([x, y]) => `${x * width},${y * height}`).join(' '));
      vector = polyline;
    } else if (annotation.kind === 'shape' && Array.isArray(annotation.points) && annotation.points.length === 2) {
      const [[x1, y1], [x2, y2]] = annotation.points;
      const left = Math.min(x1, x2) * width;
      const top = Math.min(y1, y2) * height;
      const shapeWidth = Math.abs(x2 - x1) * width;
      const shapeHeight = Math.abs(y2 - y1) * height;
      if (annotation.shape === 'ellipse') {
        vector = document.createElementNS('http://www.w3.org/2000/svg', 'ellipse');
        vector.setAttribute('cx', String(left + shapeWidth / 2));
        vector.setAttribute('cy', String(top + shapeHeight / 2));
        vector.setAttribute('rx', String(shapeWidth / 2));
        vector.setAttribute('ry', String(shapeHeight / 2));
      } else if (annotation.shape === 'line') {
        vector = document.createElementNS('http://www.w3.org/2000/svg', 'line');
        vector.setAttribute('x1', String(x1 * width));
        vector.setAttribute('y1', String(y1 * height));
        vector.setAttribute('x2', String(x2 * width));
        vector.setAttribute('y2', String(y2 * height));
      } else {
        vector = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        vector.setAttribute('x', String(left));
        vector.setAttribute('y', String(top));
        vector.setAttribute('width', String(shapeWidth));
        vector.setAttribute('height', String(shapeHeight));
        vector.setAttribute('rx', '2');
      }
      vector.setAttribute('fill', 'none');
    }
    if (!vector) continue;
    vector.dataset.annotationId = annotation.id ?? '';
    vector.setAttribute('stroke', colorFor(annotation));
    vector.setAttribute('stroke-width', String(Math.max(1.2, (annotation.width ?? 0.003) * width)));
    if (!READ_ONLY && activeTool === 'erase' && annotation.id) {
      vector.classList.add('is-erasable');
      vector.addEventListener('pointerdown', event => {
        event.preventDefault();
        event.stopPropagation();
        removeAnnotation(view.pageNumber, annotation.id);
      });
    }
    view.vectorLayer.append(vector);
  }
}

function renderSidecars(pageNumber) {
  const view = pageViewForNumber(pageNumber);
  if (!view?.annotationLayer || !view.vectorLayer || view.renderedScale !== scale) return;
  const annotations = pageAnnotations(pageNumber);
  view.annotationLayer.replaceChildren();
  renderTextMarks(view, annotations);
  renderNotes(view, annotations);
  renderTextBoxes(view, annotations);
  renderVectorLayer(view, annotations);
  view.applyToolState();
}

function isRenderCurrent(view, generation, token) {
  return generation === renderGeneration && token === view.renderToken && pageShouldRender(view.pageNumber);
}

async function renderTextLayerForPage(view, page, viewport, generation, token) {
  view.textLayerAvailable = false;
  view.textLayerElement.replaceChildren();
  try {
    // Match PDF.js's own TextLayerBuilder contract. Streaming avoids building
    // a second complete text-content object before layout and keeps the layer
    // suitable for large/complex pages.
    const nextTextLayer = new TextLayer({
      textContentSource: page.streamTextContent({
        includeMarkedContent: true,
        disableNormalization: true,
      }),
      container: view.textLayerElement,
      viewport,
    });
    view.textLayerObject = nextTextLayer;

    // PDF.js writes --min-font-size while constructing TextLayer. Compute the
    // reciprocal in JavaScript as a WebKit-compatible fallback rather than
    // depending on newer CSS division support.
    const minFontSize = Number.parseFloat(
      view.textLayerElement.style.getPropertyValue('--min-font-size'),
    );
    if (Number.isFinite(minFontSize) && minFontSize > 0) {
      view.textLayerElement.style.setProperty('--min-font-size-inv', String(1 / minFontSize));
    }

    await nextTextLayer.render();
    if (!isRenderCurrent(view, generation, token)) return false;

    const endOfContent = document.createElement('div');
    endOfContent.className = 'endOfContent';
    view.textLayerElement.append(endOfContent);
    view.textLayerAvailable = true;
    view.applyToolState();
    renderSearchHighlights(view);
    return true;
  } catch (error) {
    if (error?.name === 'AbortException') return false;
    const detail = errorDetail(error);
    console.error('Rustnote PDF text layer render failed', error);
    view.textLayerElement.replaceChildren();
    view.textLayerObject = null;
    if (activeTool === 'highlight' && view.pageNumber === currentPage) setTool('select');
    notifyHost('degraded', `Text layer unavailable on page ${view.pageNumber}: ${detail}`);
    return false;
  } finally {
    if (view.pageNumber === currentPage) updateControls();
  }
}

async function finishPageEnhancements(view, page, viewport, generation, token) {
  try {
    await renderTextLayerForPage(view, page, viewport, generation, token);
    if (!isRenderCurrent(view, generation, token) && !pageIsVisible(view.pageNumber)) return false;

    await loadAnnotations(view.pageNumber);
    if (generation !== renderGeneration || token !== view.renderToken) return false;
    try {
      renderSidecars(view.pageNumber);
    } catch (error) {
      console.error('Rustnote PDF annotation layer render failed', error);
      notifyHost('degraded', `Annotation layer unavailable on page ${view.pageNumber}: ${errorDetail(error)}`);
    }

    view.setRenderState('ready');
    if (view.pageNumber === currentPage && elements.status.dataset.error !== 'true') setStatus('');
    return true;
  } finally {
    if (view.enhancementPromise) view.enhancementPromise = null;
    releasePdfPage(view.pageNumber, page);
    if (view.pageNumber === currentPage) updateControls();
  }
}

async function ensurePageEnhancements(view) {
  if (!pdf || !view || !pageHasReadableRaster(view) || view.renderState === 'ready'
      || view.enhancementPromise || !pageIsVisible(view.pageNumber)) return false;
  const generation = renderGeneration;
  const token = view.renderToken;
  let page = null;
  try {
    page = await acquirePdfPage(view.pageNumber);
    if (generation !== renderGeneration || token !== view.renderToken || !pageIsVisible(view.pageNumber)) {
      releasePdfPage(view.pageNumber, page);
      return false;
    }
    const viewport = page.getViewport({scale});
    const enhancementPage = page;
    page = null;
    view.enhancementPromise = finishPageEnhancements(
      view,
      enhancementPage,
      viewport,
      generation,
      token,
    ).catch(error => {
      if (error?.name !== 'AbortException' && error?.name !== 'RenderingCancelledException') {
        console.error('Rustnote PDF page enhancement retry failed', error);
      }
      return false;
    });
    return true;
  } catch (error) {
    if (page) releasePdfPage(view.pageNumber, page);
    console.error('Rustnote PDF page enhancement setup failed', error);
    return false;
  }
}


async function awaitPageReady(view) {
  if (!view) return false;
  const rendered = await enqueuePageRender(view);
  if (!rendered) return false;
  if (view.renderState !== 'ready') {
    if (!view.enhancementPromise) await ensurePageEnhancements(view);
    if (view.enhancementPromise) await view.enhancementPromise;
  }
  return view.renderState === 'ready';
}

async function renderPageView(view, generation) {
  if (!pdf || generation !== renderGeneration || !pageShouldRender(view.pageNumber)) return false;
  view.cancelRendering(false);
  const token = view.renderToken;
  view.ensureLayers();
  view.showCachedPreview();
  view.setRenderState('rendering', 'Rendering…');
  view.textLayerAvailable = false;
  updateControls();

  let page = null;
  try {
    page = await acquirePdfPage(view.pageNumber);
    if (!isRenderCurrent(view, generation, token)) return false;

    const baseViewport = page.getViewport({scale: 1});
    view.setBaseMetrics(baseViewport.width, baseViewport.height);
    const viewport = page.getViewport({scale});
    view.shell.style.setProperty('--user-unit', String(viewport.userUnit || 1));
    view.shell.style.setProperty('--total-scale-factor', String(viewport.scale));

    const deviceScale = Math.max(1, Number(window.devicePixelRatio) || 1);
    const requestedPixels = Math.max(1, Math.round(viewport.width * viewport.height * deviceScale ** 2));
    const maxCanvasPixels = adaptiveMaxCanvasPixelsForRender(view, requestedPixels);
    const nativePageView = view.ensureNativePageView(page, baseViewport, maxCanvasPixels);
    const visibleDescriptor = visiblePageSnapshot.find(item => item.pageNumber === view.pageNumber);
    if (visibleDescriptor) {
      try { nativePageView.updateVisibleArea(visibleDescriptor.visibleArea); } catch { /* optional detail path */ }
    }

    await nativePageView.draw();
    if (!isRenderCurrent(view, generation, token) && !pageIsVisible(view.pageNumber)) return false;
    view.renderedScale = scale;
    view.refreshCanvasBytes();

    // Mozilla now owns the expensive page-canvas lifecycle. Rustnote reuses
    // those completed pixels to seed its shared thumbnail/fast-scroll cache.
    if (activePreviewRender?.pageNumber === view.pageNumber) cancelActivePreviewRender();
    seedPreviewFromPageView(view);
    view.hideCachedPreview();
    view.setRenderState('raster-ready');
    updateNativeVisibleAreas();
    const requiresDetail = Boolean(nativePageView.detailView);
    view.requiresCriticalDetail = requiresDetail && pageIsVisible(view.pageNumber);

    if (!hostRenderedNotified) {
      hostRenderedNotified = true;
      notifyHost('rendered');
    }

    // Text selection and Rustnote sidecars remain ours, but they no longer hold
    // the one foreground raster lane. The next visible page can start as soon
    // as this readable base canvas exists.
    const enhancementPage = page;
    page = null;
    view.enhancementPromise = finishPageEnhancements(
      view,
      enhancementPage,
      viewport,
      generation,
      token,
    ).catch(error => {
      if (error?.name !== 'AbortException' && error?.name !== 'RenderingCancelledException') {
        console.error('Rustnote PDF page enhancement failed', error);
      }
      return false;
    });
    return true;
  } catch (error) {
    if (error?.name === 'RenderingCancelledException' || error?.name === 'AbortException') {
      return false;
    }
    console.error('Rustnote PDF canvas render failed', error);
    const detail = errorDetail(error);
    view.setRenderState('error', 'This page could not be rendered.');
    notifyHost('page-error', `Page ${view.pageNumber}: ${detail}`);
    return false;
  } finally {
    if (page) releasePdfPage(view.pageNumber, page);
    if (view.pageNumber === currentPage) updateControls();
  }
}


function pageSearchMatches(pageNumber) {
  return searchMatches.filter(match => match.pageNumber === pageNumber);
}

function renderSearchHighlights(view) {
  if (!view?.searchLayer) return;
  view.searchLayer.replaceChildren();
  if (elements.searchPanel.hidden || !lastSearchQuery || !view.textLayerAvailable || !view.textLayerElement) return;

  const pageMatches = pageSearchMatches(view.pageNumber);
  if (pageMatches.length === 0) return;
  const currentMatch = searchMatches[searchMatchIndex] ?? null;
  let visibleMatches = pageMatches.slice(0, MAX_SEARCH_HIGHLIGHTS_PER_PAGE);
  if (currentMatch?.pageNumber === view.pageNumber && pageMatches.length > MAX_SEARCH_HIGHLIGHTS_PER_PAGE) {
    const currentIndex = Math.max(0, pageMatches.findIndex(match => match === currentMatch));
    const halfWindow = Math.floor(MAX_SEARCH_HIGHLIGHTS_PER_PAGE / 2);
    const start = Math.max(0, Math.min(
      pageMatches.length - MAX_SEARCH_HIGHLIGHTS_PER_PAGE,
      currentIndex - halfWindow,
    ));
    visibleMatches = pageMatches.slice(start, start + MAX_SEARCH_HIGHLIGHTS_PER_PAGE);
  }
  const wanted = new Set(visibleMatches.map(match => match.occurrence));
  if (currentMatch?.pageNumber === view.pageNumber) wanted.add(currentMatch.occurrence);

  const map = textLayerSearchMap(view);
  const searchable = map.text.toLowerCase();
  const shellRect = view.shell.getBoundingClientRect();
  let offset = 0;
  let occurrence = 0;
  let renderedRects = 0;
  while (offset < searchable.length && wanted.size > 0 && renderedRects < MAX_SEARCH_HIGHLIGHT_RECTS_PER_PAGE) {
    const found = searchable.indexOf(lastSearchQuery, offset);
    if (found < 0) break;
    if (wanted.has(occurrence)) {
      const range = searchRangeForOffsets(map, found, found + lastSearchQuery.length);
      for (const rect of range?.getClientRects?.() ?? []) {
        if (renderedRects >= MAX_SEARCH_HIGHLIGHT_RECTS_PER_PAGE) break;
        if (rect.width <= 0.5 || rect.height <= 0.5) continue;
        const mark = document.createElement('span');
        mark.className = 'search-hit';
        mark.dataset.searchOccurrence = String(occurrence);
        if (currentMatch?.pageNumber === view.pageNumber && currentMatch.occurrence === occurrence) {
          mark.classList.add('is-current');
        }
        mark.style.left = `${Math.max(0, rect.left - shellRect.left)}px`;
        mark.style.top = `${Math.max(0, rect.top - shellRect.top)}px`;
        mark.style.width = `${Math.min(shellRect.width, rect.width)}px`;
        mark.style.height = `${Math.min(shellRect.height, rect.height)}px`;
        view.searchLayer.append(mark);
        renderedRects += 1;
      }
      wanted.delete(occurrence);
    }
    occurrence += 1;
    offset = found + Math.max(1, lastSearchQuery.length);
  }
}

function refreshRenderedSearchHighlights() {
  if (!pdf) return;
  const start = Math.max(1, currentPage - MAX_RENDER_CANDIDATE_DISTANCE);
  const end = Math.min(pdf.numPages, currentPage + MAX_RENDER_CANDIDATE_DISTANCE);
  for (let pageNumber = start; pageNumber <= end; pageNumber += 1) {
    const view = pageViewForNumber(pageNumber);
    if (view?.searchLayer && view.textLayerAvailable && view.renderedScale === scale) {
      renderSearchHighlights(view);
    }
  }
}

async function yieldSearchToForeground(serial, yieldFrame = false) {
  let waitedForRender = false;
  while (serial === searchSerial && (activePageRenders > 0 || renderQueue.length > 0 || activePreviewRender)) {
    waitedForRender = true;
    await new Promise(resolve => setTimeout(resolve, 16));
  }
  if (serial === searchSerial && yieldFrame && !waitedForRender) {
    await new Promise(resolve => requestAnimationFrame(() => resolve()));
  }
}

async function pageSearchText(pageNumber, serial) {
  if (pageTextCache.has(pageNumber)) return pageTextCache.get(pageNumber);
  if (searchTextBytes >= MAX_SEARCH_TEXT_BYTES) {
    searchResultsTruncated = true;
    return null;
  }

  const page = await acquirePdfPage(pageNumber);
  let reader = null;
  try {
    const stream = page.streamTextContent({includeMarkedContent: true, disableNormalization: true});
    reader = stream.getReader();
    const state = {text: '', segments: []};
    let capped = false;
    while (serial === searchSerial) {
      await yieldSearchToForeground(serial);
      if (serial !== searchSerial) break;
      const {value, done} = await reader.read();
      if (done) break;
      for (const item of value?.items ?? []) {
        if (typeof item?.str !== 'string') continue;
        appendSearchFragment(state, item.str);
        if (state.text.length >= MAX_SEARCH_TEXT_CHARS_PER_PAGE) {
          capped = true;
          break;
        }
      }
      if (capped) {
        searchResultsTruncated = true;
        await reader.cancel();
        break;
      }
    }
    if (serial !== searchSerial) {
      await reader.cancel().catch(() => undefined);
      return null;
    }

    const text = state.text.slice(0, MAX_SEARCH_TEXT_CHARS_PER_PAGE);
    const byteLength = new TextEncoder().encode(text).byteLength;
    if (searchTextBytes + byteLength > MAX_SEARCH_TEXT_BYTES) {
      searchResultsTruncated = true;
      return null;
    }
    searchTextBytes += byteLength;
    pageTextCache.set(pageNumber, text);
    return text;
  } finally {
    try { reader?.releaseLock?.(); } catch { /* already released */ }
    const view = pageViewForNumber(pageNumber);
    if (!view || view.renderState !== 'rendering') releasePdfPage(pageNumber, page);
  }
}

async function runSearch() {
  if (!pdf) return;
  const query = elements.search.value.trim().toLowerCase();
  clearSearchErrorStatus();
  const serial = ++searchSerial;
  lastSearchQuery = query;
  searchMatches = [];
  searchMatchIndex = -1;
  searchResultsTruncated = false;
  searchRunning = Boolean(query);
  if (searchRunning) {
    cancelBackgroundPreviewRender();
    prunePreviewRenderQueue();
  }
  elements.searchCount.textContent = '';
  elements.searchCount.title = '';
  clearRenderedSearchHighlights();
  updateControls();
  if (!query) {
    searchRunning = false;
    return;
  }

  elements.searchCount.textContent = '…';
  elements.searchCount.title = 'Searching';
  try {
    outer: for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
      if (serial !== searchSerial) return;
      await yieldSearchToForeground(serial, true);
      if (serial !== searchSerial) return;
      const pageText = await pageSearchText(pageNumber, serial);
      if (serial !== searchSerial) return;
      if (pageText === null) break;
      const text = pageText.toLowerCase();
      let offset = 0;
      let occurrence = 0;
      while (offset < text.length) {
        const found = text.indexOf(query, offset);
        if (found < 0) break;
        if (searchMatches.length >= MAX_SEARCH_MATCHES) {
          searchResultsTruncated = true;
          break outer;
        }
        searchMatches.push({pageNumber, offset: found, occurrence});
        occurrence += 1;
        offset = found + Math.max(1, query.length);
      }
    }
    if (serial !== searchSerial) return;
    searchRunning = false;
    if (searchMatches.length === 0) {
      elements.searchCount.textContent = searchResultsTruncated ? '0+' : '0';
      elements.searchCount.title = searchResultsTruncated
        ? 'No matches found within the bounded search cache'
        : 'No matches found';
    } else {
      searchMatchIndex = 0;
      elements.searchCount.title = searchResultsTruncated
        ? 'Result count is bounded to protect memory'
        : 'Search results';
      await showSearchMatch();
      if (serial !== searchSerial) return;
    }
  } catch (error) {
    if (serial !== searchSerial) return;
    searchRunning = false;
    console.error('Rustnote PDF search failed', error);
    setStatus('Search could not be completed for this PDF.', true);
  } finally {
    if (serial === searchSerial) searchRunning = false;
    updateControls();
    pumpRenderScheduler();
  }
}

function searchCountLabel() {
  if (searchMatches.length === 0) return searchResultsTruncated ? '0+' : '0';
  const suffix = searchResultsTruncated ? '+' : '';
  return `${searchMatchIndex + 1} / ${searchMatches.length}${suffix}`;
}

function scrollCurrentSearchHitIntoView(view, occurrence) {
  const hit = view?.searchLayer?.querySelector(`[data-search-occurrence="${occurrence}"]`);
  if (!hit) return false;
  const rect = hit.getBoundingClientRect();
  const deltaY = rect.top + rect.height / 2 - viewportContentCenterY();
  if (Number.isFinite(deltaY) && Math.abs(deltaY) > 1) window.scrollBy(0, deltaY);

  const canvasRect = elements.canvasShell.getBoundingClientRect();
  const visibleLeft = Math.max(0, canvasRect.left) + 16;
  const visibleRight = Math.min(window.innerWidth, canvasRect.right) - 16;
  if (rect.left < visibleLeft) elements.canvasShell.scrollLeft -= visibleLeft - rect.left;
  else if (rect.right > visibleRight) elements.canvasShell.scrollLeft += rect.right - visibleRight;
  scheduleViewportUpdate();
  return true;
}

async function showSearchMatch() {
  if (searchMatches.length === 0) return;
  searchMatchIndex = (searchMatchIndex + searchMatches.length) % searchMatches.length;
  const match = searchMatches[searchMatchIndex];
  elements.searchCount.textContent = searchCountLabel();
  const serial = searchSerial;

  goToPage(match.pageNumber);
  const view = pageViewForNumber(match.pageNumber);
  if (!view) return;
  await awaitPageReady(view);
  if (serial !== searchSerial || searchMatches[searchMatchIndex] !== match) return;
  refreshRenderedSearchHighlights();
  scrollCurrentSearchHitIntoView(view, match.occurrence);
}

function stepSearch(delta) {
  if (searchMatches.length === 0) return;
  searchMatchIndex += delta;
  void showSearchMatch();
}

function startVector(view, event) {
  if (READ_ONLY || !['pen', 'shape'].includes(activeTool) || event.button !== 0) return;
  if (event.target.closest('textarea')) return;
  if (view.renderedScale !== scale) return;
  event.preventDefault();
  setCurrentPage(view.pageNumber);
  view.shell.setPointerCapture?.(event.pointerId);
  const now = Date.now();
  const point = normalizedPoint(view, event.clientX, event.clientY);
  currentVector = activeTool === 'pen'
    ? {
        pageNumber: view.pageNumber,
        id: annotationId(), kind: 'ink', createdAt: now, updatedAt: now,
        points: [point], width: STROKE_WIDTHS[penWidth] ?? STROKE_WIDTHS.medium, color: penColor,
      }
    : {
        pageNumber: view.pageNumber,
        id: annotationId(), kind: 'shape', createdAt: now, updatedAt: now,
        shape: shapeType, points: [point, point],
        width: STROKE_WIDTHS[shapeWidth] ?? STROKE_WIDTHS.medium, color: shapeColor,
      };
  renderVectorLayer(view, pageAnnotations(view.pageNumber), currentVector);
}

function moveVector(view, event) {
  if (READ_ONLY || !currentVector || currentVector.pageNumber !== view.pageNumber) return;
  if ((currentVector.kind === 'ink' && activeTool !== 'pen')
      || (currentVector.kind === 'shape' && activeTool !== 'shape')) return;
  event.preventDefault();
  const point = normalizedPoint(view, event.clientX, event.clientY);
  if (currentVector.kind === 'ink') {
    const previous = currentVector.points.at(-1);
    if (!previous || Math.hypot(point[0] - previous[0], point[1] - previous[1]) > 0.001) {
      if (currentVector.points.length < 10_000) currentVector.points.push(point);
    }
  } else {
    currentVector.points[1] = point;
  }
  currentVector.updatedAt = Date.now();
  renderVectorLayer(view, pageAnnotations(view.pageNumber), currentVector);
}

function finishVector(view, event) {
  if (READ_ONLY) { currentVector = null; return; }
  if (!currentVector || currentVector.pageNumber !== view.pageNumber) return;
  view.shell.releasePointerCapture?.(event.pointerId);
  const annotation = currentVector;
  currentVector = null;
  const points = annotation.points ?? [];
  const shapeDx = points.length === 2 ? Math.abs(points[1][0] - points[0][0]) : 0;
  const shapeDy = points.length === 2 ? Math.abs(points[1][1] - points[0][1]) : 0;
  const usable = annotation.kind === 'ink'
    ? points.length >= 2
    : annotation.shape === 'line'
      ? points.length === 2 && Math.hypot(shapeDx, shapeDy) >= 0.002
      : points.length === 2 && shapeDx >= 0.002 && shapeDy >= 0.002;
  if (!usable) {
    renderVectorLayer(view, pageAnnotations(view.pageNumber));
    return;
  }
  delete annotation.pageNumber;
  const before = cloneAnnotations(pageAnnotations(view.pageNumber));
  pageAnnotations(view.pageNumber).push(annotation);
  commitPageMutation(view.pageNumber, before);
}

function resetTextSelectionState() {
  for (const view of mountedPageViews.values()) {
    const layer = view.textLayerElement;
    if (!layer) continue;
    layer.classList.remove('selecting');
    const endOfContent = layer.querySelector('.endOfContent');
    if (!endOfContent) continue;
    endOfContent.style.width = '';
    endOfContent.style.height = '';
    endOfContent.style.userSelect = '';
    if (endOfContent.parentElement !== layer || endOfContent !== layer.lastElementChild) {
      layer.append(endOfContent);
    }
  }
}

function pageViewFromEventTarget(target) {
  const shell = target?.closest?.('.page-shell');
  return shell?.dataset.pageNumber ? pageViewForNumber(Number(shell.dataset.pageNumber)) : null;
}

elements.pages.addEventListener('mousedown', event => {
  const layer = event.target.closest?.('.textLayer');
  if (!layer) return;
  layer.classList.add('selecting');
  const view = pageViewFromEventTarget(event.target);
  if (view) setCurrentPage(view.pageNumber);
});

document.addEventListener('pointerup', resetTextSelectionState);
window.addEventListener('blur', resetTextSelectionState);
document.addEventListener('selectionchange', () => {
  const selection = document.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
    resetTextSelectionState();
    hideSelectionActions();
    return;
  }
  const selectedView = selectedPageView();
  for (const view of mountedPageViews.values()) {
    const layer = view.textLayerElement;
    if (!layer) continue;
    layer.classList.toggle('selecting', selectedView?.pageNumber === view.pageNumber);
  }
  updateSelectionActions();
});

elements.searchToggle.addEventListener('click', () => {
  setSearchOpen(elements.searchPanel.hidden, true);
});
elements.sidebarToggle.addEventListener('click', () => {
  setSidebarOpen(!sidebarOpen);
});
elements.sidebarThumbnailsTab.addEventListener('click', () => setSidebarMode('thumbnails'));
elements.sidebarOutlineTab.addEventListener('click', () => setSidebarMode('outline'));
elements.sidebarAnnotationsTab.addEventListener('click', () => setSidebarMode('annotations'));
elements.thumbnailViewport.addEventListener('scroll', scheduleThumbnailVirtualization, {passive: true});
elements.annotationViewport.addEventListener('scroll', scheduleAnnotationVirtualization, {passive: true});
elements.annotationPrevious.addEventListener('click', () => navigateAnnotation(-1));
elements.annotationNext.addEventListener('click', () => navigateAnnotation(1));
for (const button of [elements.selectionHighlight, elements.selectionCopy, elements.selectionNote]) {
  button.addEventListener('pointerdown', event => event.preventDefault());
}
elements.selectionHighlight.addEventListener('click', () => {
  if (createTextMarkFromSelection('highlight')) hideSelectionActions();
});
elements.selectionCopy.addEventListener('click', () => void copySelectedText());
elements.selectionNote.addEventListener('click', () => { void createNoteFromSelection(); });

elements.markupToggle.addEventListener('click', () => {
  setMarkupOpen(elements.markupToolbar.hidden);
});

for (const button of [
  elements.toolSelect,
  elements.toolHighlight,
  elements.toolUnderline,
  elements.toolStrike,
  elements.toolNote,
  elements.toolPen,
  elements.toolShape,
  elements.toolText,
  elements.toolErase,
]) {
  button.addEventListener('pointerdown', event => event.preventDefault());
}
for (const button of elements.markupOptions.querySelectorAll('[data-option-group]')) {
  button.addEventListener('pointerdown', event => event.preventDefault());
  button.addEventListener('click', () => applyMarkupOption(button));
}

elements.toolSelect.addEventListener('click', () => setTool('select'));
for (const [button, tool] of [
  [elements.toolHighlight, 'highlight'],
  [elements.toolUnderline, 'underline'],
  [elements.toolStrike, 'strike'],
]) {
  button.addEventListener('click', () => {
    if (!createTextMarkFromSelection(tool)) setTool(tool);
  });
}
elements.toolNote.addEventListener('click', () => setTool('note'));
elements.toolPen.addEventListener('click', () => setTool('pen'));
elements.toolShape.addEventListener('click', () => setTool('shape'));
elements.toolText.addEventListener('click', () => setTool('text'));
elements.toolErase.addEventListener('click', () => setTool('erase'));
elements.annotationUndo.addEventListener('click', () => void undoAnnotation());
elements.annotationRedo.addEventListener('click', () => void redoAnnotation());

for (const [group, value] of [
  ['text-mark-color', textMarkColor],
  ['note-color', noteColor],
  ['note-size', noteSize],
  ['pen-color', penColor],
  ['pen-width', penWidth],
  ['shape-type', shapeType],
  ['shape-color', shapeColor],
  ['shape-width', shapeWidth],
  ['text-color', textColor],
  ['text-size', textSize],
]) setPressedOption(group, value);
setMarkupOptionGroup(activeTool);
setFocusActive(false);

elements.pages.addEventListener('mouseup', event => {
  const view = pageViewFromEventTarget(event.target);
  if (!view) return;
  setCurrentPage(view.pageNumber);
  if (TEXT_MARK_TOOLS.has(activeTool)) {
    if (createTextMarkFromSelection(activeTool)) setTool('select');
  } else if (activeTool === 'note' && !event.target.closest('textarea, button')) {
    createNote(view, event.clientX, event.clientY);
    setTool('select');
  } else if (activeTool === 'text' && !event.target.closest('textarea, button')) {
    createTextBox(view, event.clientX, event.clientY);
    setTool('select');
  }
});
elements.pages.addEventListener('pointerdown', event => {
  const view = pageViewFromEventTarget(event.target);
  if (view) startVector(view, event);
});
elements.pages.addEventListener('pointermove', event => {
  const view = currentVector ? pageViewForNumber(currentVector.pageNumber) : pageViewFromEventTarget(event.target);
  if (view) moveVector(view, event);
});
elements.pages.addEventListener('pointerup', event => {
  const view = currentVector ? pageViewForNumber(currentVector.pageNumber) : pageViewFromEventTarget(event.target);
  if (view) finishVector(view, event);
});
elements.pages.addEventListener('pointercancel', event => {
  const view = currentVector ? pageViewForNumber(currentVector.pageNumber) : pageViewFromEventTarget(event.target);
  if (view) finishVector(view, event);
});

function setFocusActive(active) {
  focusActive = Boolean(active);
  elements.focusPdf.hidden = focusActive;
  elements.focusPdf.classList.toggle('is-active', focusActive);
  elements.focusPdf.setAttribute('aria-pressed', focusActive ? 'true' : 'false');
  elements.focusActions.hidden = !focusActive;
  requestAnimationFrame(syncViewerChromeGeometry);
}

function zoomByStep(direction, anchor = captureScrollAnchor()) {
  fitMode = null;
  fitOperationSerial += 1;
  applyScale(Number((scale + direction * SCALE_STEP).toFixed(2)), anchor);
}

function zoomToActualSize(anchor = captureScrollAnchor()) {
  fitMode = null;
  fitOperationSerial += 1;
  applyScale(1, anchor);
}

if (typeof ResizeObserver === 'function') {
  const chromeResizeObserver = new ResizeObserver(() => syncViewerChromeGeometry());
  chromeResizeObserver.observe(elements.toolbar);
  chromeResizeObserver.observe(elements.markupToolbar);
}

elements.previous.addEventListener('click', () => goToPage(currentPage - 1));
elements.next.addEventListener('click', () => goToPage(currentPage + 1));
elements.pageNumber.addEventListener('change', () => goToPage(elements.pageNumber.value));
elements.pageNumber.addEventListener('keydown', event => {
  if (event.key === 'Enter') {
    event.preventDefault();
    goToPage(elements.pageNumber.value);
    elements.pageNumber.blur();
  }
});
elements.zoomOut.addEventListener('click', () => zoomByStep(-1));
elements.zoomIn.addEventListener('click', () => zoomByStep(1));
elements.focusPdf.addEventListener('click', () => notifyHost('focus-toggle'));
elements.windowFullscreen.addEventListener('click', () => notifyHost('window-fullscreen-toggle'));
elements.exitFocus.addEventListener('click', () => notifyHost('focus-exit'));
elements.exportPdf.addEventListener('click', () => void requestAnnotatedPdfExport());
elements.fitWidth.addEventListener('click', () => void fitWidth());
elements.fitPage.addEventListener('click', () => void fitPage());

elements.search.addEventListener('input', () => {
  const query = elements.search.value.trim().toLowerCase();
  if (query === lastSearchQuery) return;
  if (searchRunning) cancelSearchWork(true);
  else {
    searchMatches = [];
    searchMatchIndex = -1;
    searchResultsTruncated = false;
    elements.searchCount.textContent = '';
    elements.searchCount.title = '';
  }
  lastSearchQuery = '';
  clearRenderedSearchHighlights();
  updateControls();
});

elements.search.addEventListener('keydown', event => {
  if (event.key === 'Enter') {
    event.preventDefault();
    const query = elements.search.value.trim().toLowerCase();
    if (searchMatches.length > 0 && query && query === lastSearchQuery) stepSearch(event.shiftKey ? -1 : 1);
    else void runSearch();
  }
});
elements.search.addEventListener('search', () => void runSearch());
elements.searchPrevious.addEventListener('click', () => stepSearch(-1));
elements.searchNext.addEventListener('click', () => stepSearch(1));

window.addEventListener('message', event => {
  if (event.source !== window.parent) return;
  const message = event.data;
  if (!message || message.type !== 'rustnote-pdf-host') return;
  if (message.stage === 'focus-state') {
    setFocusActive(message.active === true);
    return;
  }
  if (message.stage === 'export-complete') {
    completeAnnotatedPdfExport(message.requestId, message.success === true, String(message.message || ''));
  }
});

document.addEventListener('keydown', event => {
  const modifier = event.metaKey || event.ctrlKey;
  if (modifier && !event.altKey) {
    if (event.key === '+' || event.key === '=') {
      event.preventDefault();
      zoomByStep(1);
      return;
    }
    if (event.key === '-' || event.key === '_') {
      event.preventDefault();
      zoomByStep(-1);
      return;
    }
    if (event.key === '0') {
      event.preventDefault();
      zoomToActualSize();
      return;
    }
  }
  if (modifier && event.key.toLowerCase() === 'f') {
    event.preventDefault();
    setSearchOpen(true, true);
  }
  if (event.key === 'Escape') {
    if (!elements.selectionActions.hidden) {
      event.preventDefault();
      window.getSelection()?.removeAllRanges();
      hideSelectionActions();
      return;
    }
    if (!elements.searchPanel.hidden) {
      event.preventDefault();
      setSearchOpen(false);
      elements.searchToggle.focus();
      return;
    }
    if (!elements.markupToolbar.hidden) {
      event.preventDefault();
      setMarkupOpen(false);
      elements.markupToggle.focus();
      return;
    }
    if (focusActive) {
      event.preventDefault();
      notifyHost('focus-exit');
      return;
    }
  }
  if (!READ_ONLY && modifier && event.key.toLowerCase() === 'z' && !event.target.closest('textarea, input')) {
    event.preventDefault();
    if (event.shiftKey) void redoAnnotation(); else void undoAnnotation();
  }
});

elements.canvasShell.addEventListener('scroll', () => {
  hideSelectionActions();
  scheduleViewportUpdate();
}, {passive: true});

window.addEventListener('scroll', () => {
  hideSelectionActions();
  cancelBackgroundPreviewRender();
  const nextY = window.scrollY;
  if (geometryScrollCompensationY != null && Math.abs(nextY - geometryScrollCompensationY) <= 1) {
    geometryScrollCompensationY = null;
  } else {
    geometryScrollCompensationY = null;
    if (Math.abs(nextY - lastScrollY) > 1) scrollDirection = nextY > lastScrollY ? 1 : -1;
  }
  lastScrollY = nextY;
  scrollSettled = false;
  cancelActiveDetailRender();
  prunePreviewRenderQueue();
  clearTimeout(scrollSettleTimer);
  scrollSettleTimer = setTimeout(() => {
    scrollSettled = true;
    renderAroundCurrentPage();
  }, SCROLL_SETTLE_MS);
  scheduleViewportUpdate();
}, {passive: true});
window.addEventListener('resize', () => {
  hideSelectionActions();
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    syncViewerChromeGeometry();
    if (fitMode) void refitCurrentMode();
    else scheduleViewportUpdate();
  }, RESIZE_DEBOUNCE_MS);
});

async function start() {
  attachmentId = attachmentIdFromViewerParameters();
  if (!attachmentId) {
    setStatus('Invalid PDF reference.', true);
    notifyHost('error', 'Invalid PDF reference');
    return;
  }
  setSearchOpen(false);
  setMarkupOpen(false);
  setSidebarOpen(false);
  updateControls();
  try {
    notifyHost('module-ready', `WebKit-targeted bundled PDF.js runtime — ${pdfjsLib.version} ${PDF_RUNTIME_PROFILE}`);
    if (ENABLE_PDF_SCRIPTING) throw new Error('PDF scripting must remain disabled');
    const encodedAttachmentId = encodeURIComponent(attachmentId);
    const documentUrl = `/rustnotepdf/document/${encodedAttachmentId}`;
    notifyHost('metadata');
    const metadata = await pdfDocumentMetadata(`/rustnotepdf/metadata/${encodedAttachmentId}`);
    const {length} = metadata;
    pdfSessionReleaseUrl = length > metadata.wholeFileMaxBytes
      ? `/rustnotepdf/session/${encodedAttachmentId}`
      : '';
    let task = null;
    const commonDocumentOptions = {
      isEvalSupported: false,
      enableXfa: false,
      useWasm: true,
      wasmUrl: PDFJS_WASM_URL,
      standardFontDataUrl: PDFJS_STANDARD_FONT_DATA_URL,
      // Keep built-in resource fetches on the viewer side. Rustnote's worker
      // response intentionally has a no-network CSP, while the viewer's
      // srcdoc permits same-origin requests through the private PDF protocol.
      useWorkerFetch: false,
      maxImageSize: PDFJS_MAX_SOURCE_IMAGE_PIXELS,
      canvasMaxAreaInBytes: PDFJS_IMAGE_CANVAS_MAX_AREA_BYTES,
    };
    const preparedTransport = await preparePdfDocumentTransport(
      documentUrl,
      metadata,
      error => {
        console.error('Rustnote PDF range load failed', error);
        reportPdfPreviewFailure(error);
        Promise.resolve().then(() => {
          void task?.destroy().catch(destroyError => {
            console.error('Rustnote PDF shutdown after range failure failed', destroyError);
          });
        });
      },
    );
    activePdfRangeTransport = preparedTransport.range;
    notifyHost('transport-ready', preparedTransport.detail);
    task = getDocument({
      ...preparedTransport.documentOptions,
      ...commonDocumentOptions,
    });
    pdf = await task.promise;
    notifyHost('document-ready', `${pdf.numPages} page${pdf.numPages === 1 ? '' : 's'}`);

    const firstPage = await pdf.getPage(1);
    try {
      const firstViewport = firstPage.getViewport({scale: 1});
      defaultPageMetrics = {width: firstViewport.width, height: firstViewport.height};
    } finally {
      firstPage.cleanup();
    }

    currentPage = 1;
    initializePageGeometry();
    elements.canvasShell.hidden = false;
    setSidebarOpen(defaultSidebarOpenForPageCount(pdf.numPages), {refit: false});
    syncViewerChromeGeometry();
    updatePageVirtualWindow([1]);
    pageViewForNumber(1)?.shell.classList.add('is-current');
    setStatus('');
    setTool('select');
    const rendered = await fitWidth();
    if (rendered && !hostRenderedNotified) {
      hostRenderedNotified = true;
      notifyHost('rendered');
    }
    renderAroundCurrentPage();
  } catch (error) {
    console.error('Rustnote PDF loading failed', error);
    reportPdfPreviewFailure(error);
  } finally {
    updateControls();
  }
}

window.addEventListener('pagehide', () => {
  activePdfRangeTransport?.abort();
  activePdfRangeTransport = null;
  if (pdfSessionReleaseUrl) {
    void fetch(pdfSessionReleaseUrl, {
      method: 'DELETE',
      cache: 'no-store',
      credentials: 'omit',
      keepalive: true,
    }).catch(() => {});
    pdfSessionReleaseUrl = '';
  }
  renderGeneration += 1;
  previewGeneration += 1;
  searchSerial += 1;
  clearThumbnailVirtualWindow();
  cancelActivePreviewRender();
  for (const item of previewRenderQueue) item.resolve(false);
  previewRenderQueue = [];
  previewQueuedPages.clear();
  clearPreviewCache();
  previewFailedPages.clear();
  if (scrollFrame) cancelAnimationFrame(scrollFrame);
  clearTimeout(scrollSettleTimer);
  clearTimeout(resizeTimer);
  for (const timer of textAnnotationSaveTimers.values()) clearTimeout(timer);
  if (!READ_ONLY) for (const pageNumber of textAnnotationSaveTimers.keys()) void queueAnnotationSave(pageNumber);
  textAnnotationSaveTimers.clear();
  for (const pageNumber of [...mountedPageViews.keys()]) unmountPageView(pageNumber);
  pageGeometries.length = 1;
  pageHeightIndex = null;
  maxBasePageWidth = 1;
  pdf?.destroy();
});

void start();

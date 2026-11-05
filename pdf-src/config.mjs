// Shared immutable viewer configuration. Build-time constants are injected by esbuild.
// These two build-time constants are injected by the PDF runtime bundler.
export const PDF_RUNTIME_PROFILE = __RUSTNOTE_PDF_RUNTIME_PROFILE__;
export const PDF_RUNTIME_BASE_URL = __RUSTNOTE_PDF_RUNTIME_BASE_URL__;
export const PDFJS_WORKER_URL = `${PDF_RUNTIME_BASE_URL}/pdf.worker.mjs`;
export const PDFJS_STANDARD_FONT_DATA_URL = `${PDF_RUNTIME_BASE_URL}/standard_fonts/`;
export const PDFJS_WASM_URL = `${PDF_RUNTIME_BASE_URL}/wasm/`;

export const MIN_SCALE = 0.25;
export const MAX_SCALE = 3;
export const SCALE_STEP = 0.25;
export const PDFJS_BASE_CANVAS_MAX_PIXELS = 32_000_000;
// Source-image safety is deliberately separate from the page-canvas ceiling.
// 64 Mi-pixels permits common 600 dpi scans (and A4 around 800 dpi) while
// bounding a single decoded RGBA image to about 256 MiB. PDF.js defaults this
// limit to unlimited, which is inappropriate for untrusted local documents.
export const PDFJS_MAX_SOURCE_IMAGE_PIXELS = 64 * 1024 * 1024;
export const PDFJS_IMAGE_CANVAS_MAX_AREA_BYTES = PDFJS_MAX_SOURCE_IMAGE_PIXELS * 4;
export const PDFJS_MIN_CANVAS_UPDATE_MS = 160;
export const DEFAULT_PDF_TRANSPORT_BLOCK_BYTES = 1024 * 1024;
export const MAX_CONCURRENT_PDF_BLOCK_FETCHES = 4;
export const PDF_TRANSPORT_CACHE_BUDGET_BYTES = 32 * 1024 * 1024;
export const PDF_TRANSPORT_READ_AHEAD_DELAY_MS = 24;
export const MAX_PDF_TRANSPORT_REQUEST_BYTES = 64 * 1024 * 1024;
export const MAX_CONCURRENT_PAGE_RENDERS = 1;
export const MAX_RENDER_CANDIDATE_DISTANCE = 6;
export const ADAPTIVE_RETENTION_MAX_DISTANCE = 6;
export const PAGE_READ_AHEAD_FORWARD = 2;
export const PAGE_GAP_PX = 18;
export const PAGE_DOM_OVERSCAN_VIEWPORTS = 1.5;
export const PAGE_DOM_MIN_OVERSCAN_PX = 900;
export const MAX_MOUNTED_PAGE_VIEWS = 64;
export const CANVAS_MEMORY_MIN_OPERATING_BYTES = 64 * 1024 * 1024;
// Hard limit for Rustnote-owned PDF raster surfaces: base/detail page canvases,
// shared preview cache, mounted page-preview copies, and visible thumbnail
// canvases. Transport buffers and PDF.js worker internals are separate bounded
// subsystems with their own limits.
export const CANVAS_MEMORY_HARD_LIMIT_BYTES = 256 * 1024 * 1024;
export const RASTER_BYTES_PER_PIXEL = 4;
export const RESTRICTED_PAGE_BASE_AREA_FACTOR = 1 / 16;
export const SCROLL_SETTLE_MS = 140;
export const THUMBNAIL_ROW_HEIGHT = 182;
export const THUMBNAIL_MAX_DISPLAY_WIDTH = 132;
export const THUMBNAIL_MAX_DISPLAY_HEIGHT = 150;
export const THUMBNAIL_OVERSCAN_ROWS = 3;
export const PREVIEW_WIDTH = 288;
export const PREVIEW_MAX_HEIGHT = 420;
export const MAX_PREVIEW_PIXELS = 180_000;
export const PREVIEW_CACHE_MIN_BUDGET_BYTES = 8 * 1024 * 1024;
export const PREVIEW_CACHE_HARD_LIMIT_BYTES = 32 * 1024 * 1024;
export const MAX_OUTLINE_ITEMS = 2_000;
export const ANNOTATION_ROW_HEIGHT = 64;
export const ANNOTATION_OVERSCAN_ROWS = 4;
export const MAX_ANNOTATION_INDEX_ENTRIES = 5_000;
export const MAX_SEARCH_MATCHES = 5_000;
export const MAX_SEARCH_HIGHLIGHTS_PER_PAGE = 240;
export const MAX_SEARCH_HIGHLIGHT_RECTS_PER_PAGE = 720;
export const MAX_SEARCH_TEXT_BYTES = 20 * 1024 * 1024;
export const MAX_SEARCH_TEXT_CHARS_PER_PAGE = 1_048_576;
export const TEXT_ANNOTATION_SAVE_DELAY_MS = 250;
export const RESIZE_DEBOUNCE_MS = 120;
// Security invariant shared by every Rustnote PDF.js runtime profile.
export const ENABLE_PDF_SCRIPTING = false;
export const PORTABLE_ANNOTATION_TYPE = Object.freeze({FREETEXT: 3, HIGHLIGHT: 9, INK: 15});

// Contextual markup settings are session-local defaults for newly created annotations.
export const STROKE_WIDTHS = Object.freeze({thin: 0.0015, medium: 0.003, thick: 0.006});
export const NOTE_SIZES = Object.freeze({
  small: [0.20, 0.09],
  medium: [0.27, 0.13],
  large: [0.36, 0.18],
});
export const TEXT_BOX_SIZES = Object.freeze({
  small: [0.26, 0.055],
  medium: [0.31, 0.075],
  large: [0.36, 0.095],
});
export const TEXT_FONT_SIZES = Object.freeze({small: 12, medium: 15, large: 19});
export const TEXT_MARK_TOOLS = new Set(['highlight', 'underline', 'strike']);

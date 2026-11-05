import {NodeSelection, TextSelection} from "prosemirror-state";
import {canonicalAttachmentKind, mediaTimeLabelStatic} from "./import.js";
import {attachmentStatus, emitAttachmentCommand} from "../bridge/events.js";
import {MEDIA_ANNOTATION_LABEL_MAX, MEDIA_ANNOTATION_LIMIT, MEDIA_ANNOTATION_NOTE_MAX, MEDIA_ANNOTATION_TAG_MAX, MEDIA_RESUME_PERSIST_INTERVAL_MS, normalizedMediaAnnotation, normalizedMediaAnnotations, readMediaResumePosition, schema, writeMediaResumePosition} from "../document/schema.js";
import {specDomain, specLimit, truncateUtf8} from "../document/spec.js";



function attachmentNodeAttrs(descriptor) {
  const defaults = schema.nodes.attachment.defaultAttrs;
  return {
    ...defaults,
    attachmentId: String(descriptor?.id || defaults.attachmentId).trim(),
    kind: canonicalAttachmentKind(descriptor?.kind),
    displayName: String(descriptor?.file_name || defaults.displayName).trim() || defaults.displayName,
    mediaType: typeof descriptor?.mime_type === 'string' ? descriptor.mime_type : defaults.mediaType,
    byteSize: Number.isSafeInteger(Number(descriptor?.byte_size)) ? Math.max(0, Number(descriptor.byte_size)) : defaults.byteSize,
  };
}

function attachmentSizeLabel(byteSize) {
  const bytes = Number(byteSize);
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const precision = unit === 0 || value >= 10 ? 0 : 1;
  return `${value.toFixed(precision)} ${units[unit]}`;
}

function imageRuntimePixelSize(runtime) {
  const width = Number(runtime && runtime.imagePixelWidth);
  const height = Number(runtime && runtime.imagePixelHeight);
  if (!Number.isSafeInteger(width) || width <= 0 || !Number.isSafeInteger(height) || height <= 0) return null;
  const orientation = Number(runtime && runtime.imageOrientation);
  return [5, 6, 7, 8].includes(orientation)
    ? {width: height, height: width}
    : {width, height};
}

const PDF_VIEWER_SRCDOC_CSP = "default-src 'none'; script-src 'self' dioxus: http://dioxus.index.html https://dioxus.index.html; worker-src 'self' dioxus: http://dioxus.index.html https://dioxus.index.html blob:; connect-src 'self' dioxus: http://dioxus.index.html https://dioxus.index.html; style-src 'self' dioxus: http://dioxus.index.html https://dioxus.index.html; img-src 'self' dioxus: http://dioxus.index.html https://dioxus.index.html data: blob:; font-src data: blob:; object-src 'none'; frame-src 'none'; child-src 'none'; media-src 'none'; base-uri 'none'; form-action 'none'; navigate-to 'none'";

function pdfViewerDocumentHtml(attachmentId, readOnly) {
  const query = `attachment=${encodeURIComponent(attachmentId)}${readOnly ? '&readonly=1' : ''}`;
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="referrer" content="no-referrer">
  <base href="/">
  <meta http-equiv="Content-Security-Policy" content="${PDF_VIEWER_SRCDOC_CSP}">
  <title>Rustnote PDF preview</title>
  <link rel="stylesheet" href="/rustnotepdf/viewer.css">
</head>
<body>
  <main class="viewer-shell" aria-label="PDF preview">
    <div class="viewer-toolbar" role="toolbar" aria-label="PDF controls">
      <div class="toolbar-group sidebar-controls" aria-label="Sidebar controls">
        <button id="sidebar-toggle" class="icon-button sidebar-toggle" type="button" aria-label="Show sidebar" aria-expanded="false" aria-controls="pdf-sidebar" title="Show sidebar">
          <svg viewBox="0 0 20 20" aria-hidden="true"><rect x="3" y="3.5" width="14" height="13" rx="2"></rect><path d="M7.5 4v12"></path></svg>
        </button>
      </div>
      <div class="toolbar-group navigation-group" aria-label="Page navigation">
        <button id="previous" class="icon-button" type="button" aria-label="Previous page" title="Previous page">&lsaquo;</button>
        <label class="page-control"><span class="visually-hidden">Page</span><input id="page-number" type="number" min="1" value="1" inputmode="numeric" aria-label="Page number"><span id="page-count">/ &mdash;</span></label>
        <button id="next" class="icon-button" type="button" aria-label="Next page" title="Next page">&rsaquo;</button>
      </div>
      <span class="toolbar-spacer"></span>
      <div class="toolbar-group reading-tools" aria-label="Reading tools">
        <button id="search-toggle" class="icon-button search-toggle" type="button" aria-label="Find in PDF" aria-expanded="false" title="Find in PDF">
          <svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="8.5" cy="8.5" r="4.75"></circle><path d="m12 12 4 4"></path></svg>
        </button>
        <div id="search-panel" class="toolbar-group search-group" role="search" hidden>
          <input id="search" type="search" placeholder="Find in PDF" autocomplete="off" spellcheck="false" aria-label="Find in PDF">
          <span id="search-count" class="search-count" aria-live="polite"></span>
          <button id="search-previous" class="icon-button" type="button" aria-label="Previous search result" title="Previous result">&uarr;</button>
          <button id="search-next" class="icon-button" type="button" aria-label="Next search result" title="Next result">&darr;</button>
        </div>
        <span class="toolbar-divider" aria-hidden="true"></span>
        <button id="zoom-out" class="icon-button" type="button" aria-label="Zoom out" title="Zoom out">&minus;</button>
        <div class="toolbar-group fit-controls" aria-label="Fit mode">
          <button id="fit-width" class="compact-button fit-button is-active" type="button" aria-label="Fit page width" aria-pressed="true" title="Fit width">Width</button>
          <button id="fit-page" class="compact-button fit-button" type="button" aria-label="Fit whole page" aria-pressed="false" title="Fit page">Page</button>
        </div>
        <button id="zoom-in" class="icon-button" type="button" aria-label="Zoom in" title="Zoom in">+</button>
        <span id="markup-divider" class="toolbar-divider" aria-hidden="true"></span>
        <button id="markup-toggle" class="markup-toggle" type="button" aria-expanded="false" aria-controls="markup-toolbar">Markup</button>
        <span class="toolbar-divider" aria-hidden="true"></span>
        <button id="focus-pdf" class="compact-button" type="button" aria-label="Focus PDF" aria-pressed="false" title="Focus PDF">Focus</button>
        <button id="export-pdf" class="compact-button" type="button" aria-label="Export annotated PDF" title="Export annotated PDF">Export</button>
      </div>
    </div>
    <div id="markup-toolbar" class="markup-toolbar" role="toolbar" aria-label="PDF markup tools" hidden>
      <div class="toolbar-group markup-tools">
        <button id="tool-select" class="tool-button is-active" type="button" aria-pressed="true">Select</button>
        <button id="tool-highlight" class="tool-button" type="button" aria-pressed="false">Highlight</button>
        <button id="tool-underline" class="tool-button" type="button" aria-pressed="false">Underline</button>
        <button id="tool-strike" class="tool-button" type="button" aria-pressed="false">Strike</button>
        <button id="tool-note" class="tool-button" type="button" aria-pressed="false">Note</button>
        <button id="tool-pen" class="tool-button" type="button" aria-pressed="false">Pen</button>
        <button id="tool-shape" class="tool-button" type="button" aria-pressed="false">Shape</button>
        <button id="tool-text" class="tool-button" type="button" aria-pressed="false">Text</button>
        <button id="tool-erase" class="tool-button" type="button" aria-pressed="false">Erase</button>
      </div>
      <div id="markup-options" class="markup-options" aria-label="Markup tool options" hidden>
        <div id="text-mark-options" class="tool-options" hidden>
          <div class="option-set" aria-label="Text mark color">
            <span class="option-label">Color</span>
            <button class="option-button color-option color-yellow" type="button" data-option-group="text-mark-color" data-option-value="yellow" aria-label="Yellow" aria-pressed="true"></button>
            <button class="option-button color-option color-green" type="button" data-option-group="text-mark-color" data-option-value="green" aria-label="Green" aria-pressed="false"></button>
            <button class="option-button color-option color-blue" type="button" data-option-group="text-mark-color" data-option-value="blue" aria-label="Blue" aria-pressed="false"></button>
            <button class="option-button color-option color-pink" type="button" data-option-group="text-mark-color" data-option-value="pink" aria-label="Pink" aria-pressed="false"></button>
          </div>
        </div>
        <div id="note-options" class="tool-options" hidden>
          <div class="option-set" aria-label="Note color">
            <span class="option-label">Color</span>
            <button class="option-button color-option color-yellow" type="button" data-option-group="note-color" data-option-value="yellow" aria-label="Yellow" aria-pressed="true"></button>
            <button class="option-button color-option color-green" type="button" data-option-group="note-color" data-option-value="green" aria-label="Green" aria-pressed="false"></button>
            <button class="option-button color-option color-blue" type="button" data-option-group="note-color" data-option-value="blue" aria-label="Blue" aria-pressed="false"></button>
            <button class="option-button color-option color-pink" type="button" data-option-group="note-color" data-option-value="pink" aria-label="Pink" aria-pressed="false"></button>
          </div>
          <div class="option-set" aria-label="Note size"><span class="option-label">Size</span><button class="option-button" type="button" data-option-group="note-size" data-option-value="small" aria-pressed="false">S</button><button class="option-button is-active" type="button" data-option-group="note-size" data-option-value="medium" aria-pressed="true">M</button><button class="option-button" type="button" data-option-group="note-size" data-option-value="large" aria-pressed="false">L</button></div>
        </div>
        <div id="pen-options" class="tool-options" hidden>
          <div class="option-set" aria-label="Pen color"><span class="option-label">Color</span><button class="option-button color-option color-red" type="button" data-option-group="pen-color" data-option-value="red" aria-label="Red" aria-pressed="true"></button><button class="option-button color-option color-blue" type="button" data-option-group="pen-color" data-option-value="blue" aria-label="Blue" aria-pressed="false"></button><button class="option-button color-option color-black" type="button" data-option-group="pen-color" data-option-value="black" aria-label="Black" aria-pressed="false"></button></div>
          <div class="option-set" aria-label="Pen thickness"><span class="option-label">Width</span><button class="option-button" type="button" data-option-group="pen-width" data-option-value="thin" aria-pressed="false">S</button><button class="option-button is-active" type="button" data-option-group="pen-width" data-option-value="medium" aria-pressed="true">M</button><button class="option-button" type="button" data-option-group="pen-width" data-option-value="thick" aria-pressed="false">L</button></div>
        </div>
        <div id="shape-options" class="tool-options" hidden>
          <div class="option-set" aria-label="Shape type"><span class="option-label">Shape</span><button class="option-button is-active" type="button" data-option-group="shape-type" data-option-value="rectangle" aria-pressed="true">Rect</button><button class="option-button" type="button" data-option-group="shape-type" data-option-value="ellipse" aria-pressed="false">Oval</button><button class="option-button" type="button" data-option-group="shape-type" data-option-value="line" aria-pressed="false">Line</button></div>
          <div class="option-set" aria-label="Shape color"><span class="option-label">Color</span><button class="option-button color-option color-red" type="button" data-option-group="shape-color" data-option-value="red" aria-label="Red" aria-pressed="true"></button><button class="option-button color-option color-blue" type="button" data-option-group="shape-color" data-option-value="blue" aria-label="Blue" aria-pressed="false"></button><button class="option-button color-option color-black" type="button" data-option-group="shape-color" data-option-value="black" aria-label="Black" aria-pressed="false"></button></div>
          <div class="option-set" aria-label="Shape thickness"><span class="option-label">Width</span><button class="option-button" type="button" data-option-group="shape-width" data-option-value="thin" aria-pressed="false">S</button><button class="option-button is-active" type="button" data-option-group="shape-width" data-option-value="medium" aria-pressed="true">M</button><button class="option-button" type="button" data-option-group="shape-width" data-option-value="thick" aria-pressed="false">L</button></div>
        </div>
        <div id="text-options" class="tool-options" hidden>
          <div class="option-set" aria-label="Text color"><span class="option-label">Color</span><button class="option-button color-option color-black" type="button" data-option-group="text-color" data-option-value="black" aria-label="Black" aria-pressed="true"></button><button class="option-button color-option color-blue" type="button" data-option-group="text-color" data-option-value="blue" aria-label="Blue" aria-pressed="false"></button><button class="option-button color-option color-red" type="button" data-option-group="text-color" data-option-value="red" aria-label="Red" aria-pressed="false"></button></div>
          <div class="option-set" aria-label="Text size"><span class="option-label">Size</span><button class="option-button" type="button" data-option-group="text-size" data-option-value="small" aria-pressed="false">S</button><button class="option-button is-active" type="button" data-option-group="text-size" data-option-value="medium" aria-pressed="true">M</button><button class="option-button" type="button" data-option-group="text-size" data-option-value="large" aria-pressed="false">L</button></div>
        </div>
      </div>
      <span class="toolbar-spacer"></span>
      <div class="toolbar-group markup-history" aria-label="Annotation history">
        <button id="annotation-undo" class="icon-button" type="button" aria-label="Undo PDF annotation" title="Undo">&#8630;</button>
        <button id="annotation-redo" class="icon-button" type="button" aria-label="Redo PDF annotation" title="Redo">&#8631;</button>
      </div>
    </div>
    <div id="selection-actions" class="selection-actions" role="toolbar" aria-label="Selected PDF text actions" hidden>
      <button id="selection-highlight" type="button">Highlight</button>
      <button id="selection-copy" type="button">Copy</button>
      <button id="selection-note" type="button">Note</button>
    </div>
    <div id="focus-actions" class="focus-actions" role="toolbar" aria-label="PDF focus controls" hidden>
      <button id="window-fullscreen" class="compact-button" type="button" aria-label="Toggle app window full screen" title="Toggle app window full screen">Full Screen</button>
      <button id="exit-focus" class="compact-button" type="button" aria-label="Exit PDF focus" title="Exit PDF focus">Done</button>
    </div>
    <div id="status" class="viewer-status" role="status" aria-live="polite">Loading PDF&hellip;</div>
    <div class="viewer-body">
      <aside id="pdf-sidebar" class="pdf-sidebar" aria-label="PDF sidebar" hidden>
        <div class="sidebar-header">
          <div class="sidebar-tabs" role="tablist" aria-label="PDF sidebar views">
            <button id="sidebar-thumbnails-tab" class="sidebar-tab is-active" type="button" role="tab" aria-selected="true" aria-controls="thumbnail-panel">Thumbnails</button>
            <button id="sidebar-outline-tab" class="sidebar-tab" type="button" role="tab" aria-selected="false" aria-controls="outline-panel">Contents</button>
            <button id="sidebar-annotations-tab" class="sidebar-tab" type="button" role="tab" aria-selected="false" aria-controls="annotation-panel">Annotations</button>
          </div>
        </div>
        <div id="thumbnail-panel" class="sidebar-panel thumbnail-panel" role="tabpanel" aria-labelledby="sidebar-thumbnails-tab">
          <div id="thumbnail-viewport" class="thumbnail-viewport">
            <div id="thumbnail-content" class="thumbnail-content" aria-label="PDF page thumbnails"></div>
          </div>
        </div>
        <div id="outline-panel" class="sidebar-panel outline-panel" role="tabpanel" aria-labelledby="sidebar-outline-tab" hidden>
          <div id="outline-content" class="outline-content" aria-label="PDF table of contents"></div>
        </div>
        <div id="annotation-panel" class="sidebar-panel annotation-panel" role="tabpanel" aria-labelledby="sidebar-annotations-tab" hidden>
          <div class="annotation-nav" aria-label="Annotation navigation">
            <button id="annotation-previous" class="icon-button" type="button" aria-label="Previous annotation" title="Previous annotation">&lsaquo;</button>
            <span id="annotation-count" class="annotation-count" aria-live="polite">0</span>
            <button id="annotation-next" class="icon-button" type="button" aria-label="Next annotation" title="Next annotation">&rsaquo;</button>
          </div>
          <div id="annotation-viewport" class="annotation-viewport">
            <div id="annotation-content" class="annotation-content" aria-label="PDF annotations"></div>
          </div>
        </div>
      </aside>
      <div id="canvas-shell" class="canvas-shell" hidden>
        <div id="pages" class="pages" aria-label="PDF pages"></div>
      </div>
    </div>
  </main>
  <script src="/rustnotepdf/bootstrap.js?${query.replace(/&/g, '&amp;')}"></script>
</body>
</html>`;
}

const IMAGE_EDIT_RECIPE_VERSION = 1;
const IMAGE_SCREEN_RENDITION_LONG_EDGE = 2560;
const IMAGE_EDIT_MAX_OUTPUT_PIXELS = 40_000_000;
const IMAGE_EDIT_MAX_OUTPUT_EDGE = 16_384;
const IMAGE_EDIT_FALLBACK_MAX_PIXELS = 6_000_000;
const IMAGE_EDIT_FALLBACK_MAX_EDGE = 2560;
const IMAGE_EDIT_TEXT_MAX_CHARS = 4000;
const IMAGE_EDIT_TEXT_MAX_LINES = 20;
const IMAGE_EDIT_COLORS = Object.freeze({
  red: '#d64545',
  blue: '#3478d4',
  black: '#202124',
  white: '#ffffff',
  yellow: '#f2c94c',
  green: '#2f9e68',
});

function clampImageEditUnit(value) {
  return Math.max(0, Math.min(1, Number(value) || 0));
}

function clampImageEditText(value) {
  return String(value || '')
    .slice(0, IMAGE_EDIT_TEXT_MAX_CHARS)
    .split('\n')
    .slice(0, IMAGE_EDIT_TEXT_MAX_LINES)
    .join('\n');
}

function defaultImageEditRecipe() {
  return {
    version: IMAGE_EDIT_RECIPE_VERSION,
    crop: {x: 0, y: 0, width: 1, height: 1},
    rotation: 0,
    flipX: false,
    flipY: false,
    overlays: [],
  };
}

function cloneImageEditRecipe(recipe) {
  return JSON.parse(JSON.stringify(recipe || defaultImageEditRecipe()));
}

function sanitizeImageEditRecipe(value) {
  let parsed = value;
  if (typeof parsed === 'string' && parsed.trim()) {
    try { parsed = JSON.parse(parsed); } catch { parsed = null; }
  }
  if (!parsed || typeof parsed !== 'object' || Number(parsed.version) !== IMAGE_EDIT_RECIPE_VERSION) {
    return defaultImageEditRecipe();
  }
  const rawCrop = parsed.crop && typeof parsed.crop === 'object' ? parsed.crop : {};
  let x = clampImageEditUnit(rawCrop.x);
  let y = clampImageEditUnit(rawCrop.y);
  let width = Math.max(0.001, Math.min(1 - x, Number(rawCrop.width) || 1));
  let height = Math.max(0.001, Math.min(1 - y, Number(rawCrop.height) || 1));
  if (x + width > 1) width = 1 - x;
  if (y + height > 1) height = 1 - y;
  const rotation = [0, 90, 180, 270].includes(Number(parsed.rotation)) ? Number(parsed.rotation) : 0;
  const overlays = Array.isArray(parsed.overlays)
    ? parsed.overlays.slice(0, 500).map((overlay, index) => sanitizeImageEditOverlay(overlay, index)).filter(Boolean)
    : [];
  return {
    version: IMAGE_EDIT_RECIPE_VERSION,
    crop: {x, y, width, height},
    rotation,
    flipX: parsed.flipX === true,
    flipY: parsed.flipY === true,
    overlays,
  };
}

function sanitizeImageEditOverlay(overlay, index) {
  if (!overlay || typeof overlay !== 'object') return null;
  const type = String(overlay.type || '');
  const id = String(overlay.id || `overlay_${index + 1}`).slice(0, 80);
  const color = IMAGE_EDIT_COLORS[overlay.color] ? overlay.color : 'red';
  const width = Math.max(0.0002, Math.min(0.08, Number(overlay.width) || 0.004));
  const common = {id, type, color, width};
  if (type === 'path') {
    const points = Array.isArray(overlay.points)
      ? overlay.points.slice(0, 5000).map(point => ({x: clampImageEditUnit(point && point.x), y: clampImageEditUnit(point && point.y)}))
      : [];
    if (points.length < 2) return null;
    return {...common, points, opacity: Math.max(0.08, Math.min(1, Number(overlay.opacity) || 1)), mode: overlay.mode === 'highlighter' ? 'highlighter' : 'pen'};
  }
  if (['rect', 'ellipse', 'line', 'arrow'].includes(type)) {
    return {
      ...common,
      x1: clampImageEditUnit(overlay.x1), y1: clampImageEditUnit(overlay.y1),
      x2: clampImageEditUnit(overlay.x2), y2: clampImageEditUnit(overlay.y2),
    };
  }
  if (type === 'text') {
    return {
      ...common,
      x: clampImageEditUnit(overlay.x), y: clampImageEditUnit(overlay.y),
      boxWidth: Math.max(0.03, Math.min(1, Number(overlay.boxWidth) || 0.25)),
      boxHeight: Math.max(0.02, Math.min(1, Number(overlay.boxHeight) || 0.1)),
      text: clampImageEditText(overlay.text),
      fontSize: Math.max(0.004, Math.min(0.2, Number(overlay.fontSize) || 0.035)),
      bold: overlay.bold === true,
      background: overlay.background === true,
    };
  }
  return null;
}

function imageEditRecipeEqual(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function imageEditCrop(recipe, full = false) {
  return full ? {x: 0, y: 0, width: 1, height: 1} : recipe.crop;
}

function imageEditSourceToOutput(point, recipe, full = false) {
  const crop = imageEditCrop(recipe, full);
  let x = (point.x - crop.x) / Math.max(0.000001, crop.width);
  let y = (point.y - crop.y) / Math.max(0.000001, crop.height);
  if (recipe.flipX) x = 1 - x;
  if (recipe.flipY) y = 1 - y;
  switch (recipe.rotation) {
    case 90: return {x: 1 - y, y: x};
    case 180: return {x: 1 - x, y: 1 - y};
    case 270: return {x: y, y: 1 - x};
    default: return {x, y};
  }
}

function imageEditOutputToSource(point, recipe, full = false) {
  let x = point.x;
  let y = point.y;
  let a;
  let b;
  switch (recipe.rotation) {
    case 90: a = y; b = 1 - x; break;
    case 180: a = 1 - x; b = 1 - y; break;
    case 270: a = 1 - y; b = x; break;
    default: a = x; b = y; break;
  }
  if (recipe.flipX) a = 1 - a;
  if (recipe.flipY) b = 1 - b;
  const crop = imageEditCrop(recipe, full);
  return {
    x: clampImageEditUnit(crop.x + a * crop.width),
    y: clampImageEditUnit(crop.y + b * crop.height),
  };
}

function imageEditCropPixelSize(recipe, sourceWidth, sourceHeight, full = false) {
  const crop = imageEditCrop(recipe, full);
  const width = Math.max(1, Math.round(sourceWidth * crop.width));
  const height = Math.max(1, Math.round(sourceHeight * crop.height));
  if (recipe.rotation === 90 || recipe.rotation === 270) return {width: height, height: width};
  return {width, height};
}

function imageEditDrawRenderPlan(context, source, plan) {
  if (!context || !source || !plan) return;
  const {width, height, base, overlays} = plan;
  context.save();
  if (base.rotation === 90) { context.translate(width, 0); context.rotate(Math.PI / 2); }
  else if (base.rotation === 180) { context.translate(width, height); context.rotate(Math.PI); }
  else if (base.rotation === 270) { context.translate(0, height); context.rotate(-Math.PI / 2); }
  if (base.flipX) { context.translate(base.localWidth, 0); context.scale(-1, 1); }
  if (base.flipY) { context.translate(0, base.localHeight); context.scale(1, -1); }
  context.drawImage(source, base.sx, base.sy, base.sw, base.sh, 0, 0, base.localWidth, base.localHeight);
  context.restore();

  for (const overlay of overlays || []) {
    context.save();
    context.strokeStyle = overlay.color;
    context.fillStyle = overlay.color;
    context.lineWidth = overlay.strokeWidth;
    context.lineCap = 'round';
    context.lineJoin = 'round';
    if (overlay.type === 'path') {
      context.globalAlpha = overlay.opacity;
      context.beginPath();
      overlay.points.forEach((point, index) => index === 0 ? context.moveTo(point.x, point.y) : context.lineTo(point.x, point.y));
      context.stroke();
    } else if (overlay.type === 'line' || overlay.type === 'arrow') {
      context.beginPath();
      context.moveTo(overlay.start.x, overlay.start.y);
      context.lineTo(overlay.end.x, overlay.end.y);
      context.stroke();
      if (overlay.type === 'arrow' && overlay.arrowHead) {
        context.beginPath();
        context.moveTo(overlay.arrowHead.p1.x, overlay.arrowHead.p1.y);
        context.lineTo(overlay.end.x, overlay.end.y);
        context.lineTo(overlay.arrowHead.p2.x, overlay.arrowHead.p2.y);
        context.stroke();
      }
    } else if (overlay.type === 'rect') {
      context.strokeRect(overlay.bounds.x1, overlay.bounds.y1, overlay.bounds.x2 - overlay.bounds.x1, overlay.bounds.y2 - overlay.bounds.y1);
    } else if (overlay.type === 'ellipse') {
      const bounds = overlay.bounds;
      context.beginPath();
      context.ellipse(
        (bounds.x1 + bounds.x2) / 2,
        (bounds.y1 + bounds.y2) / 2,
        Math.max(0.5, (bounds.x2 - bounds.x1) / 2),
        Math.max(0.5, (bounds.y2 - bounds.y1) / 2),
        0, 0, Math.PI * 2,
      );
      context.stroke();
    } else if (overlay.type === 'text') {
      const bounds = overlay.bounds;
      context.font = `${overlay.bold ? '700' : '400'} ${overlay.fontSize}px -apple-system, BlinkMacSystemFont, sans-serif`;
      context.textBaseline = 'top';
      if (overlay.background) {
        context.save();
        context.fillStyle = 'rgba(255,255,255,0.82)';
        context.fillRect(bounds.x1, bounds.y1, Math.max(1, bounds.x2 - bounds.x1), Math.max(1, bounds.y2 - bounds.y1));
        context.restore();
      }
      context.fillStyle = overlay.color;
      const lineHeight = overlay.fontSize * 1.2;
      overlay.lines.forEach((line, index) => context.fillText(line, bounds.x1 + 4, bounds.y1 + 4 + index * lineHeight, Math.max(1, bounds.x2 - bounds.x1 - 8)));
    }
    context.restore();
  }
}

function imageEditOverlayBounds(overlay) {
  if (!overlay) return null;
  if (overlay.type === 'path') {
    let minX = 1; let minY = 1; let maxX = 0; let maxY = 0;
    for (const point of overlay.points || []) {
      minX = Math.min(minX, point.x); minY = Math.min(minY, point.y);
      maxX = Math.max(maxX, point.x); maxY = Math.max(maxY, point.y);
    }
    return {x1: minX, y1: minY, x2: maxX, y2: maxY};
  }
  if (overlay.type === 'text') {
    return {x1: overlay.x, y1: overlay.y, x2: overlay.x + overlay.boxWidth, y2: overlay.y + overlay.boxHeight};
  }
  if (['rect', 'ellipse', 'line', 'arrow'].includes(overlay.type)) {
    return {x1: Math.min(overlay.x1, overlay.x2), y1: Math.min(overlay.y1, overlay.y2), x2: Math.max(overlay.x1, overlay.x2), y2: Math.max(overlay.y1, overlay.y2)};
  }
  return null;
}

function imageEditMoveOverlay(overlay, deltaX, deltaY) {
  const bounds = imageEditOverlayBounds(overlay);
  if (!bounds) return;
  const minDx = -bounds.x1;
  const maxDx = 1 - bounds.x2;
  const minDy = -bounds.y1;
  const maxDy = 1 - bounds.y2;
  const dx = Math.max(minDx, Math.min(maxDx, deltaX));
  const dy = Math.max(minDy, Math.min(maxDy, deltaY));
  if (overlay.type === 'path') {
    overlay.points = overlay.points.map(point => ({x: point.x + dx, y: point.y + dy}));
  } else if (overlay.type === 'text') {
    overlay.x += dx; overlay.y += dy;
  } else {
    overlay.x1 += dx; overlay.x2 += dx; overlay.y1 += dy; overlay.y2 += dy;
  }
}

function imageEditNormalizedRect(a, b) {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.max(0.001, Math.abs(b.x - a.x)),
    height: Math.max(0.001, Math.abs(b.y - a.y)),
  };
}

class AttachmentView {
  constructor(node, view, getPos, controller) {
    this.node = node;
    this.view = view;
    this.getPos = getPos;
    this.controller = controller;
    this.dom = document.createElement(node.attrs.kind === 'image' ? 'figure' : 'div');
    this.dom.contentEditable = 'false';
    this.dom.setAttribute('role', 'group');
    this.dom.tabIndex = 0;
    this.dom.dataset.contextLongPress = 'true';
    this.dom.setAttribute('aria-keyshortcuts', 'Shift+F10');
    this.dom.setAttribute('aria-haspopup', 'menu');
    this.controller?.attachmentViews?.add(this);
    this.pdfFocusActive = false;
    if (node.attrs.kind === 'image') this.buildImageSurface();
    else if (node.attrs.kind === 'audio' || node.attrs.kind === 'video') this.buildMediaSurface();
    else if (node.attrs.kind === 'pdf') this.buildPdfSurface();
    else this.buildFileSurface();
    this.dom.setAttribute('aria-label', `Attachment: ${node.attrs.displayName || 'file'}. Press Shift+F10 for actions.`);
    this.render();
  }

  buildAttachmentHeader() {
    const header = document.createElement('div');
    header.className = 'prosemirror-attachment-header';
    this.icon = document.createElement('span');
    this.icon.className = 'prosemirror-attachment-icon';
    this.details = document.createElement('span');
    this.details.className = 'prosemirror-attachment-details';
    this.name = document.createElement('strong');
    this.name.className = 'prosemirror-attachment-name';
    this.meta = document.createElement('span');
    this.meta.className = 'prosemirror-attachment-meta';
    this.details.append(this.name, this.meta);
    header.append(this.icon, this.details);
    return header;
  }

  buildAttachmentActions() {
    const actions = document.createElement('div');
    actions.className = 'prosemirror-attachment-actions';
    const openButton = document.createElement('button');
    openButton.type = 'button';
    openButton.className = 'prosemirror-attachment-action-button';
    openButton.textContent = 'Open';
    openButton.addEventListener('click', () => this.openOriginal());
    const deleteButton = document.createElement('button');
    deleteButton.type = 'button';
    deleteButton.className = 'prosemirror-attachment-action-button prosemirror-attachment-remove-button';
    deleteButton.textContent = 'Remove';
    deleteButton.title = 'Remove from document';
    deleteButton.addEventListener('click', () => this.deleteNode());
    actions.append(openButton, deleteButton);
    this.openButton = openButton;
    this.deleteButton = deleteButton;
    return actions;
  }

  buildFileSurface() {
    this.dom.className = 'prosemirror-attachment-node prosemirror-file-node';
    this.dom.append(this.buildAttachmentHeader(), this.buildAttachmentActions());
  }

  buildPdfSurface() {
    this.dom.className = 'prosemirror-attachment-node prosemirror-pdf-node';
    this.dom.append(this.buildAttachmentHeader());

    this.pdfActions = document.createElement('div');
    this.pdfActions.className = 'prosemirror-attachment-actions prosemirror-pdf-controls';

    this.previewButton = document.createElement('button');
    this.previewButton.type = 'button';
    this.previewButton.className = 'prosemirror-attachment-action-button';
    this.previewButton.textContent = 'Preview';
    this.previewButton.setAttribute('aria-expanded', 'false');
    this.previewButton.addEventListener('click', () => this.togglePdfPreview());

    this.openButton = document.createElement('button');
    this.openButton.type = 'button';
    this.openButton.className = 'prosemirror-attachment-action-button';
    this.openButton.textContent = 'Open externally';
    this.openButton.addEventListener('click', () => this.openOriginal());

    this.deleteButton = document.createElement('button');
    this.deleteButton.type = 'button';
    this.deleteButton.className = 'prosemirror-attachment-action-button prosemirror-attachment-remove-button';
    this.deleteButton.textContent = 'Remove';
    this.deleteButton.title = 'Remove from document';
    this.deleteButton.addEventListener('click', () => this.deleteNode());
    this.pdfActions.append(this.previewButton, this.openButton, this.deleteButton);
    this.dom.append(this.pdfActions);

    this.pdfPreview = document.createElement('div');
    this.pdfPreview.className = 'prosemirror-pdf-preview';
    this.pdfPreview.hidden = true;
    this.pdfFrame = this.createPdfFrame();
    this.pdfHostStatus = document.createElement('div');
    this.pdfHostStatus.className = 'prosemirror-pdf-host-status';
    this.pdfHostStatus.textContent = 'Starting PDF preview…';
    this.pdfPreview.append(this.pdfHostStatus, this.pdfFrame);
    this.pdfMessageHandler = event => this.handlePdfViewerMessage(event);
    window.addEventListener('message', this.pdfMessageHandler);
    this.dom.append(this.pdfPreview);
  }


  createPdfFrame() {
    const frame = document.createElement('iframe');
    frame.className = 'prosemirror-pdf-frame';
    frame.title = 'PDF preview';
    frame.referrerPolicy = 'no-referrer';
    frame.setAttribute('sandbox', 'allow-scripts allow-same-origin');
    return frame;
  }

  resetPdfFrame() {
    if (!this.pdfFrame || !this.pdfPreview) return;
    this.pdfFrame.remove();
    this.pdfFrame = this.createPdfFrame();
    this.pdfPreview.append(this.pdfFrame);
  }

  pdfViewerKey() {
    return `${this.node.attrs.attachmentId}:${this.controller?.editable === true ? 'edit' : 'read'}`;
  }

  installPdfViewerDocument() {
    const key = this.pdfViewerKey();
    if (this.currentPdfViewerKey === key) return;
    this.currentPdfViewerKey = key;
    this.pdfHostStatus.hidden = false;
    this.pdfHostStatus.dataset.error = 'false';
    this.pdfHostStatus.textContent = 'Starting PDF preview…';
    window.clearTimeout(this.pdfBootstrapTimer);
    this.pdfBootstrapTimer = window.setTimeout(() => {
      if (this.currentPdfViewerKey !== key || this.pdfPreview?.hidden === true) return;
      this.pdfHostStatus.hidden = false;
      this.pdfHostStatus.dataset.error = 'true';
      this.pdfHostStatus.textContent = 'PDF viewer failed to start. Use Open externally.';
    }, 5000);
    const viewerDocument = this.pdfFrame?.contentDocument;
    if (!viewerDocument) {
      window.clearTimeout(this.pdfBootstrapTimer);
      this.pdfBootstrapTimer = null;
      this.currentPdfViewerKey = null;
      this.pdfHostStatus.dataset.error = 'true';
      this.pdfHostStatus.textContent = 'PDF viewer could not be created. Use Open externally.';
      return;
    }
    viewerDocument.open();
    viewerDocument.write(pdfViewerDocumentHtml(
      this.node.attrs.attachmentId,
      this.controller?.editable !== true,
    ));
    viewerDocument.close();
  }

  handlePdfViewerMessage(event) {
    if (!this.pdfFrame || event.source !== this.pdfFrame.contentWindow) return;
    const message = event.data;
    if (!message || message.type !== 'rustnote-pdf-viewer') return;
    const messageAttachmentId = String(message.attachmentId || '');
    if (messageAttachmentId && messageAttachmentId !== this.node.attrs.attachmentId) return;
    const stage = String(message.stage || '');
    if (stage === 'focus-toggle') {
      this.controller?.togglePdfFocus(this);
      return;
    }
    if (stage === 'focus-exit') {
      this.controller?.exitPdfFocus(this);
      return;
    }
    if (stage === 'window-fullscreen-toggle') {
      this.controller?.requestWindowFullscreenToggle();
      return;
    }
    window.clearTimeout(this.pdfBootstrapTimer);
    this.pdfBootstrapTimer = null;
    if (stage === 'export-ready') {
      let detail = null;
      try { detail = JSON.parse(String(message.detail || '')); } catch { /* invalid export detail */ }
      if (!detail || !this.controller?.requestPdfExportSave(this, String(detail.requestId || ''), String(detail.token || ''))) {
        this.completePdfExport(String(detail?.requestId || ''), false, 'Could not open the PDF export dialog.');
      }
      return;
    }
    if (stage === 'export-error') return;
    if (stage === 'error') {
      this.pdfHostStatus.hidden = false;
      this.pdfHostStatus.dataset.error = 'true';
      const detail = String(message.detail || '').trim();
      this.pdfHostStatus.textContent = detail
        ? `PDF preview failed: ${detail}`
        : 'PDF preview failed. Use Open externally.';
      return;
    }
    if (stage === 'module-ready' || stage === 'metadata' || stage === 'transport-ready'
      || stage === 'document-ready' || stage === 'rendered') {
      this.pdfHostStatus.hidden = true;
      this.pdfHostStatus.dataset.error = 'false';
    }
  }

  applyPdfFocusState(active) {
    const next = Boolean(active);
    if (this.pdfFocusActive === next) return;
    this.pdfFocusActive = next;
    this.dom.classList.toggle('prosemirror-pdf-focus-node', next);
    if (next && this.pdfPreview && !this.pdfFocusPlaceholder) {
      this.pdfFocusPlaceholder = document.createElement('div');
      this.pdfFocusPlaceholder.className = 'prosemirror-pdf-focus-placeholder';
      this.pdfFocusPlaceholder.style.height = `${Math.max(0, this.pdfPreview.getBoundingClientRect().height)}px`;
      this.pdfPreview.before(this.pdfFocusPlaceholder);
    }
    this.pdfPreview?.classList.toggle('is-pdf-focus', next);
    if (this.pdfPreview) {
      if (next) {
        this.pdfPreview.setAttribute('role', 'dialog');
        this.pdfPreview.setAttribute('aria-label', `PDF focus view: ${this.node.attrs.displayName || 'PDF'}`);
      } else {
        this.pdfPreview.removeAttribute('role');
        this.pdfPreview.removeAttribute('aria-label');
      }
    }
    if (!next && this.pdfFocusPlaceholder) {
      this.pdfFocusPlaceholder.remove();
      this.pdfFocusPlaceholder = null;
    }
    this.pdfFrame?.contentWindow?.postMessage({
      type: 'rustnote-pdf-host',
      stage: 'focus-state',
      active: next,
    }, '*');
  }

  completePdfExport(requestId, success, message = '') {
    this.pdfFrame?.contentWindow?.postMessage({
      type: 'rustnote-pdf-host',
      stage: 'export-complete',
      requestId,
      success: Boolean(success),
      message: String(message || ''),
    }, '*');
  }

  togglePdfPreview(forceOpen = null) {
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
    const available = runtime?.available === true;
    const nextOpen = forceOpen === null ? this.pdfPreview?.hidden === true : Boolean(forceOpen);
    if (!available && nextOpen) return;
    if (!nextOpen && this.pdfFocusActive) this.controller?.exitPdfFocus(this);
    this.pdfPreview.hidden = !nextOpen;
    this.previewButton?.setAttribute('aria-expanded', nextOpen ? 'true' : 'false');
    this.previewButton && (this.previewButton.textContent = nextOpen ? 'Close preview' : 'Preview');
    if (nextOpen) {
      this.installPdfViewerDocument();
    } else if (this.pdfFrame) {
      window.clearTimeout(this.pdfBootstrapTimer);
      this.pdfBootstrapTimer = null;
      this.currentPdfViewerKey = null;
      this.resetPdfFrame();
    }
  }

  buildMediaSurface() {
    const kind = this.node.attrs.kind;
    this.dom.className = `prosemirror-attachment-node prosemirror-media-node prosemirror-${kind}-node`;
    this.dom.append(this.buildAttachmentHeader());
    this.mediaFrame = document.createElement('div');
    this.mediaFrame.className = 'prosemirror-media-frame';
    this.mediaFrame.tabIndex = 0;
    this.mediaFrame.setAttribute('role', 'group');
    this.mediaFrame.setAttribute('aria-label', `${kind === 'video' ? 'Video' : 'Audio'} player`);
    this.media = document.createElement(kind === 'video' ? 'video' : 'audio');
    this.media.className = `prosemirror-${kind}`;
    this.media.controls = false;
    this.media.preload = 'metadata';
    this.mediaPreviewLoaded = false;
    this.mediaNearViewport = typeof IntersectionObserver !== 'function';
    this.mediaPlaybackPinned = false;
    this.mediaSourceResetting = false;
    this.mediaAttachedPreviewUrl = '';
    this.mediaFailedPreviewUrls = new Set();
    this.mediaPreviewCandidates = [];
    this.mediaPreviewCandidatesKey = '';
    this.mediaPreviewCandidateIndex = -1;
    this.currentPreviewUrl = '';
    this.mediaSeekActive = false;
    this.mediaAnnotationsOpen = false;
    this.mediaAnnotationEditingId = null;
    this.mediaRangeStartMs = null;
    this.mediaLoopAnnotationId = null;
    this.mediaResumeAppliedForUrl = '';
    this.mediaPendingSeek = null;
    this.mediaLastResumePersistedMs = readMediaResumePosition(this.node.attrs.attachmentId);
    this.mediaLastResumePersistAt = 0;
    this.mediaTrimEnded = false;

    this.buildMediaControls(kind);
    this.buildMediaAnnotationsSurface();

    this.media.addEventListener('loadedmetadata', () => {
      if (!this.currentPreviewUrl || this.mediaAttachedPreviewUrl !== this.currentPreviewUrl) return;
      this.mediaPreviewLoaded = true;
      this.media.hidden = false;
      this.ensureMediaWithinTrimBounds();
      this.mediaMissing.hidden = true;
      this.mediaControls.hidden = false;
      this.applyMediaResumePosition();
      this.applyPendingMediaSeek();
      this.updateMediaControls();
      this.updateMediaAdvancedControls();
      this.renderMediaAnnotations();
    });
    this.media.addEventListener('durationchange', () => { this.updateMediaControls(); this.renderMediaAnnotationMarkers(); });
    this.media.addEventListener('timeupdate', () => {
      this.enforceMediaTrimEnd();
      this.applyMediaLoop();
      this.updateMediaControls();
      this.persistMediaPlaybackPosition(false);
    });
    this.media.addEventListener('progress', () => this.updateMediaBufferedProgress());
    this.media.addEventListener('play', () => {
      this.mediaPlaybackPinned = true;
      this.updateMediaControls();
    });
    this.media.addEventListener('pause', () => {
      this.updateMediaControls();
      if (this.mediaSourceResetting) return;
      this.persistMediaPlaybackPosition(true);
      this.mediaPlaybackPinned = false;
      if (!this.mediaNearViewport) this.render();
    });
    this.media.addEventListener('ended', () => {
      this.mediaPlaybackPinned = false;
      this.mediaTrimEnded = true;
      this.persistMediaPlaybackPosition(true, 0);
      this.updateMediaControls();
      if (!this.mediaNearViewport) this.render();
    });
    this.media.addEventListener('volumechange', () => this.updateMediaControls());
    this.media.addEventListener('ratechange', () => this.updateMediaControls());
    this.media.addEventListener('error', () => {
      if (!this.currentPreviewUrl || this.mediaAttachedPreviewUrl !== this.currentPreviewUrl) return;
      const failedCandidate = this.currentMediaPreviewCandidate();
      this.mediaPreviewLoaded = false;
      this.mediaPlaybackPinned = false;
      this.mediaFailedPreviewUrls.add(this.currentPreviewUrl);
      if (failedCandidate?.source === 'compatible') {
        this.releaseMediaSource();
        this.controller?.invalidateMediaCompatibility(this);
        this.controller?.ensureMediaCompatibility(this);
        this.render();
        return;
      }
      if (this.advanceMediaPreviewCandidate()) return;
      const runtime = this.controller?.attachmentRegistry?.get(this.node.attrs.attachmentId) || null;
      if (failedCandidate?.source === 'native' && this.mediaCompatibilityPotential(runtime)) {
        this.releaseMediaSource();
        this.controller?.ensureMediaCompatibility(this);
      }
      this.render();
    });
    if (kind === 'video') {
      this.media.playsInline = true;
      this.media.setAttribute('playsinline', '');
      this.media.addEventListener('click', event => {
        if (!this.mediaPreviewLoaded) return;
        event.preventDefault();
        this.mediaFrame.focus({preventScroll: true});
        this.toggleMediaPlayback();
      });
    }

    this.mediaMissing = document.createElement('div');
    this.mediaMissing.className = 'prosemirror-media-missing';
    this.mediaMissingText = document.createElement('span');
    this.mediaMissingText.textContent = kind === 'video' ? 'Video unavailable' : 'Audio unavailable';
    this.mediaFallbackButton = document.createElement('button');
    this.mediaFallbackButton.type = 'button';
    this.mediaFallbackButton.className = 'prosemirror-attachment-action-button prosemirror-media-open-fallback';
    this.mediaFallbackButton.textContent = 'Open externally';
    this.mediaFallbackButton.hidden = true;
    this.mediaFallbackButton.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      this.openOriginal();
    });
    this.mediaMissing.append(this.mediaMissingText, this.mediaFallbackButton);
    this.mediaFrame.append(this.media, this.mediaMissing, this.mediaControls);
    this.dom.append(
      this.mediaFrame,
      this.mediaCaptionInput,
      this.mediaAnnotationSummaryBar,
      this.mediaAnnotationPanel,
      this.buildAttachmentActions(),
    );

    this.mediaFrame.addEventListener('keydown', event => this.handleMediaKeydown(event));
    this.mediaFullscreenChangeHandler = () => this.updateMediaAdvancedControls();
    document.addEventListener('fullscreenchange', this.mediaFullscreenChangeHandler);
    document.addEventListener('webkitfullscreenchange', this.mediaFullscreenChangeHandler);
    if (kind === 'video') {
      this.mediaEnterPictureInPictureHandler = () => this.updateMediaAdvancedControls();
      this.mediaLeavePictureInPictureHandler = () => this.updateMediaAdvancedControls();
      this.media.addEventListener('enterpictureinpicture', this.mediaEnterPictureInPictureHandler);
      this.media.addEventListener('leavepictureinpicture', this.mediaLeavePictureInPictureHandler);
      this.mediaWebkitPresentationModeHandler = () => this.updateMediaAdvancedControls();
      this.media.addEventListener('webkitpresentationmodechanged', this.mediaWebkitPresentationModeHandler);
    }

    if (typeof IntersectionObserver === 'function') {
      const scrollRoot = this.view.dom.closest('.markdown-preview');
      this.mediaVisibilityObserver = new IntersectionObserver(entries => {
        const entry = entries[entries.length - 1];
        const near = Boolean(entry && (entry.isIntersecting || entry.intersectionRatio > 0));
        if (near === this.mediaNearViewport) return;
        this.mediaNearViewport = near;
        this.render();
      }, {root: scrollRoot instanceof Element ? scrollRoot : null, rootMargin: '150% 0px', threshold: 0});
      this.mediaVisibilityObserver.observe(this.dom);
    }
  }

  buildMediaControls(kind) {
    this.mediaControls = document.createElement('div');
    this.mediaControls.className = `prosemirror-media-controls prosemirror-${kind}-controls`;
    this.mediaControls.hidden = true;

    this.mediaPlayButton = document.createElement('button');
    this.mediaPlayButton.type = 'button';
    this.mediaPlayButton.className = 'prosemirror-media-control-button prosemirror-media-play-button';
    this.mediaPlayButton.textContent = '▶';
    this.mediaPlayButton.setAttribute('aria-label', 'Play');
    this.mediaPlayButton.title = 'Play (Space)';
    this.mediaPlayButton.addEventListener('click', () => this.toggleMediaPlayback());

    this.mediaCurrentTime = document.createElement('span');
    this.mediaCurrentTime.className = 'prosemirror-media-time prosemirror-media-current-time';
    this.mediaCurrentTime.textContent = '0:00';

    this.mediaSeekShell = document.createElement('div');
    this.mediaSeekShell.className = 'prosemirror-media-seek-shell';
    this.mediaSeek = document.createElement('input');
    this.mediaSeek.type = 'range';
    this.mediaSeek.className = 'prosemirror-media-range prosemirror-media-seek';
    this.mediaSeek.min = '0';
    this.mediaSeek.max = '1000';
    this.mediaSeek.step = '1';
    this.mediaSeek.value = '0';
    this.mediaSeek.setAttribute('aria-label', 'Seek');
    this.mediaSeek.setAttribute('aria-valuetext', '0:00 of 0:00');
    this.mediaSeek.addEventListener('pointerdown', () => { this.mediaSeekActive = true; });
    this.mediaSeek.addEventListener('pointerup', () => { this.mediaSeekActive = false; this.commitMediaSeek(); });
    this.mediaSeek.addEventListener('pointercancel', () => { this.mediaSeekActive = false; this.updateMediaControls(); });
    this.mediaSeek.addEventListener('input', () => this.previewMediaSeek());
    this.mediaSeek.addEventListener('change', () => { this.mediaSeekActive = false; this.commitMediaSeek(); });
    this.mediaAnnotationTrack = document.createElement('div');
    this.mediaAnnotationTrack.className = 'prosemirror-media-annotation-track';
    this.mediaSeekShell.append(this.mediaSeek, this.mediaAnnotationTrack);

    this.mediaDuration = document.createElement('span');
    this.mediaDuration.className = 'prosemirror-media-time prosemirror-media-duration';
    this.mediaDuration.textContent = '0:00';

    this.mediaMuteButton = document.createElement('button');
    this.mediaMuteButton.type = 'button';
    this.mediaMuteButton.className = 'prosemirror-media-control-button prosemirror-media-mute-button';
    this.mediaMuteButton.textContent = 'Mute';
    this.mediaMuteButton.setAttribute('aria-label', 'Mute');
    this.mediaMuteButton.title = 'Mute (M)';
    this.mediaMuteButton.addEventListener('click', () => {
      this.media.muted = !this.media.muted;
      this.updateMediaControls();
    });

    this.mediaVolume = document.createElement('input');
    this.mediaVolume.type = 'range';
    this.mediaVolume.className = 'prosemirror-media-range prosemirror-media-volume';
    this.mediaVolume.min = '0';
    this.mediaVolume.max = '1';
    this.mediaVolume.step = '0.05';
    this.mediaVolume.value = '1';
    this.mediaVolume.setAttribute('aria-label', 'Volume');
    this.mediaVolume.setAttribute('aria-valuetext', '100%');
    this.mediaVolume.addEventListener('input', () => {
      const value = Number(this.mediaVolume.value);
      if (!Number.isFinite(value)) return;
      this.media.volume = Math.max(0, Math.min(1, value));
      if (this.media.volume > 0) this.media.muted = false;
    });

    this.mediaSpeed = document.createElement('select');
    this.mediaSpeed.className = 'prosemirror-media-speed';
    this.mediaSpeed.setAttribute('aria-label', 'Playback speed');
    this.mediaSpeed.title = 'Playback speed';
    for (const rate of [0.5, 0.75, 1, 1.25, 1.5, 2]) {
      const option = document.createElement('option');
      option.value = String(rate);
      option.textContent = `${rate}×`;
      if (rate === 1) option.selected = true;
      this.mediaSpeed.append(option);
    }
    this.mediaSpeed.addEventListener('change', () => {
      const value = Number(this.mediaSpeed.value);
      if (Number.isFinite(value) && value > 0) this.media.playbackRate = value;
    });

    this.mediaMoreControls = document.createElement('div');
    this.mediaMoreControls.className = 'prosemirror-media-more';
    this.mediaMoreSummary = document.createElement('button');
    this.mediaMoreSummary.type = 'button';
    this.mediaMoreSummary.className = 'prosemirror-media-control-button prosemirror-media-more-button';
    this.mediaMoreSummary.textContent = '⋯';
    this.mediaMoreSummary.setAttribute('aria-label', 'More media controls');
    this.mediaMoreSummary.setAttribute('aria-expanded', 'false');
    this.mediaMoreSummary.title = 'More media controls';
    this.mediaMoreSummary.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      const open = this.mediaMoreControls.classList.toggle('is-open');
      this.mediaMoreSummary.setAttribute('aria-expanded', open ? 'true' : 'false');
    });
    this.mediaMoreOutsideClickHandler = event => {
      if (!this.mediaMoreControls.classList.contains('is-open')) return;
      if (event.target instanceof Node && this.mediaMoreControls.contains(event.target)) return;
      this.mediaMoreControls.classList.remove('is-open');
      this.mediaMoreSummary.setAttribute('aria-expanded', 'false');
    };
    document.addEventListener('pointerdown', this.mediaMoreOutsideClickHandler);
    this.mediaMoreMenu = document.createElement('div');
    this.mediaMoreMenu.className = 'prosemirror-media-more-menu';
    this.mediaMoreMenu.setAttribute('role', 'group');
    this.mediaMoreMenu.setAttribute('aria-label', 'Secondary media controls');
    this.mediaMoreMenu.append(this.mediaMuteButton, this.mediaVolume, this.mediaSpeed);
    this.mediaMoreControls.append(this.mediaMoreSummary, this.mediaMoreMenu);

    this.mediaMarkButton = document.createElement('button');
    this.mediaMarkButton.type = 'button';
    this.mediaMarkButton.className = 'prosemirror-media-control-button prosemirror-media-mark-button';
    this.mediaMarkButton.textContent = 'Mark';
    this.mediaMarkButton.title = 'Add bookmark at current time (B)';
    this.mediaMarkButton.setAttribute('aria-label', 'Add bookmark at current time');
    this.mediaMarkButton.addEventListener('click', () => this.addMediaPointAnnotation());

    this.mediaRangeButton = document.createElement('button');
    this.mediaRangeButton.type = 'button';
    this.mediaRangeButton.className = 'prosemirror-media-control-button prosemirror-media-range-button';
    this.mediaRangeButton.textContent = 'Range';
    this.mediaRangeButton.title = 'Start or finish a range annotation (R)';
    this.mediaRangeButton.setAttribute('aria-label', 'Start range annotation');
    this.mediaRangeButton.addEventListener('click', () => this.toggleMediaRangeCapture());

    this.mediaControls.append(
      this.mediaPlayButton,
      this.mediaCurrentTime,
      this.mediaSeekShell,
      this.mediaDuration,
      this.mediaMoreControls,
      this.mediaMarkButton,
      this.mediaRangeButton,
    );

    if (kind === 'video') {
      this.mediaPictureInPictureButton = document.createElement('button');
      this.mediaPictureInPictureButton.type = 'button';
      this.mediaPictureInPictureButton.className = 'prosemirror-media-control-button prosemirror-media-pip-button';
      this.mediaPictureInPictureButton.textContent = 'PiP';
      this.mediaPictureInPictureButton.setAttribute('aria-label', 'Picture in Picture');
      this.mediaPictureInPictureButton.title = 'Picture in Picture (P)';
      this.mediaPictureInPictureButton.addEventListener('click', () => this.toggleMediaPictureInPicture());

      this.mediaFullscreenButton = document.createElement('button');
      this.mediaFullscreenButton.type = 'button';
      this.mediaFullscreenButton.className = 'prosemirror-media-control-button prosemirror-media-fullscreen-button';
      this.mediaFullscreenButton.textContent = 'Full';
      this.mediaFullscreenButton.setAttribute('aria-label', 'Enter fullscreen');
      this.mediaFullscreenButton.title = 'Fullscreen (F)';
      this.mediaFullscreenButton.addEventListener('click', () => this.toggleMediaFullscreen());
      this.mediaMoreMenu.append(this.mediaPictureInPictureButton);
      this.mediaControls.append(this.mediaFullscreenButton);
    }
  }

  buildMediaAnnotationsSurface() {
    this.mediaCaptionInput = document.createElement('input');
    this.mediaCaptionInput.type = 'text';
    this.mediaCaptionInput.className = 'prosemirror-media-caption';
    this.mediaCaptionInput.placeholder = 'Add title or caption';
    this.mediaCaptionInput.maxLength = specLimit('attachment_caption_max_bytes');
    this.mediaCaptionInput.setAttribute('aria-label', 'Media title or caption');
    this.mediaCaptionInput.addEventListener('change', () => {
      this.updateAttrs({caption: truncateUtf8(this.mediaCaptionInput.value, specLimit('attachment_caption_max_bytes'))});
    });

    this.mediaAnnotationSummaryBar = document.createElement('div');
    this.mediaAnnotationSummaryBar.className = 'prosemirror-media-annotation-summary-bar';
    this.mediaNotesButton = document.createElement('button');
    this.mediaNotesButton.type = 'button';
    this.mediaNotesButton.className = 'prosemirror-media-notes-toggle';
    this.mediaNotesButton.textContent = 'Notes';
    this.mediaNotesButton.setAttribute('aria-label', 'Show media annotations');
    this.mediaNotesButton.setAttribute('aria-expanded', 'false');
    this.mediaNotesButton.addEventListener('click', () => this.toggleMediaAnnotationsPanel());
    this.mediaAnnotationSummaryBar.append(this.mediaNotesButton);

    this.mediaAnnotationPanel = document.createElement('section');
    this.mediaAnnotationPanel.className = 'prosemirror-media-annotations';
    this.mediaAnnotationPanel.hidden = true;
    this.mediaAnnotationToolbar = document.createElement('div');
    this.mediaAnnotationToolbar.className = 'prosemirror-media-annotations-toolbar';

    this.mediaPreviousAnnotationButton = document.createElement('button');
    this.mediaPreviousAnnotationButton.type = 'button';
    this.mediaPreviousAnnotationButton.className = 'prosemirror-media-annotation-toolbar-button';
    this.mediaPreviousAnnotationButton.textContent = '‹';
    this.mediaPreviousAnnotationButton.title = 'Previous annotation';
    this.mediaPreviousAnnotationButton.setAttribute('aria-label', 'Previous media annotation');
    this.mediaPreviousAnnotationButton.addEventListener('click', () => this.navigateMediaAnnotation(-1));

    this.mediaNextAnnotationButton = document.createElement('button');
    this.mediaNextAnnotationButton.type = 'button';
    this.mediaNextAnnotationButton.className = 'prosemirror-media-annotation-toolbar-button';
    this.mediaNextAnnotationButton.textContent = '›';
    this.mediaNextAnnotationButton.title = 'Next annotation';
    this.mediaNextAnnotationButton.setAttribute('aria-label', 'Next media annotation');
    this.mediaNextAnnotationButton.addEventListener('click', () => this.navigateMediaAnnotation(1));

    this.mediaCopyTimeButton = document.createElement('button');
    this.mediaCopyTimeButton.type = 'button';
    this.mediaCopyTimeButton.className = 'prosemirror-media-annotation-toolbar-button';
    this.mediaCopyTimeButton.textContent = 'Copy time';
    this.mediaCopyTimeButton.title = 'Copy a link to the current timestamp';
    this.mediaCopyTimeButton.addEventListener('click', async () => {
      const copied = await this.controller?.copyMediaDeepLink(this, this.currentMediaTimeMs());
      this.controller?.showEditorFeedback(copied ? 'Timestamp link copied' : 'Could not copy timestamp link', !copied);
    });

    this.mediaAnnotationToolbar.append(
      this.mediaPreviousAnnotationButton,
      this.mediaNextAnnotationButton,
      this.mediaCopyTimeButton,
    );

    this.mediaRangeStatus = document.createElement('div');
    this.mediaRangeStatus.className = 'prosemirror-media-range-status';
    this.mediaRangeStatus.hidden = true;

    this.mediaAnnotationList = document.createElement('div');
    this.mediaAnnotationList.className = 'prosemirror-media-annotation-list';
    this.mediaAnnotationList.setAttribute('role', 'list');

    this.mediaAnnotationEmpty = document.createElement('div');
    this.mediaAnnotationEmpty.className = 'prosemirror-media-annotation-empty';
    this.mediaAnnotationEmpty.textContent = 'No media notes yet. Use Mark for a point or Range for a section.';

    this.mediaAnnotationPanel.append(
      this.mediaAnnotationToolbar,
      this.mediaRangeStatus,
      this.mediaAnnotationEmpty,
      this.mediaAnnotationList,
    );
  }

  mediaAnnotations() {
    return normalizedMediaAnnotations(this.node.attrs.mediaAnnotations);
  }

  mediaTrimStartSeconds() {
    const value = Number(this.node.attrs.mediaTrimStartMs);
    return Number.isSafeInteger(value) && value > 0 ? value / 1000 : 0;
  }

  mediaTrimEndSeconds() {
    const sourceDuration = this.mediaSourceDuration();
    const value = Number(this.node.attrs.mediaTrimEndMs);
    if (!Number.isSafeInteger(value) || value <= 0) return sourceDuration;
    const end = value / 1000;
    return sourceDuration > 0 ? Math.min(sourceDuration, end) : end;
  }

  mediaSourceDuration() {
    const duration = Number(this.media?.duration);
    return Number.isFinite(duration) && duration > 0 ? duration : 0;
  }

  mediaSourceTimeFromLogical(seconds) {
    const duration = this.mediaFiniteDuration();
    const logical = Math.max(0, Number(seconds) || 0);
    return this.mediaTrimStartSeconds() + (duration > 0 ? Math.min(duration, logical) : logical);
  }

  mediaLogicalTimeFromSource(seconds) {
    const source = Number(seconds);
    if (!Number.isFinite(source)) return 0;
    const logical = Math.max(0, source - this.mediaTrimStartSeconds());
    const duration = this.mediaFiniteDuration();
    return duration > 0 ? Math.min(duration, logical) : logical;
  }

  currentMediaTimeMs() {
    const seconds = this.mediaLogicalTimeFromSource(this.media?.currentTime);
    return Math.round(seconds * 1000);
  }

  ensureMediaWithinTrimBounds() {
    if (!this.media || !this.mediaPreviewLoaded) return false;
    const start = this.mediaTrimStartSeconds();
    const end = this.mediaTrimEndSeconds();
    const current = Number(this.media.currentTime);
    if (!Number.isFinite(current) || current < start - 0.02 || (end > start && current > end + 0.02)) {
      try { this.media.currentTime = start; } catch { return false; }
    }
    return true;
  }

  enforceMediaTrimEnd() {
    if (!this.media || !this.mediaPreviewLoaded || this.mediaLoopAnnotationId || this.mediaTrimEnded) return false;
    const end = this.mediaTrimEndSeconds();
    const start = this.mediaTrimStartSeconds();
    if (!(end > start) || this.media.currentTime < end - 0.03) return false;
    try {
      this.media.pause();
      this.media.currentTime = end;
    } catch { return false; }
    this.mediaTrimEnded = true;
    this.mediaPlaybackPinned = false;
    this.persistMediaPlaybackPosition(true, this.mediaFiniteDuration() * 1000);
    return true;
  }

  newMediaAnnotationId() {
    if (globalThis.crypto && typeof globalThis.crypto.randomUUID === 'function') {
      return globalThis.crypto.randomUUID();
    }
    return `media_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 12)}`;
  }

  replaceMediaAnnotations(annotations) {
    const normalized = normalizedMediaAnnotations(annotations);
    return this.updateAttrs({mediaAnnotations: normalized.length > 0 ? normalized : null});
  }

  addMediaPointAnnotation() {
    if (!this.controller?.editable || !this.mediaPreviewLoaded) return false;
    const annotations = this.mediaAnnotations();
    if (annotations.length >= MEDIA_ANNOTATION_LIMIT) {
      this.controller.showEditorFeedback(`A media attachment can contain at most ${MEDIA_ANNOTATION_LIMIT} annotations`, true);
      return false;
    }
    const markedAtMs = this.currentMediaTimeMs();
    this.seekAndPauseMedia(markedAtMs);
    const annotation = {
      id: this.newMediaAnnotationId(),
      startMs: markedAtMs,
      endMs: null,
      label: '',
      tag: '',
      note: '',
    };
    this.replaceMediaAnnotations([...annotations, annotation]);
    this.controller.showEditorFeedback(`Bookmark added at ${this.mediaTimeLabel(markedAtMs / 1000)}`);
    return true;
  }

  toggleMediaRangeCapture() {
    if (!this.controller?.editable || !this.mediaPreviewLoaded) return false;
    const currentMs = this.currentMediaTimeMs();
    if (!Number.isSafeInteger(this.mediaRangeStartMs)) {
      this.mediaRangeStartMs = currentMs;
      this.renderMediaAnnotations();
      this.controller.showEditorFeedback(`Range start set at ${this.mediaTimeLabel(currentMs / 1000)}`);
      return true;
    }
    const startMs = Math.min(this.mediaRangeStartMs, currentMs);
    const endMs = Math.max(this.mediaRangeStartMs, currentMs);
    if (endMs - startMs < 250) {
      this.controller.showEditorFeedback('Move the playhead, then press Range again', true);
      return false;
    }
    const annotations = this.mediaAnnotations();
    if (annotations.length >= MEDIA_ANNOTATION_LIMIT) {
      this.mediaRangeStartMs = null;
      this.renderMediaAnnotations();
      this.controller.showEditorFeedback(`A media attachment can contain at most ${MEDIA_ANNOTATION_LIMIT} annotations`, true);
      return false;
    }
    const annotation = {
      id: this.newMediaAnnotationId(),
      startMs,
      endMs,
      label: '',
      tag: '',
      note: '',
    };
    this.mediaRangeStartMs = null;
    this.seekAndPauseMedia(currentMs);
    this.replaceMediaAnnotations([...annotations, annotation]);
    this.controller.showEditorFeedback(`Range saved: ${this.mediaAnnotationTimeLabel(annotation)}`);
    return true;
  }

  cancelMediaRangeCapture() {
    if (!Number.isSafeInteger(this.mediaRangeStartMs)) return false;
    this.mediaRangeStartMs = null;
    this.renderMediaAnnotations();
    return true;
  }

  toggleMediaAnnotationsPanel(force = null) {
    this.mediaAnnotationsOpen = force === null ? !this.mediaAnnotationsOpen : Boolean(force);
    this.renderMediaAnnotations();
    return true;
  }

  seekAndPauseMedia(positionMs, focus = false) {
    if (!this.media || !this.mediaPreviewLoaded) return false;
    const durationMs = Math.max(0, Math.round(this.mediaFiniteDuration() * 1000));
    const requestedMs = Math.max(0, Math.round(Number(positionMs) || 0));
    const targetMs = durationMs > 0 ? Math.min(durationMs, requestedMs) : requestedMs;
    try {
      this.media.pause();
      this.media.currentTime = this.mediaSourceTimeFromLogical(targetMs / 1000);
      this.mediaTrimEnded = false;
    } catch {
      return false;
    }
    this.mediaPlaybackPinned = false;
    this.persistMediaPlaybackPosition(true, targetMs);
    if (focus) this.mediaFrame?.focus({preventScroll: true});
    this.updateMediaControls();
    return true;
  }

  seekToMediaLink(startMs, endMs = null) {
    const normalizedStart = Math.max(0, Math.round(Number(startMs) || 0));
    const normalizedEnd = Number.isSafeInteger(Number(endMs)) && Number(endMs) > normalizedStart
      ? Math.round(Number(endMs))
      : null;
    this.mediaPendingSeek = {startMs: normalizedStart, endMs: normalizedEnd};
    this.mediaPlaybackPinned = true;
    if (this.mediaPreviewLoaded) this.applyPendingMediaSeek();
    else this.render();
    return true;
  }

  applyPendingMediaSeek() {
    if (!this.mediaPreviewLoaded || !this.mediaPendingSeek) return false;
    const duration = this.mediaFiniteDuration();
    const target = duration > 0
      ? Math.min(duration, this.mediaPendingSeek.startMs / 1000)
      : this.mediaPendingSeek.startMs / 1000;
    const targetMs = Math.round(Math.max(0, target) * 1000);
    if (!this.seekAndPauseMedia(targetMs, true)) return false;
    this.mediaPendingSeek = null;
    return true;
  }

  seekToMediaAnnotation(annotation) {
    const normalized = normalizedMediaAnnotation(annotation);
    if (!normalized) return false;
    return this.seekToMediaLink(normalized.startMs, normalized.endMs);
  }

  navigateMediaAnnotation(direction) {
    const annotations = this.mediaAnnotations();
    if (annotations.length === 0) return false;
    const currentMs = this.currentMediaTimeMs();
    let index = 0;
    if (direction < 0) {
      index = annotations.map(annotation => annotation.startMs).findLastIndex(start => start < currentMs - 100);
      if (index < 0) index = annotations.length - 1;
    } else {
      index = annotations.findIndex(annotation => annotation.startMs > currentMs + 100);
      if (index < 0) index = 0;
    }
    return this.seekToMediaAnnotation(annotations[index]);
  }

  toggleMediaLoop(annotation) {
    const normalized = normalizedMediaAnnotation(annotation);
    if (!normalized || normalized.endMs === null) return false;
    this.mediaLoopAnnotationId = this.mediaLoopAnnotationId === normalized.id ? null : normalized.id;
    if (this.mediaLoopAnnotationId) this.seekToMediaAnnotation(normalized);
    this.renderMediaAnnotations();
    return true;
  }

  applyMediaLoop() {
    if (!this.mediaLoopAnnotationId || !this.media || this.media.paused) return;
    const annotation = this.mediaAnnotations().find(candidate => candidate.id === this.mediaLoopAnnotationId);
    if (!annotation || annotation.endMs === null) {
      this.mediaLoopAnnotationId = null;
      return;
    }
    const currentMs = this.currentMediaTimeMs();
    if (currentMs + 40 < annotation.endMs) return;
    try { this.media.currentTime = this.mediaSourceTimeFromLogical(annotation.startMs / 1000); this.mediaTrimEnded = false; } catch { /* media no longer seekable */ }
  }

  applyMediaResumePosition() {
    if (!this.currentPreviewUrl || this.mediaResumeAppliedForUrl === this.currentPreviewUrl) return;
    this.mediaResumeAppliedForUrl = this.currentPreviewUrl;
    if (this.mediaPendingSeek) return;
    const resumeMs = readMediaResumePosition(this.node.attrs.attachmentId);
    this.mediaLastResumePersistedMs = resumeMs;
    const duration = this.mediaFiniteDuration();
    if (resumeMs <= 0 || (duration > 0 && resumeMs >= Math.max(0, duration * 1000 - 1500))) return;
    try { this.media.currentTime = this.mediaSourceTimeFromLogical(resumeMs / 1000); this.mediaTrimEnded = false; } catch { /* media may not be seekable yet */ }
  }

  persistMediaPlaybackPosition(force = false, overrideMs = null) {
    if (!this.mediaPreviewLoaded) return false;
    const now = Date.now();
    const nextMs = overrideMs === null ? this.currentMediaTimeMs() : Math.max(0, Math.round(Number(overrideMs) || 0));
    if (!force) {
      if (now - this.mediaLastResumePersistAt < MEDIA_RESUME_PERSIST_INTERVAL_MS) return false;
      if (Math.abs(nextMs - this.mediaLastResumePersistedMs) < 5000) return false;
    }
    if (!writeMediaResumePosition(this.node.attrs.attachmentId, nextMs)) return false;
    this.mediaLastResumePersistAt = now;
    this.mediaLastResumePersistedMs = nextMs;
    return true;
  }


  renderMediaAnnotationMarkers() {
    if (!this.mediaAnnotationTrack) return;
    this.mediaAnnotationTrack.replaceChildren();
    const duration = this.mediaFiniteDuration();
    if (duration <= 0) return;
    const durationMs = duration * 1000;
    for (const annotation of this.mediaAnnotations()) {
      const marker = document.createElement('button');
      marker.type = 'button';
      marker.className = annotation.endMs === null
        ? 'prosemirror-media-annotation-marker is-point'
        : 'prosemirror-media-annotation-marker is-range';
      if (this.mediaLoopAnnotationId === annotation.id) marker.classList.add('is-looping');
      const left = Math.max(0, Math.min(100, (annotation.startMs / durationMs) * 100));
      marker.style.left = `${left}%`;
      if (annotation.endMs !== null) {
        const right = Math.max(left, Math.min(100, (annotation.endMs / durationMs) * 100));
        marker.style.width = `${Math.max(0.35, right - left)}%`;
      }
      const label = annotation.label || annotation.note || (annotation.endMs === null ? 'Bookmark' : 'Annotated range');
      marker.title = `${this.mediaAnnotationTimeLabel(annotation)}${label ? ` — ${label}` : ''}`;
      marker.setAttribute('aria-label', marker.title);
      marker.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        this.seekToMediaAnnotation(annotation);
      });
      this.mediaAnnotationTrack.append(marker);
    }
    if (Number.isSafeInteger(this.mediaRangeStartMs)) {
      const start = document.createElement('span');
      start.className = 'prosemirror-media-range-capture-marker';
      start.style.left = `${Math.max(0, Math.min(100, (this.mediaRangeStartMs / durationMs) * 100))}%`;
      start.title = 'Range start';
      this.mediaAnnotationTrack.append(start);
    }
  }

  mediaAnnotationTimeLabel(annotation) {
    const start = this.mediaTimeLabel(annotation.startMs / 1000);
    return annotation.endMs === null ? start : `${start}–${this.mediaTimeLabel(annotation.endMs / 1000)}`;
  }

  createMediaAnnotationRow(annotation, editable) {
    const row = document.createElement('div');
    row.className = 'prosemirror-media-annotation-row';
    row.setAttribute('role', 'listitem');
    row.dataset.annotationId = annotation.id;

    const timeButton = document.createElement('button');
    timeButton.type = 'button';
    timeButton.className = 'prosemirror-media-annotation-time';
    timeButton.textContent = this.mediaAnnotationTimeLabel(annotation);
    timeButton.title = 'Seek to annotation';
    timeButton.addEventListener('click', () => this.seekToMediaAnnotation(annotation));

    const body = document.createElement('div');
    body.className = 'prosemirror-media-annotation-body';
    const title = document.createElement('div');
    title.className = 'prosemirror-media-annotation-title';
    title.textContent = annotation.label || (annotation.endMs === null ? 'Bookmark' : 'Range');
    body.append(title);
    if (annotation.tag) {
      const tag = document.createElement('span');
      tag.className = 'prosemirror-media-annotation-tag';
      tag.textContent = annotation.tag;
      body.append(tag);
    }
    if (annotation.note) {
      const note = document.createElement('div');
      note.className = 'prosemirror-media-annotation-note';
      note.textContent = annotation.note;
      body.append(note);
    }

    const actions = document.createElement('div');
    actions.className = 'prosemirror-media-annotation-actions';
    if (annotation.endMs !== null) {
      const loop = document.createElement('button');
      loop.type = 'button';
      loop.className = 'prosemirror-media-annotation-action';
      loop.textContent = this.mediaLoopAnnotationId === annotation.id ? 'Stop loop' : 'Loop';
      loop.setAttribute('aria-pressed', this.mediaLoopAnnotationId === annotation.id ? 'true' : 'false');
      loop.addEventListener('click', () => this.toggleMediaLoop(annotation));
      actions.append(loop);
    }
    const copy = document.createElement('button');
    copy.type = 'button';
    copy.className = 'prosemirror-media-annotation-action';
    copy.textContent = 'Copy link';
    copy.addEventListener('click', async () => {
      const copied = await this.controller?.copyMediaDeepLink(
        this,
        annotation.startMs,
        annotation.endMs,
        `${this.node.attrs.caption || this.node.attrs.displayName} · ${this.mediaAnnotationTimeLabel(annotation)}${annotation.label ? ` — ${annotation.label}` : ''}`,
      );
      this.controller?.showEditorFeedback(copied ? 'Media link copied' : 'Could not copy media link', !copied);
    });
    actions.append(copy);
    if (editable) {
      const edit = document.createElement('button');
      edit.type = 'button';
      edit.className = 'prosemirror-media-annotation-action';
      edit.textContent = 'Edit';
      edit.addEventListener('click', () => {
        this.mediaAnnotationEditingId = annotation.id;
        this.renderMediaAnnotations();
      });
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'prosemirror-media-annotation-action is-destructive';
      remove.textContent = 'Remove';
      remove.addEventListener('click', () => {
        if (this.mediaLoopAnnotationId === annotation.id) this.mediaLoopAnnotationId = null;
        if (this.mediaAnnotationEditingId === annotation.id) this.mediaAnnotationEditingId = null;
        this.replaceMediaAnnotations(this.mediaAnnotations().filter(candidate => candidate.id !== annotation.id));
      });
      actions.append(edit, remove);
    }
    row.append(timeButton, body, actions);

    if (editable && this.mediaAnnotationEditingId === annotation.id) {
      const editor = document.createElement('div');
      editor.className = 'prosemirror-media-annotation-editor';
      const labelInput = document.createElement('input');
      labelInput.type = 'text';
      labelInput.maxLength = MEDIA_ANNOTATION_LABEL_MAX;
      labelInput.placeholder = 'Label (optional)';
      labelInput.value = annotation.label;
      labelInput.setAttribute('aria-label', 'Annotation label');
      const tagInput = document.createElement('input');
      tagInput.type = 'text';
      tagInput.maxLength = MEDIA_ANNOTATION_TAG_MAX;
      tagInput.placeholder = 'Tag (optional)';
      tagInput.value = annotation.tag;
      tagInput.setAttribute('aria-label', 'Annotation tag');
      const noteInput = document.createElement('textarea');
      noteInput.maxLength = MEDIA_ANNOTATION_NOTE_MAX;
      noteInput.rows = 2;
      noteInput.placeholder = 'Note (optional)';
      noteInput.value = annotation.note;
      noteInput.setAttribute('aria-label', 'Annotation note');
      const editorActions = document.createElement('div');
      editorActions.className = 'prosemirror-media-annotation-editor-actions';
      const save = document.createElement('button');
      save.type = 'button';
      save.className = 'prosemirror-media-annotation-action is-primary';
      save.textContent = 'Save';
      save.addEventListener('click', () => {
        const updated = {
          ...annotation,
          label: labelInput.value.slice(0, MEDIA_ANNOTATION_LABEL_MAX),
          tag: tagInput.value.slice(0, MEDIA_ANNOTATION_TAG_MAX),
          note: noteInput.value.slice(0, MEDIA_ANNOTATION_NOTE_MAX),
        };
        this.mediaAnnotationEditingId = null;
        this.replaceMediaAnnotations(this.mediaAnnotations().map(candidate => candidate.id === annotation.id ? updated : candidate));
      });
      const cancel = document.createElement('button');
      cancel.type = 'button';
      cancel.className = 'prosemirror-media-annotation-action';
      cancel.textContent = 'Cancel';
      cancel.addEventListener('click', () => {
        this.mediaAnnotationEditingId = null;
        this.renderMediaAnnotations();
      });
      editorActions.append(save, cancel);
      editor.append(labelInput, tagInput, noteInput, editorActions);
      row.append(editor);
      window.requestAnimationFrame(() => labelInput.focus({preventScroll: true}));
    }
    return row;
  }

  renderMediaAnnotations() {
    if (!this.mediaAnnotationPanel) return;
    const annotations = this.mediaAnnotations();
    const editable = Boolean(this.controller?.editable);
    if (this.mediaLoopAnnotationId && !annotations.some(annotation => annotation.id === this.mediaLoopAnnotationId)) {
      this.mediaLoopAnnotationId = null;
    }
    if (this.mediaAnnotationEditingId && !annotations.some(annotation => annotation.id === this.mediaAnnotationEditingId)) {
      this.mediaAnnotationEditingId = null;
    }
    this.mediaAnnotationSummaryBar.hidden = !editable && annotations.length === 0;
    this.mediaAnnotationPanel.hidden = !this.mediaAnnotationsOpen || this.mediaAnnotationSummaryBar.hidden;
    this.mediaNotesButton.textContent = annotations.length > 0 ? `Notes ${annotations.length}` : 'Notes';
    this.mediaNotesButton.setAttribute('aria-expanded', this.mediaAnnotationsOpen ? 'true' : 'false');
    this.mediaNotesButton.setAttribute('aria-label', `${this.mediaAnnotationsOpen ? 'Hide' : 'Show'} media annotations${annotations.length > 0 ? ` (${annotations.length})` : ''}`);
    this.mediaPreviousAnnotationButton.disabled = annotations.length === 0;
    this.mediaNextAnnotationButton.disabled = annotations.length === 0;
    this.mediaCopyTimeButton.disabled = !this.mediaPreviewLoaded;
    this.mediaAnnotationEmpty.hidden = annotations.length > 0;
    const annotationEditorActive = Boolean(
      this.mediaAnnotationEditingId && this.mediaAnnotationPanel.contains(document.activeElement),
    );
    if (this.mediaAnnotationsOpen && !annotationEditorActive) {
      this.mediaAnnotationList.replaceChildren(...annotations.map(annotation => this.createMediaAnnotationRow(annotation, editable)));
    }
    this.mediaMarkButton.disabled = !editable || !this.mediaPreviewLoaded;
    this.mediaRangeButton.disabled = !editable || !this.mediaPreviewLoaded;
    const capturingRange = Number.isSafeInteger(this.mediaRangeStartMs);
    this.mediaRangeButton.classList.toggle('is-active', capturingRange);
    this.mediaRangeButton.textContent = capturingRange ? 'Finish' : 'Range';
    this.mediaRangeButton.setAttribute('aria-label', capturingRange ? 'Finish range annotation' : 'Start range annotation');
    this.mediaRangeStatus.hidden = !capturingRange;
    if (capturingRange) {
      this.mediaRangeStatus.textContent = `Range starts at ${this.mediaTimeLabel(this.mediaRangeStartMs / 1000)}. Move the playhead and press Finish.`;
    }
    if (this.mediaCaptionInput) {
      if (document.activeElement !== this.mediaCaptionInput) this.mediaCaptionInput.value = this.node.attrs.caption || '';
      this.mediaCaptionInput.disabled = !editable;
      this.mediaCaptionInput.hidden = !editable && !String(this.node.attrs.caption || '').trim();
    }
    this.renderMediaAnnotationMarkers();
  }

  mediaTimeLabel(seconds) {
    return mediaTimeLabelStatic(seconds);
  }

  mediaFiniteDuration() {
    const sourceDuration = this.mediaSourceDuration();
    const start = this.mediaTrimStartSeconds();
    const end = this.mediaTrimEndSeconds();
    if (sourceDuration <= 0 && end <= 0) return 0;
    const effectiveEnd = end > 0 ? end : sourceDuration;
    return Math.max(0, effectiveEnd - start);
  }

  updateMediaControls() {
    if (!this.media || !this.mediaControls) return;
    const duration = this.mediaFiniteDuration();
    const currentTime = this.mediaLogicalTimeFromSource(this.media.currentTime);
    this.mediaPlayButton.textContent = this.media.paused || this.media.ended ? '▶' : 'Ⅱ';
    this.mediaPlayButton.setAttribute('aria-label', this.media.paused || this.media.ended ? 'Play' : 'Pause');
    this.mediaPlayButton.title = this.media.paused || this.media.ended ? 'Play (Space)' : 'Pause (Space)';
    this.mediaDuration.textContent = duration > 0 ? this.mediaTimeLabel(duration) : '0:00';
    if (!this.mediaSeekActive) {
      this.mediaCurrentTime.textContent = this.mediaTimeLabel(currentTime);
      this.mediaSeek.value = duration > 0 ? String(Math.round((Math.min(currentTime, duration) / duration) * 1000)) : '0';
      this.mediaSeek.setAttribute('aria-valuetext', `${this.mediaTimeLabel(currentTime)} of ${duration > 0 ? this.mediaTimeLabel(duration) : '0:00'}`);
      const playedPercent = duration > 0 ? Math.max(0, Math.min(100, (currentTime / duration) * 100)) : 0;
      this.mediaSeek.style.setProperty('--media-played', `${playedPercent}%`);
    }
    this.mediaSeek.disabled = duration <= 0;
    const muted = this.media.muted || this.media.volume <= 0;
    this.mediaMuteButton.textContent = muted ? 'Unmute' : 'Mute';
    this.mediaMuteButton.setAttribute('aria-label', muted ? 'Unmute' : 'Mute');
    this.mediaVolume.value = String(this.media.muted ? 0 : this.media.volume);
    this.mediaVolume.setAttribute('aria-valuetext', `${Math.round((this.media.muted ? 0 : this.media.volume) * 100)}%`);
    const currentRate = Number(this.media.playbackRate) || 1;
    const matchingRate = [...this.mediaSpeed.options].some(option => Number(option.value) === currentRate);
    if (matchingRate) this.mediaSpeed.value = String(currentRate);
    this.updateMediaBufferedProgress();
  }

  updateMediaBufferedProgress() {
    if (!this.mediaSeek || !this.media) return;
    const duration = this.mediaFiniteDuration();
    let bufferedEnd = 0;
    if (duration > 0 && this.media.buffered?.length) {
      try {
        for (let index = 0; index < this.media.buffered.length; index += 1) {
          bufferedEnd = Math.max(bufferedEnd, this.mediaLogicalTimeFromSource(this.media.buffered.end(index)));
        }
      } catch { /* transient media range state */ }
    }
    const percent = duration > 0 ? Math.max(0, Math.min(100, (bufferedEnd / duration) * 100)) : 0;
    this.mediaSeek.style.setProperty('--media-buffered', `${percent}%`);
  }

  previewMediaSeek() {
    const duration = this.mediaFiniteDuration();
    if (duration <= 0) return;
    const fraction = Math.max(0, Math.min(1, Number(this.mediaSeek.value) / 1000));
    const previewTime = duration * fraction;
    this.mediaCurrentTime.textContent = this.mediaTimeLabel(previewTime);
    this.mediaSeek.setAttribute('aria-valuetext', `${this.mediaTimeLabel(previewTime)} of ${this.mediaTimeLabel(duration)}`);
  }

  commitMediaSeek() {
    const duration = this.mediaFiniteDuration();
    if (duration <= 0) return;
    const fraction = Math.max(0, Math.min(1, Number(this.mediaSeek.value) / 1000));
    try { this.media.currentTime = this.mediaSourceTimeFromLogical(duration * fraction); this.mediaTrimEnded = false; } catch { /* media is no longer seekable */ }
    this.updateMediaControls();
  }

  toggleMediaPlayback() {
    if (!this.media || !this.mediaPreviewLoaded) return;
    if (this.media.paused || this.media.ended || this.mediaTrimEnded) {
      if (this.mediaTrimEnded || this.currentMediaTimeMs() >= Math.max(0, Math.round(this.mediaFiniteDuration() * 1000) - 40)) {
        try { this.media.currentTime = this.mediaTrimStartSeconds(); } catch { /* media is no longer seekable */ }
      } else {
        this.ensureMediaWithinTrimBounds();
      }
      this.mediaTrimEnded = false;
      const playResult = this.media.play();
      if (playResult && typeof playResult.catch === 'function') playResult.catch(() => {});
    } else {
      this.media.pause();
    }
  }

  seekMediaBy(seconds) {
    const duration = this.mediaFiniteDuration();
    if (duration <= 0) return;
    const current = this.mediaLogicalTimeFromSource(this.media.currentTime);
    const logical = Math.max(0, Math.min(duration, current + seconds));
    try { this.media.currentTime = this.mediaSourceTimeFromLogical(logical); this.mediaTrimEnded = false; } catch { /* not seekable */ }
  }

  adjustMediaVolume(delta) {
    const current = this.media.muted ? 0 : this.media.volume;
    this.media.volume = Math.max(0, Math.min(1, current + delta));
    this.media.muted = false;
  }

  handleMediaKeydown(event) {
    const target = event.target;
    if (target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLButtonElement) return;
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const key = String(event.key || '').toLowerCase();
    if (key === ' ' || key === 'k') {
      event.preventDefault();
      this.toggleMediaPlayback();
    } else if (key === 'arrowleft') {
      event.preventDefault();
      this.seekMediaBy(-5);
    } else if (key === 'arrowright') {
      event.preventDefault();
      this.seekMediaBy(5);
    } else if (key === 'j') {
      event.preventDefault();
      this.seekMediaBy(-10);
    } else if (key === 'l') {
      event.preventDefault();
      this.seekMediaBy(10);
    } else if (key === 'arrowup') {
      event.preventDefault();
      this.adjustMediaVolume(0.05);
    } else if (key === 'arrowdown') {
      event.preventDefault();
      this.adjustMediaVolume(-0.05);
    } else if (key === 'm') {
      event.preventDefault();
      this.media.muted = !this.media.muted;
    } else if (key === 'b') {
      event.preventDefault();
      this.addMediaPointAnnotation();
    } else if (key === 'r') {
      event.preventDefault();
      this.toggleMediaRangeCapture();
    } else if (key === 'escape' && this.mediaMoreControls?.classList.contains('is-open')) {
      event.preventDefault();
      this.mediaMoreControls.classList.remove('is-open');
      this.mediaMoreSummary?.setAttribute('aria-expanded', 'false');
    } else if (key === 'escape' && Number.isSafeInteger(this.mediaRangeStartMs)) {
      event.preventDefault();
      this.cancelMediaRangeCapture();
    } else if (key === 'f' && this.node.attrs.kind === 'video') {
      event.preventDefault();
      this.toggleMediaFullscreen();
    } else if (key === 'p' && this.node.attrs.kind === 'video') {
      event.preventDefault();
      this.toggleMediaPictureInPicture();
    }
  }

  mediaFullscreenElement() {
    return document.fullscreenElement || document.webkitFullscreenElement || null;
  }

  mediaFullscreenSupported() {
    return this.node.attrs.kind === 'video'
      && this.mediaFrame
      && (typeof this.mediaFrame.requestFullscreen === 'function'
        || typeof this.mediaFrame.webkitRequestFullscreen === 'function'
        || typeof this.media?.webkitEnterFullscreen === 'function');
  }

  async toggleMediaFullscreen() {
    if (!this.mediaFullscreenSupported()) return;
    try {
      if (this.mediaFullscreenElement()) {
        if (typeof document.exitFullscreen === 'function') await document.exitFullscreen();
        else if (typeof document.webkitExitFullscreen === 'function') document.webkitExitFullscreen();
      } else if (typeof this.mediaFrame.requestFullscreen === 'function') {
        await this.mediaFrame.requestFullscreen();
      } else if (typeof this.mediaFrame.webkitRequestFullscreen === 'function') {
        this.mediaFrame.webkitRequestFullscreen();
      } else if (typeof this.media.webkitEnterFullscreen === 'function') {
        this.media.webkitEnterFullscreen();
      }
    } catch { /* fullscreen may be denied by the host */ }
    this.updateMediaAdvancedControls();
  }

  mediaPictureInPictureSupported() {
    if (this.node.attrs.kind !== 'video' || !this.media) return false;
    const standard = document.pictureInPictureEnabled === true
      && typeof this.media.requestPictureInPicture === 'function';
    if (standard) return true;
    try {
      return typeof this.media.webkitSupportsPresentationMode === 'function'
        && typeof this.media.webkitSetPresentationMode === 'function'
        && this.media.webkitSupportsPresentationMode('picture-in-picture');
    } catch {
      return false;
    }
  }

  mediaPictureInPictureActive() {
    return document.pictureInPictureElement === this.media
      || this.media?.webkitPresentationMode === 'picture-in-picture';
  }

  async toggleMediaPictureInPicture() {
    if (!this.mediaPictureInPictureSupported()) return;
    const webkitToggle = () => {
      if (typeof this.media.webkitSetPresentationMode !== 'function') return false;
      try {
        this.media.webkitSetPresentationMode(
          this.media.webkitPresentationMode === 'picture-in-picture' ? 'inline' : 'picture-in-picture',
        );
        return true;
      } catch {
        return false;
      }
    };
    try {
      if (document.pictureInPictureElement === this.media) {
        if (typeof document.exitPictureInPicture === 'function') await document.exitPictureInPicture();
      } else if (document.pictureInPictureEnabled === true && typeof this.media.requestPictureInPicture === 'function') {
        try {
          await this.media.requestPictureInPicture();
        } catch {
          webkitToggle();
        }
      } else {
        webkitToggle();
      }
    } catch { /* picture in picture may be denied by the host */ }
    this.updateMediaAdvancedControls();
  }

  updateMediaAdvancedControls() {
    if (this.mediaPictureInPictureButton) {
      const supported = this.mediaPictureInPictureSupported();
      this.mediaPictureInPictureButton.hidden = !supported;
      const active = this.mediaPictureInPictureActive();
      this.mediaPictureInPictureButton.textContent = active ? 'Exit PiP' : 'PiP';
      this.mediaPictureInPictureButton.setAttribute('aria-label', active ? 'Exit Picture in Picture' : 'Picture in Picture');
    }
    if (this.mediaFullscreenButton) {
      const supported = this.mediaFullscreenSupported();
      this.mediaFullscreenButton.hidden = !supported;
      const active = this.mediaFullscreenElement() === this.mediaFrame;
      this.mediaFullscreenButton.textContent = active ? 'Exit' : 'Full';
      this.mediaFullscreenButton.setAttribute('aria-label', active ? 'Exit fullscreen' : 'Enter fullscreen');
    }
  }

  mediaPlaybackCapability(mediaType) {
    const normalized = String(mediaType || '').trim().toLowerCase();
    if (!normalized || (!normalized.startsWith('audio/') && !normalized.startsWith('video/'))) {
      return 'unknown';
    }
    if (!this.media || typeof this.media.canPlayType !== 'function') return 'unknown';
    try {
      const support = this.media.canPlayType(normalized);
      return support && support !== 'no' ? 'supported' : 'unsupported';
    } catch {
      return 'unknown';
    }
  }

  setMediaStatus(message, showExternalOpen = false) {
    this.media.hidden = true;
    if (this.mediaControls) this.mediaControls.hidden = true;
    this.mediaMissing.hidden = false;
    this.mediaMissingText.textContent = message;
    this.mediaFallbackButton.hidden = !showExternalOpen;
  }

  mediaCompatibilityCandidate(runtime) {
    const candidates = Array.isArray(runtime?.compatiblePlaybackCandidates)
      ? runtime.compatiblePlaybackCandidates
      : [];
    for (const candidate of candidates) {
      const profile = typeof candidate?.profile === 'string' ? candidate.profile.trim() : '';
      const url = typeof candidate?.previewUrl === 'string' ? candidate.previewUrl : '';
      const mediaType = typeof candidate?.mimeType === 'string' ? candidate.mimeType.trim().toLowerCase() : '';
      if (!profile || !url || !mediaType) continue;
      if (this.mediaFailedPreviewUrls.has(url)) continue;
      if (this.controller?.mediaCompatibilityProfileFailed(this.node?.attrs?.attachmentId, profile)) continue;
      if (this.mediaPlaybackCapability(mediaType) === 'unsupported') continue;
      return {profile, url, mediaType};
    }
    return null;
  }

  mediaCompatibilityPotential(runtime) {
    return Boolean(this.mediaCompatibilityCandidate(runtime));
  }

  mediaCandidates(runtime, attrs) {
    const runtimeMediaType = runtime && typeof runtime.mimeType === 'string' ? runtime.mimeType.trim() : '';
    const nativeUrl = runtime && typeof runtime.previewUrl === 'string' ? runtime.previewUrl : '';
    const nativeType = (runtimeMediaType || (typeof attrs.mediaType === 'string' ? attrs.mediaType : '')).trim().toLowerCase();
    const compatible = this.mediaCompatibilityCandidate(runtime);
    const compatibilityState = this.controller?.mediaCompatibilityState(attrs.attachmentId) || 'unknown';
    const compatibilityProfile = this.controller?.mediaCompatibilityProfile(attrs.attachmentId) || '';
    const candidates = [];
    const appendCandidate = (url, mediaType, source, profile = '') => {
      if (!url || candidates.some(candidate => candidate.url === url) || this.mediaFailedPreviewUrls.has(url)) return;
      if (this.mediaPlaybackCapability(mediaType) === 'unsupported') return;
      candidates.push({url, mediaType, source, profile});
    };
    appendCandidate(nativeUrl, nativeType, 'native');
    if (
      compatibilityState === 'ready'
      && compatible
      && compatibilityProfile === compatible.profile
    ) {
      appendCandidate(compatible.url, compatible.mediaType, 'compatible', compatible.profile);
    }
    return {
      candidates,
      hasAnyPreview: Boolean(nativeUrl || compatible?.url),
      compatibilityPotential: Boolean(compatible),
      compatibilityState,
      compatibilityProfile,
    };
  }

  setMediaPreviewCandidates(candidates) {
    const key = candidates.map(candidate => `${candidate.source}:${candidate.mediaType}:${candidate.url}`).join('\n');
    if (key === this.mediaPreviewCandidatesKey) return;
    this.releaseMediaSource();
    this.mediaPreviewCandidates = candidates;
    this.mediaPreviewCandidatesKey = key;
    this.mediaPreviewCandidateIndex = candidates.length > 0 ? 0 : -1;
  }

  currentMediaPreviewCandidate() {
    if (this.mediaPreviewCandidateIndex < 0) return null;
    return this.mediaPreviewCandidates[this.mediaPreviewCandidateIndex] || null;
  }

  advanceMediaPreviewCandidate() {
    const nextIndex = this.mediaPreviewCandidateIndex + 1;
    if (nextIndex < 0 || nextIndex >= this.mediaPreviewCandidates.length) return false;
    this.releaseMediaSource();
    this.mediaPreviewCandidateIndex = nextIndex;
    this.render();
    return true;
  }

  releaseMediaSource() {
    if (!this.media || !this.mediaAttachedPreviewUrl) return;
    this.mediaSourceResetting = true;
    this.mediaPlaybackPinned = false;
    this.media.pause();
    this.media.removeAttribute('src');
    this.mediaAttachedPreviewUrl = '';
    this.mediaPreviewLoaded = false;
    this.mediaResumeAppliedForUrl = '';
    this.mediaMoreControls?.classList.remove('is-open');
    this.mediaMoreSummary?.setAttribute('aria-expanded', 'false');
    if (this.mediaControls) this.mediaControls.hidden = true;
    if (this.mediaSeek) {
      this.mediaSeek.value = '0';
      this.mediaSeek.style.setProperty('--media-played', '0%');
      this.mediaSeek.style.setProperty('--media-buffered', '0%');
    }
    if (this.mediaCurrentTime) this.mediaCurrentTime.textContent = '0:00';
    if (this.mediaDuration) this.mediaDuration.textContent = '0:00';
    this.media.load();
    this.mediaSourceResetting = false;
  }

  activateMediaSource(previewUrl) {
    if (!previewUrl || this.mediaAttachedPreviewUrl === previewUrl) return;
    this.releaseMediaSource();
    this.mediaPreviewLoaded = false;
    this.mediaAttachedPreviewUrl = previewUrl;
    this.media.src = previewUrl;
    this.media.load();
  }

  buildImageSurface() {
    this.dom.className = 'prosemirror-attachment-node prosemirror-image-node';
    this.imageShell = document.createElement('div');
    this.imageShell.className = 'prosemirror-image-shell';
    this.imageFrame = document.createElement('div');
    this.imageFrame.className = 'prosemirror-image-frame';
    this.image = document.createElement('img');
    this.image.className = 'prosemirror-image';
    this.image.draggable = false;
    this.imagePreviewLoaded = false;
    this.imagePrimaryPreviewUrl = '';
    this.imageCompatiblePreviewUrl = '';
    this.imageScreenPreviewUrl = '';
    this.imagePreviewCandidates = [];
    this.imagePreviewCandidateIndex = -1;
    this.imageNearViewport = typeof IntersectionObserver !== 'function';
    this.image.addEventListener('load', () => {
      if (!this.image.currentSrc && !this.image.src) return;
      this.imagePreviewLoaded = true;
      this.image.hidden = false;
      this.missing.hidden = true;
      this.render();
    });
    this.image.addEventListener('error', () => {
      if (this.advanceInlineImagePreviewCandidate()) return;
      this.imagePreviewLoaded = false;
      this.image.hidden = true;
      this.missing.hidden = false;
      this.missing.textContent = 'Image preview unavailable';
      this.render();
    });
    this.image.addEventListener('dblclick', event => {
      event.preventDefault();
      event.stopPropagation();
      if (this.controller) this.controller.toggleImageFocus(this);
    });
    this.missing = document.createElement('div');
    this.missing.className = 'prosemirror-image-missing';
    this.missing.textContent = 'Missing image';
    this.resizeHandle = document.createElement('button');
    this.resizeHandle.type = 'button';
    this.resizeHandle.className = 'prosemirror-image-resize-handle';
    this.resizeHandle.title = 'Resize image';
    this.resizeHandle.setAttribute('aria-label', 'Resize image');
    this.resizeHandle.addEventListener('pointerdown', event => this.beginResize(event));
    this.imageFrame.append(this.image, this.missing, this.resizeHandle);
    if (typeof IntersectionObserver === 'function') {
      const scrollRoot = this.view.dom.closest('.markdown-preview');
      this.imageVisibilityObserver = new IntersectionObserver(entries => {
        const entry = entries[entries.length - 1];
        const near = Boolean(entry && (entry.isIntersecting || entry.intersectionRatio > 0));
        if (near === this.imageNearViewport) return;
        this.imageNearViewport = near;
        this.render();
      }, {root: scrollRoot instanceof Element ? scrollRoot : null, rootMargin: '150% 0px', threshold: 0});
      this.imageVisibilityObserver.observe(this.dom);
    }

    this.captionInput = document.createElement('input');
    this.captionInput.type = 'text';
    this.captionInput.className = 'prosemirror-image-caption';
    this.captionInput.placeholder = 'Add caption';
    this.captionInput.maxLength = specLimit('attachment_caption_max_bytes');
    this.captionInput.setAttribute('aria-label', 'Image caption');
    this.captionInput.addEventListener('change', () => {
      this.updateAttrs({caption: truncateUtf8(this.captionInput.value, specLimit('attachment_caption_max_bytes'))});
    });
    this.captionInput.addEventListener('blur', () => {
      this.captionEditing = false;
      this.render();
    });

    this.controls = document.createElement('div');
    this.controls.className = 'prosemirror-image-controls';
    this.controls.setAttribute('role', 'toolbar');
    this.controls.setAttribute('aria-label', 'Image actions');

    this.viewButton = this.createImageActionButton('View', () => { if (this.controller) this.controller.toggleImageFocus(this); });
    this.controls.append(this.viewButton);

    const edit = this.createImageMenu('Edit', 'Image editing');
    this.editButton = edit.button;
    this.appendImageMenuItem(edit.menu, 'Crop & Rotate', () => this.controller?.enterImageEditFocus(this, 'crop'));
    this.appendImageMenuItem(edit.menu, 'Markup', () => this.controller?.enterImageEditFocus(this, 'draw'));
    this.appendImageMenuSeparator(edit.menu);
    this.appendImageMenuItem(edit.menu, 'Open Image Editor', () => this.controller?.enterImageEditFocus(this, 'select'));
    this.controls.append(edit.wrap);

    const align = this.createImageMenu('Align', 'Image alignment');
    this.alignmentButtons = new Map();
    const alignmentLabels = {left: 'Left', center: 'Centre', right: 'Right'};
    for (const alignment of specDomain('attachment_alignment')) {
      const label = alignmentLabels[alignment] || alignment;
      const item = this.appendImageMenuItem(align.menu, label, () => this.updateAttrs({alignment}));
      item.setAttribute('role', 'menuitemradio');
      this.alignmentButtons.set(alignment, item);
    }
    this.controls.append(align.wrap);

    const size = this.createImageMenu('Size', 'Image size');
    this.sizeButtons = new Map();
    const natural = this.appendImageMenuItem(size.menu, 'Natural', () => this.updateAttrs({sizeMode: 'auto'}));
    natural.setAttribute('role', 'menuitemradio');
    this.sizeButtons.set('auto', natural);
    this.appendImageMenuSeparator(size.menu);
    for (const [label, widthPercent] of [['Small', 35], ['Medium', 55], ['Large', 75], ['Full width', 100]]) {
      const item = this.appendImageMenuItem(size.menu, label, () => this.updateAttrs({widthPercent, sizeMode: 'percent'}));
      item.setAttribute('role', 'menuitemradio');
      this.sizeButtons.set(String(widthPercent), item);
    }
    this.appendImageMenuSeparator(size.menu);
    this.appendImageMenuItem(size.menu, 'Reset size', () => this.updateAttrs({
      widthPercent: schema.nodes.attachment.defaultAttrs.widthPercent,
      sizeMode: schema.nodes.attachment.defaultAttrs.sizeMode,
    }));
    this.controls.append(size.wrap);

    this.replaceInput = document.createElement('input');
    this.replaceInput.type = 'file';
    this.replaceInput.accept = 'image/*,.heic,.heif';
    this.replaceInput.hidden = true;
    this.replaceInput.addEventListener('change', () => {
      const file = this.replaceInput.files ? this.replaceInput.files[0] : undefined;
      this.replaceInput.value = '';
      if (file && this.controller) this.controller.queueImageImport([file], {mode: 'replace', attachmentView: this});
    });
    this.controls.append(this.replaceInput);
    this.replaceButton = this.createImageActionButton('Replace', () => this.replaceInput.click());
    this.controls.append(this.replaceButton);

    const overflow = this.createImageMenu('•••', 'More image actions', {compact: true});
    this.copyButton = this.appendImageMenuItem(overflow.menu, 'Copy Image', () => this.copyImage());
    this.saveDisplayedButton = this.appendImageMenuItem(overflow.menu, 'Save Image As…', () => this.saveDisplayedAs());
    this.saveOriginalButton = this.appendImageMenuItem(overflow.menu, 'Save original…', () => this.saveOriginalAs());
    this.saveOriginalButton.hidden = true;
    this.openButton = this.appendImageMenuItem(overflow.menu, 'Open Original', () => this.openOriginal());
    this.appendImageMenuSeparator(overflow.menu);
    this.appendImageMenuItem(overflow.menu, 'Caption', () => this.editCaption());
    this.detailsButton = this.appendImageMenuItem(overflow.menu, 'Image Details…', () => this.openImageDetails());
    this.resetOriginalButton = this.appendImageMenuItem(overflow.menu, 'Reset to Original', () => this.resetToOriginal());
    this.appendImageMenuSeparator(overflow.menu);
    this.deleteButton = this.appendImageMenuItem(overflow.menu, 'Delete', () => this.deleteNode(), {danger: true});
    this.controls.append(overflow.wrap);

    this.detailsPanel = document.createElement('div');
    this.detailsPanel.className = 'prosemirror-image-details-panel';
    this.detailsPanel.hidden = true;
    this.detailsPanel.setAttribute('role', 'dialog');
    this.detailsPanel.setAttribute('aria-label', 'Image details');
    this.detailsPanel.tabIndex = -1;
    this.detailsGrid = document.createElement('dl');
    this.detailsGrid.className = 'prosemirror-image-details-grid';
    this.altLabel = document.createElement('label');
    this.altLabel.className = 'prosemirror-image-description-label';
    const altPrefix = document.createElement('span');
    altPrefix.textContent = 'Description / alt text';
    this.altInput = document.createElement('textarea');
    this.altInput.className = 'prosemirror-image-alt';
    this.altInput.placeholder = 'Optional description for accessibility';
    this.altInput.maxLength = specLimit('attachment_alt_text_max_bytes');
    this.altInput.rows = 2;
    this.altInput.setAttribute('aria-label', 'Image description or alt text');
    this.altInput.addEventListener('change', () => {
      this.updateAttrs({altText: truncateUtf8(this.altInput.value, specLimit('attachment_alt_text_max_bytes'))});
    });
    this.altLabel.append(altPrefix, this.altInput);
    const detailsDone = this.createImageActionButton('Done', () => this.closeImageMenus());
    detailsDone.classList.add('prosemirror-image-details-done');
    this.detailsPanel.append(this.detailsGrid, this.altLabel, detailsDone);
    this.controls.append(this.detailsPanel);

    this.imageShell.append(this.imageFrame, this.captionInput);
    this.dom.append(this.imageShell, this.controls);
  }

  createImageActionButton(label, action, {compact = false} = {}) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'prosemirror-image-action';
    if (compact) button.classList.add('is-compact');
    button.textContent = label;
    button.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      this.closeImageMenus();
      action();
    });
    return button;
  }

  createImageMenu(label, ariaLabel, {compact = false} = {}) {
    const wrap = document.createElement('div');
    wrap.className = 'prosemirror-image-menu-wrap';
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'prosemirror-image-action';
    if (compact) button.classList.add('is-compact');
    button.textContent = label;
    button.setAttribute('aria-haspopup', 'menu');
    button.setAttribute('aria-expanded', 'false');
    const menu = document.createElement('div');
    menu.className = 'prosemirror-image-menu';
    menu.hidden = true;
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', ariaLabel);
    button.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      const open = menu.hidden;
      this.closeImageMenus();
      if (open) {
        menu.hidden = false;
        button.setAttribute('aria-expanded', 'true');
      }
    });
    wrap.append(button, menu);
    if (!this.imageMenus) this.imageMenus = [];
    this.imageMenus.push({button, menu});
    return {wrap, button, menu};
  }

  appendImageMenuItem(menu, label, action, {danger = false, disabled = false, title = ''} = {}) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'prosemirror-image-menu-item';
    if (danger) item.classList.add('is-danger');
    item.textContent = label;
    item.disabled = disabled;
    if (title) item.title = title;
    item.setAttribute('role', 'menuitem');
    if (typeof action === 'function') {
      item.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        if (item.disabled) return;
        this.closeImageMenus();
        action();
      });
    }
    menu.append(item);
    return item;
  }

  appendImageMenuSeparator(menu) {
    const separator = document.createElement('div');
    separator.className = 'prosemirror-image-menu-separator';
    separator.setAttribute('role', 'separator');
    menu.append(separator);
  }

  closeImageMenus() {
    for (const {button, menu} of this.imageMenus || []) {
      menu.hidden = true;
      button.setAttribute('aria-expanded', 'false');
    }
    if (this.detailsPanel) this.detailsPanel.hidden = true;
    if (this.imageFocusDetailsPanel) this.imageFocusDetailsPanel.hidden = true;
  }

  editCaption() {
    if (!this.controller || !this.controller.editable) return;
    this.captionEditing = true;
    this.render();
    window.requestAnimationFrame(() => { if (this.captionInput) this.captionInput.focus({preventScroll: true}); });
  }

  openImageDetails() {
    this.closeImageMenus();
    this.populateImageDetails();
    this.detailsPanel.hidden = false;
    window.requestAnimationFrame(() => {
      if (this.controller && this.controller.editable && this.altInput) this.altInput.focus({preventScroll: true});
      else if (this.detailsPanel) this.detailsPanel.focus({preventScroll: true});
    });
  }

  imageDetailRows() {
    const attrs = this.node.attrs;
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(attrs.attachmentId) || {}
      : {};
    const pixelSize = imageRuntimePixelSize(runtime);
    const dimensions = pixelSize ? `${pixelSize.width} × ${pixelSize.height} px` : 'Unknown';
    return [
      ['File', attrs.displayName || 'Image'],
      ['Format', runtime.imageFormat || attrs.mediaType || 'Unknown'],
      ['Dimensions', dimensions],
      ['File size', attachmentSizeLabel(attrs.byteSize)],
      ['Version', runtime.sourceAttachmentId ? 'Edited rendition' : 'Original'],
    ];
  }

  populateImageDetailGrid(grid) {
    if (!grid) return;
    grid.replaceChildren();
    for (const [label, value] of this.imageDetailRows()) {
      const term = document.createElement('dt');
      term.textContent = label;
      const detail = document.createElement('dd');
      detail.textContent = value;
      grid.append(term, detail);
    }
  }

  populateImageDetails() {
    this.populateImageDetailGrid(this.detailsGrid);
    this.altInput.value = this.node.attrs.altText || '';
  }


  async copyImage() {
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
    if (!runtime || runtime.available !== true) return;
    const candidates = [runtime.previewUrl, runtime.compatiblePreviewUrl]
      .filter(Boolean)
      .filter((url, index, values) => values.indexOf(url) === index);
    if (candidates.length === 0) return;
    try {
      if (!navigator.clipboard || !navigator.clipboard.write || typeof window.ClipboardItem !== 'function') {
        throw new Error('Image clipboard access is unavailable in this system WebView.');
      }
      let lastError = null;
      let originalFallbackUrl = null;
      let compatibleFetched = false;

      // Try the original directly first. If its media type is not clipboard-safe,
      // try the compatible full-resolution PNG before any main-thread canvas work.
      for (const previewUrl of candidates) {
        try {
          const response = await fetch(previewUrl);
          if (!response.ok) throw new Error('Could not read the image.');
          const blob = await response.blob();
          if (previewUrl === runtime.previewUrl) originalFallbackUrl = previewUrl;
          if (previewUrl === runtime.compatiblePreviewUrl) compatibleFetched = true;
          const supported = typeof ClipboardItem.supports !== 'function'
            || ClipboardItem.supports(blob.type);
          if (!supported) continue;
          await navigator.clipboard.write([new ClipboardItem({[blob.type]: blob})]);
          attachmentStatus('Image copied.');
          return;
        } catch (error) {
          lastError = error;
        }
      }

      // A successfully fetched compatible rendition is already PNG; converting
      // another full-resolution copy through canvas would only add memory work.
      if (!compatibleFetched && originalFallbackUrl) {
        const sourceImage = await new Promise((resolve, reject) => {
          const image = new Image();
          image.draggable = false;
          image.onload = () => resolve(image);
          image.onerror = () => reject(new Error('The image could not be decoded for the clipboard.'));
          image.src = originalFallbackUrl;
        });
        const canvas = document.createElement('canvas');
        canvas.width = sourceImage.naturalWidth;
        canvas.height = sourceImage.naturalHeight;
        const context = canvas.getContext('2d');
        if (!context) throw new Error('Could not prepare the image clipboard.');
        context.drawImage(sourceImage, 0, 0);
        const blob = await new Promise((resolve, reject) => canvas.toBlob(
          value => value ? resolve(value) : reject(new Error('Could not encode the image for the clipboard.')),
          'image/png',
        ));
        await navigator.clipboard.write([new ClipboardItem({[blob.type]: blob})]);
        attachmentStatus('Image copied.');
        return;
      }
      throw lastError || new Error('Could not copy image.');
    } catch (error) {
      attachmentStatus(error instanceof Error ? error.message : 'Could not copy image.');
    }
  }

  saveDisplayedAs() {
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
    if (!runtime || runtime.available !== true) return false;
    return emitAttachmentCommand({kind: 'file_action', action: 'save_as', attachment_id: this.node.attrs.attachmentId});
  }

  saveOriginalAs() {
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
    if (!runtime || runtime.available !== true) return;
    const attachmentId = runtime.sourceAttachmentId || this.node.attrs.attachmentId;
    const sourceRuntime = this.controller.attachmentRegistry.get(attachmentId);
    if (!sourceRuntime || sourceRuntime.available !== true) return;
    emitAttachmentCommand({kind: 'file_action', action: 'save_as', attachment_id: attachmentId});
  }

  buildImageFocusSurface() {
    this.imageFocusSurface = document.createElement('div');
    this.imageFocusSurface.className = 'prosemirror-image-focus-surface';
    this.imageFocusSurface.hidden = true;
    this.imageFocusSurface.setAttribute('role', 'dialog');
    this.imageFocusSurface.setAttribute('aria-modal', 'true');
    this.imageFocusSurface.tabIndex = -1;
    this.imageFocusSurface.setAttribute('aria-label', `Image focus: ${this.node.attrs.displayName || 'Image'}`);

    this.imageFocusStage = document.createElement('div');
    this.imageFocusStage.className = 'prosemirror-image-focus-stage';
    this.imageFocusStage.setAttribute('aria-label', 'Image viewport');
    this.imageFocusStage.tabIndex = -1;

    this.imageFocusImage = document.createElement('img');
    this.imageFocusImage.className = 'prosemirror-image-focus-image';
    this.imageFocusImage.draggable = false;
    this.imageFocusPrimaryPreviewUrl = '';
    this.imageFocusCompatiblePreviewUrl = '';
    this.imageFocusScreenPreviewUrl = '';
    this.imageFocusResolution = '';
    this.imageFocusCandidates = [];
    this.imageFocusCandidateIndex = -1;
    this.imageFocusPreviewUrl = '';
    this.imageFocusImage.addEventListener('load', () => {
      this.imageFocusImage.hidden = false;
      this.imageFocusMissing.hidden = true;
      if (this.imageFocusMode === 'fit' || !Number.isFinite(this.imageFocusScale)) this.setImageFocusFit();
      else this.applyImageFocusTransform();
      this.updateImageFocusControls();
    });
    this.imageFocusImage.addEventListener('error', () => {
      if (this.advanceImageFocusCandidate()) return;
      this.imageFocusImage.hidden = true;
      this.imageFocusMissing.hidden = false;
      this.imageFocusMissing.textContent = 'Image preview unavailable';
      this.updateImageFocusControls();
    });

    this.imageFocusMissing = document.createElement('div');
    this.imageFocusMissing.className = 'prosemirror-image-focus-missing';
    this.imageFocusMissing.textContent = 'Loading image…';
    this.imageFocusStage.append(this.imageFocusImage, this.imageFocusMissing);

    const toolbar = document.createElement('div');
    toolbar.className = 'prosemirror-image-focus-toolbar';
    toolbar.setAttribute('role', 'toolbar');
    toolbar.setAttribute('aria-label', 'Image focus controls');
    this.imageFocusToolbar = toolbar;

    this.imageFocusZoomOutButton = this.createImageActionButton('−', () => this.zoomImageFocusBy(1 / 1.25), {compact: true});
    this.imageFocusZoomOutButton.setAttribute('aria-label', 'Zoom out');
    this.imageFocusZoomOutButton.title = 'Zoom out';
    this.imageFocusFitButton = this.createImageActionButton('Fit', () => this.setImageFocusFit());
    this.imageFocusFitButton.setAttribute('aria-pressed', 'true');
    this.imageFocusFitButton.title = 'Fit image to window';
    this.imageFocusActualButton = this.createImageActionButton('100%', () => this.setImageFocusActual());
    this.imageFocusActualButton.setAttribute('aria-pressed', 'false');
    this.imageFocusActualButton.title = 'Show at actual pixel size';
    this.imageFocusZoomInButton = this.createImageActionButton('+', () => this.zoomImageFocusBy(1.25), {compact: true});
    this.imageFocusZoomInButton.setAttribute('aria-label', 'Zoom in');
    this.imageFocusZoomInButton.title = 'Zoom in';
    toolbar.append(
      this.imageFocusZoomOutButton,
      this.imageFocusFitButton,
      this.imageFocusActualButton,
      this.imageFocusZoomInButton,
    );

    const edit = this.createImageMenu('Edit', 'Image editing');
    edit.wrap.classList.add('prosemirror-image-focus-menu-wrap');
    this.imageFocusEditButton = edit.button;
    this.appendImageMenuItem(edit.menu, 'Crop & Rotate', () => this.controller?.enterImageEditFocus(this, 'crop'));
    this.appendImageMenuItem(edit.menu, 'Markup', () => this.controller?.enterImageEditFocus(this, 'draw'));
    this.appendImageMenuSeparator(edit.menu);
    this.appendImageMenuItem(edit.menu, 'Open Image Editor', () => this.controller?.enterImageEditFocus(this, 'select'));
    toolbar.append(edit.wrap);

    const overflow = this.createImageMenu('•••', 'More image actions', {compact: true});
    overflow.wrap.classList.add('prosemirror-image-focus-menu-wrap');
    overflow.button.setAttribute('aria-label', 'More image actions');
    this.imageFocusCopyButton = this.appendImageMenuItem(overflow.menu, 'Copy', () => this.copyImage());
    this.imageFocusSaveDisplayedButton = this.appendImageMenuItem(overflow.menu, 'Save Image As…', () => this.saveDisplayedAs());
    this.imageFocusSaveOriginalButton = this.appendImageMenuItem(overflow.menu, 'Save original…', () => this.saveOriginalAs());
    this.imageFocusSaveOriginalButton.hidden = true;
    this.imageFocusOpenButton = this.appendImageMenuItem(overflow.menu, 'Open Original', () => this.openOriginal());
    this.appendImageMenuSeparator(overflow.menu);
    this.imageFocusDetailsButton = this.appendImageMenuItem(overflow.menu, 'Image Details', () => this.openImageFocusDetails());
    this.imageFocusResetOriginalButton = this.appendImageMenuItem(overflow.menu, 'Reset to Original', () => this.resetToOriginal());
    this.imageFocusFullscreenButton = this.appendImageMenuItem(overflow.menu, 'Full Screen', () => { if (this.controller) this.controller.requestWindowFullscreenToggle(); });
    toolbar.append(overflow.wrap);

    const done = this.createImageActionButton('Done', () => { if (this.controller) this.controller.exitImageFocus(this); });
    done.classList.add('prosemirror-image-focus-done');
    toolbar.append(done);

    this.imageFocusDetailsPanel = document.createElement('section');
    this.imageFocusDetailsPanel.className = 'prosemirror-image-focus-details';
    this.imageFocusDetailsPanel.hidden = true;
    this.imageFocusDetailsPanel.tabIndex = -1;
    this.imageFocusDetailsPanel.setAttribute('aria-label', 'Image details');
    this.imageFocusDetailsGrid = document.createElement('dl');
    this.imageFocusDetailsGrid.className = 'prosemirror-image-details-grid';
    this.imageFocusDescription = document.createElement('div');
    this.imageFocusDescription.className = 'prosemirror-image-focus-description';
    const detailsDone = this.createImageActionButton('Done', () => this.closeImageFocusDetails());
    detailsDone.classList.add('prosemirror-image-details-done');
    this.imageFocusDetailsPanel.append(this.imageFocusDetailsGrid, this.imageFocusDescription, detailsDone);

    this.imageFocusPointers = new Map();
    this.imageFocusMode = 'fit';
    this.imageFocusScale = Number.NaN;
    this.imageFocusPanX = 0;
    this.imageFocusPanY = 0;
    this.imageFocusDragPointerId = null;
    this.imageFocusDragOrigin = null;
    this.imageFocusPinch = null;
    this.imageFocusGestureStartScale = null;
    this.imageFocusLastActivity = 0;
    this.imageFocusChromeTimer = null;

    this.imageFocusStage.addEventListener('wheel', event => this.handleImageFocusWheel(event), {passive: false});
    this.imageFocusStage.addEventListener('pointerdown', event => this.handleImageFocusPointerDown(event));
    this.imageFocusStage.addEventListener('pointermove', event => this.handleImageFocusPointerMove(event));
    this.imageFocusStage.addEventListener('pointerup', event => this.handleImageFocusPointerEnd(event));
    this.imageFocusStage.addEventListener('pointercancel', event => this.handleImageFocusPointerEnd(event));
    this.imageFocusStage.addEventListener('dblclick', event => {
      event.preventDefault();
      event.stopPropagation();
      this.toggleImageFocusFitActual();
    });
    this.imageFocusStage.addEventListener('gesturestart', event => this.handleImageFocusGestureStart(event), {passive: false});
    this.imageFocusStage.addEventListener('gesturechange', event => this.handleImageFocusGestureChange(event), {passive: false});
    this.imageFocusStage.addEventListener('gestureend', event => this.handleImageFocusGestureEnd(event), {passive: false});
    this.imageFocusSurface.addEventListener('keydown', event => this.handleImageFocusKeyDown(event));
    this.imageFocusSurface.addEventListener('pointermove', () => this.noteImageFocusActivity());
    this.imageFocusSurface.addEventListener('pointerdown', () => this.noteImageFocusActivity());
    this.imageFocusSurface.addEventListener('focusin', () => this.noteImageFocusActivity());
    this.imageFocusSurface.addEventListener('focusout', () => this.scheduleImageFocusChromeHide());

    this.imageFocusSurface.append(this.imageFocusStage, this.imageFocusDetailsPanel, toolbar);
    document.body.append(this.imageFocusSurface);
  }

  buildImageEditSurface() {
    this.imageEditSurface = document.createElement('div');
    this.imageEditSurface.className = 'prosemirror-image-edit-surface';
    this.imageEditSurface.hidden = true;
    this.imageEditSurface.setAttribute('role', 'dialog');
    this.imageEditSurface.setAttribute('aria-modal', 'true');
    this.imageEditSurface.tabIndex = -1;

    this.imageEditStage = document.createElement('div');
    this.imageEditStage.className = 'prosemirror-image-edit-stage';
    this.imageEditCanvas = document.createElement('canvas');
    this.imageEditCanvas.className = 'prosemirror-image-edit-canvas';
    this.imageEditSvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    this.imageEditSvg.classList.add('prosemirror-image-edit-overlay');
    this.imageEditSvg.setAttribute('aria-label', 'Image editing canvas');
    this.imageEditSvg.setAttribute('role', 'application');
    this.imageEditTextEditor = document.createElement('textarea');
    this.imageEditTextEditor.className = 'prosemirror-image-edit-text-editor';
    this.imageEditTextEditor.hidden = true;
    this.imageEditTextEditor.maxLength = IMAGE_EDIT_TEXT_MAX_CHARS;
    this.imageEditTextEditor.setAttribute('aria-label', 'Edit image text');
    this.imageEditTextEditor.addEventListener('input', () => this.handleImageEditTextInput());
    this.imageEditTextEditor.addEventListener('blur', () => this.finishImageEditTextEditing(true));
    this.imageEditTextEditor.addEventListener('keydown', event => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        this.finishImageEditTextEditing(false);
      } else if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
        event.preventDefault();
        this.finishImageEditTextEditing(true);
      }
    });
    this.imageEditStage.append(this.imageEditCanvas, this.imageEditSvg);

    this.imageEditContext = document.createElement('div');
    this.imageEditContext.className = 'prosemirror-image-edit-context';
    this.imageEditContext.hidden = true;
    this.imageEditContext.setAttribute('role', 'toolbar');
    this.imageEditContext.setAttribute('aria-label', 'Image edit tool options');

    this.imageEditToolbar = document.createElement('div');
    this.imageEditToolbar.className = 'prosemirror-image-edit-toolbar';
    this.imageEditToolbar.setAttribute('role', 'toolbar');
    this.imageEditToolbar.setAttribute('aria-label', 'Image editing controls');
    const tool = (label, name) => {
      const button = this.createImageActionButton(label, () => this.setImageEditTool(name));
      button.dataset.imageEditTool = name;
      this.imageEditToolbar.append(button);
      return button;
    };
    this.imageEditCropButton = tool('Crop', 'crop');
    this.imageEditRotateButton = tool('Rotate', 'rotate');
    this.imageEditDrawButton = tool('Draw', 'draw');
    this.imageEditShapeButton = tool('Shape', 'shape');
    this.imageEditTextButton = tool('Text', 'text');

    const divider = document.createElement('span');
    divider.className = 'prosemirror-image-edit-divider';
    this.imageEditToolbar.append(divider);
    this.imageEditUndoButton = this.createImageActionButton('↶', () => this.undoImageEdit(), {compact: true});
    this.imageEditUndoButton.classList.add('prosemirror-image-edit-history');
    this.imageEditUndoButton.title = 'Undo image edit (Cmd/Ctrl+Z)';
    this.imageEditUndoButton.setAttribute('aria-label', 'Undo image edit');
    this.imageEditRedoButton = this.createImageActionButton('↷', () => this.redoImageEdit(), {compact: true});
    this.imageEditRedoButton.classList.add('prosemirror-image-edit-history');
    this.imageEditRedoButton.title = 'Redo image edit (Shift+Cmd/Ctrl+Z)';
    this.imageEditRedoButton.setAttribute('aria-label', 'Redo image edit');
    this.imageEditToolbar.append(this.imageEditUndoButton, this.imageEditRedoButton);

    const actionDivider = divider.cloneNode();
    this.imageEditToolbar.append(actionDivider);
    this.imageEditCancelButton = this.createImageActionButton('Cancel', () => this.controller?.exitImageEditFocus(this, true, false));
    this.imageEditDoneButton = this.createImageActionButton('Done', () => this.commitImageEdit());
    this.imageEditDoneButton.classList.add('prosemirror-image-edit-done');
    this.imageEditToolbar.append(this.imageEditCancelButton, this.imageEditDoneButton);

    this.imageEditSourceImage = new Image();
    this.imageEditSourceImage.draggable = false;
    this.imageEditWorkingCandidates = [];
    this.imageEditWorkingCandidateIndex = -1;
    this.imageEditSourceImage.addEventListener('load', () => {
      this.imageEditSourceReady = true;
      this.imageEditStage.classList.remove('is-loading');
      this.imageEditStage.classList.remove('is-error');
      this.renderImageEditSurface();
    });
    this.imageEditSourceImage.addEventListener('error', () => {
      if (this.advanceImageEditWorkingCandidate()) return;
      this.imageEditSourceReady = false;
      this.imageEditStage.classList.remove('is-loading');
      this.imageEditStage.classList.add('is-error');
      attachmentStatus('The original image could not be loaded for editing.');
      this.updateImageEditControls();
    });

    this.imageEditSvg.addEventListener('pointerdown', event => this.handleImageEditPointerDown(event));
    this.imageEditSvg.addEventListener('pointermove', event => this.handleImageEditPointerMove(event));
    this.imageEditSvg.addEventListener('pointerenter', event => this.handleImageEditPointerHover(event));
    this.imageEditSvg.addEventListener('pointerleave', event => this.handleImageEditPointerLeave(event));
    this.imageEditSvg.addEventListener('pointerup', event => this.handleImageEditPointerEnd(event));
    this.imageEditSvg.addEventListener('pointercancel', event => this.handleImageEditPointerEnd(event));
    this.imageEditSvg.addEventListener('dblclick', event => this.handleImageEditDoubleClick(event));
    this.imageEditSurface.addEventListener('keydown', event => this.handleImageEditKeyDown(event));

    this.imageEditSurface.append(this.imageEditStage, this.imageEditTextEditor, this.imageEditContext, this.imageEditToolbar);
    document.body.append(this.imageEditSurface);

    this.imageEditRecipe = defaultImageEditRecipe();
    this.imageEditUndoStack = [];
    this.imageEditRedoStack = [];
    this.imageEditActiveTool = 'select';
    this.imageEditDrawMode = 'pen';
    this.imageEditShapeType = 'rect';
    this.imageEditColor = 'red';
    this.imageEditThickness = 'medium';
    this.imageEditTextSize = 'medium';
    this.imageEditTextBold = false;
    this.imageEditTextBackground = false;
    this.imageEditCropAspect = 'free';
    this.imageEditSelectedOverlayId = null;
    this.imageEditInteraction = null;
    this.imageEditOverlaySequence = 1;
    this.imageEditSaving = false;
    this.imageEditSourceReady = false;
    this.imageEditDisplayRect = null;
    this.imageEditTextEditingId = null;
    this.imageEditTextBefore = null;
    this.imageEditOverlayRenderFrame = 0;
    this.imageEditCommitTimer = 0;
    this.imageEditRenderWorker = null;
    this.imageEditRenderWorkerUrl = '';
    this.imageEditRenderWorkerCancel = null;
    this.imageEditStageAbortController = null;
    this.imageEditStagedToken = '';
    this.imageEditApplyStage = 'idle';
    this.imageEditPointerSource = null;
    this.imageEditPointerInside = false;
  }

  setImageEditWorkingCandidate(index) {
    const candidates = this.imageEditWorkingCandidates || [];
    if (index < 0 || index >= candidates.length) {
      this.imageEditWorkingCandidateIndex = -1;
      this.imageEditSourceImage.removeAttribute('src');
      return false;
    }
    this.imageEditWorkingCandidateIndex = index;
    this.imageEditSourceReady = false;
    this.imageEditStage.classList.add('is-loading');
    this.imageEditStage.classList.remove('is-error');
    const url = candidates[index];
    if (this.imageEditSourceImage.src !== url) {
      this.imageEditSourceImage.src = url;
    } else if (this.imageEditSourceImage.complete && this.imageEditSourceImage.naturalWidth > 0) {
      this.imageEditSourceReady = true;
      this.imageEditStage.classList.remove('is-loading');
      this.renderImageEditSurface();
    }
    return true;
  }

  advanceImageEditWorkingCandidate() {
    return this.setImageEditWorkingCandidate(this.imageEditWorkingCandidateIndex + 1);
  }

  beginImageEditSession(initialTool = 'select') {
    if (!this.controller?.editable) return false;
    if (!this.imageEditSurface) this.buildImageEditSurface();
    const currentRuntime = this.controller.attachmentRegistry.get(this.node.attrs.attachmentId);
    if (!currentRuntime || currentRuntime.available !== true) return false;
    const sourceId = currentRuntime.sourceAttachmentId || this.node.attrs.attachmentId;
    const sourceRuntime = this.controller.attachmentRegistry.get(sourceId);
    const previewUrl = sourceRuntime && sourceRuntime.available === true ? sourceRuntime.previewUrl : '';
    const compatiblePreviewUrl = sourceRuntime && sourceRuntime.available === true ? sourceRuntime.compatiblePreviewUrl : '';
    const screenPreviewUrl = sourceRuntime && sourceRuntime.available === true ? sourceRuntime.screenPreviewUrl : '';
    if (!previewUrl && !compatiblePreviewUrl && !screenPreviewUrl) {
      attachmentStatus('The original image is unavailable for editing.');
      return false;
    }
    this.imageEditSourceAttachmentId = sourceId;
    this.imageEditSourceRuntime = sourceRuntime;
    this.imageEditPrimaryPreviewUrl = previewUrl;
    this.imageEditCompatiblePreviewUrl = compatiblePreviewUrl;
    this.imageEditScreenPreviewUrl = screenPreviewUrl;
    this.imageEditWorkingCandidates = [screenPreviewUrl, previewUrl, compatiblePreviewUrl]
      .filter(Boolean)
      .filter((url, index, values) => values.indexOf(url) === index);
    this.imageEditWorkingCandidateIndex = -1;
    this.imageEditRecipe = sanitizeImageEditRecipe(currentRuntime.imageEditRecipe || null);
    this.imageEditUndoStack = [];
    this.imageEditRedoStack = [];
    this.imageEditSelectedOverlayId = null;
    this.imageEditInteraction = null;
    this.imageEditSaving = false;
    this.imageEditApplyStage = 'idle';
    this.imageEditStagedToken = '';
    this.abortImageEditStageRequest();
    this.imageEditOverlaySequence = Math.max(1, this.imageEditRecipe.overlays.length + 1);
    this.imageEditSurface.hidden = false;
    this.imageEditSurface.setAttribute('aria-label', `Image editor: ${this.node.attrs.displayName || 'Image'}`);
    this.imageEditStage.classList.remove('is-error');
    this.imageEditStage.classList.add('is-loading');
    this.setImageEditTool(initialTool);
    this.imageEditSourceReady = false;
    this.setImageEditWorkingCandidate(0);
    this.updateImageEditControls();
    window.requestAnimationFrame(() => this.imageEditSurface?.focus({preventScroll: true}));
    return true;
  }

  endImageEditSession() {
    if (!this.imageEditSurface) return;
    this.finishImageEditTextEditing(true);
    this.imageEditSurface.hidden = true;
    this.imageEditInteraction = null;
    this.imageEditSelectedOverlayId = null;
    this.imageEditSaving = false;
    this.imageEditSourceReady = false;
    this.imageEditPrimaryPreviewUrl = '';
    this.imageEditCompatiblePreviewUrl = '';
    this.imageEditScreenPreviewUrl = '';
    this.imageEditWorkingCandidates = [];
    this.imageEditWorkingCandidateIndex = -1;
    if (this.imageEditOverlayRenderFrame) {
      window.cancelAnimationFrame(this.imageEditOverlayRenderFrame);
      this.imageEditOverlayRenderFrame = 0;
    }
    if (this.imageEditCommitTimer) {
      window.clearTimeout(this.imageEditCommitTimer);
      this.imageEditCommitTimer = 0;
    }
    this.terminateImageEditRenderWorker();
    this.abortImageEditStageRequest();
    void this.discardStagedImageEdit();
    this.setImageEditApplyStage('idle');
    this.imageEditPointerSource = null;
    this.imageEditPointerInside = false;
    this.imageEditTextEditor.hidden = true;
    this.imageEditSourceImage.removeAttribute('src');
    this.imageEditCanvas.width = 1;
    this.imageEditCanvas.height = 1;
    this.imageEditSvg.replaceChildren();
  }

  setImageEditTool(tool) {
    if (!['select', 'crop', 'rotate', 'draw', 'shape', 'text'].includes(tool)) tool = 'select';
    if (this.imageEditTextEditingId) this.finishImageEditTextEditing(true);
    this.imageEditActiveTool = tool;
    this.imageEditSelectedOverlayId = null;
    for (const button of this.imageEditToolbar?.querySelectorAll('[data-image-edit-tool]') || []) {
      const active = button.dataset.imageEditTool === tool;
      button.classList.toggle('is-active', active);
      button.setAttribute('aria-pressed', active ? 'true' : 'false');
    }
    this.renderImageEditContext();
    this.renderImageEditSurface();
    return true;
  }

  imageEditSourceSize() {
    if (!this.imageEditSourceImage || this.imageEditSourceImage.naturalWidth <= 0 || this.imageEditSourceImage.naturalHeight <= 0) return null;
    return {width: this.imageEditSourceImage.naturalWidth, height: this.imageEditSourceImage.naturalHeight};
  }

  imageEditFullSourceMode() {
    return this.imageEditActiveTool === 'crop';
  }

  renderImageEditContext() {
    const context = this.imageEditContext;
    if (!context) return;
    const eraserActive = this.imageEditActiveTool === 'draw' && this.imageEditDrawMode === 'eraser';
    this.imageEditSvg?.classList.toggle('is-eraser-active', eraserActive);
    if (!eraserActive) this.imageEditPointerSource = null;
    context.replaceChildren();
    const selectedOverlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditSelectedOverlayId) || null;
    context.hidden = this.imageEditActiveTool === 'select' && !selectedOverlay;
    const addButton = (label, action, {active = false, title = '', compact = false} = {}) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'prosemirror-image-edit-option';
      if (active) button.classList.add('is-active');
      if (compact) button.classList.add('is-compact');
      button.textContent = label;
      if (title) button.title = title;
      button.setAttribute('aria-pressed', active ? 'true' : 'false');
      button.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        action();
      });
      context.append(button);
      return button;
    };
    const divider = () => {
      const element = document.createElement('span');
      element.className = 'prosemirror-image-edit-divider';
      context.append(element);
    };
    const addColors = () => {
      divider();
      for (const key of ['red', 'blue', 'black', 'white', 'yellow', 'green']) {
        const button = addButton('', () => { this.setImageEditColor(key); }, {active: this.imageEditColor === key, compact: true});
        button.classList.add('prosemirror-image-edit-color');
        button.style.setProperty('--image-edit-color', IMAGE_EDIT_COLORS[key]);
        button.setAttribute('aria-label', `${key} colour`);
      }
    };
    const addThickness = () => {
      divider();
      for (const [key, label] of [['thin', 'S'], ['medium', 'M'], ['thick', 'L']]) {
        addButton(label, () => { this.setImageEditThickness(key); }, {active: this.imageEditThickness === key, compact: true});
      }
    };
    const addTextStyle = () => {
      for (const [key, label] of [['small', 'S'], ['medium', 'M'], ['large', 'L']]) {
        addButton(label, () => { this.imageEditTextSize = key; this.applyImageEditTextStyle(); this.renderImageEditContext(); }, {active: this.imageEditTextSize === key, compact: true});
      }
      addButton('B', () => { this.imageEditTextBold = !this.imageEditTextBold; this.applyImageEditTextStyle(); this.renderImageEditContext(); }, {active: this.imageEditTextBold});
      addButton('Background', () => { this.imageEditTextBackground = !this.imageEditTextBackground; this.applyImageEditTextStyle(); this.renderImageEditContext(); }, {active: this.imageEditTextBackground});
      addColors();
    };

    if (this.imageEditActiveTool === 'select' && selectedOverlay) {
      if (selectedOverlay.type === 'text') addTextStyle();
      else { addColors(); addThickness(); }
      divider();
      addButton('Delete', () => this.deleteSelectedImageEditOverlay());
      return;
    }
    if (this.imageEditActiveTool === 'crop') {
      for (const [key, label] of [['free', 'Free'], ['original', 'Original'], ['1:1', 'Square'], ['4:3', '4:3'], ['3:2', '3:2'], ['16:9', '16:9']]) {
        addButton(label, () => this.setImageEditCropAspect(key), {active: this.imageEditCropAspect === key});
      }
      divider();
      addButton('Reset', () => this.commitImageEditMutation(recipe => { recipe.crop = {x: 0, y: 0, width: 1, height: 1}; }));
      return;
    }
    if (this.imageEditActiveTool === 'rotate') {
      addButton('Rotate left', () => this.commitImageEditMutation(recipe => { recipe.rotation = (recipe.rotation + 270) % 360; }));
      addButton('Rotate right', () => this.commitImageEditMutation(recipe => { recipe.rotation = (recipe.rotation + 90) % 360; }));
      divider();
      addButton('Flip horizontal', () => this.commitImageEditMutation(recipe => { recipe.flipX = !recipe.flipX; }));
      addButton('Flip vertical', () => this.commitImageEditMutation(recipe => { recipe.flipY = !recipe.flipY; }));
      return;
    }
    if (this.imageEditActiveTool === 'draw') {
      for (const [key, label] of [['pen', 'Pen'], ['highlighter', 'Highlighter'], ['eraser', 'Eraser']]) {
        addButton(label, () => { this.imageEditDrawMode = key; this.renderImageEditContext(); this.scheduleImageEditOverlayRender(); }, {active: this.imageEditDrawMode === key});
      }
      if (this.imageEditDrawMode !== 'eraser') addColors();
      addThickness();
      return;
    }
    if (this.imageEditActiveTool === 'shape') {
      for (const [key, label] of [['rect', 'Rectangle'], ['ellipse', 'Ellipse'], ['arrow', 'Arrow'], ['line', 'Line']]) {
        addButton(label, () => { this.imageEditShapeType = key; this.renderImageEditContext(); }, {active: this.imageEditShapeType === key});
      }
      addColors(); addThickness();
      return;
    }
    if (this.imageEditActiveTool === 'text') addTextStyle();
  }

  syncImageEditControlsFromSelection(overlay) {
    if (!overlay) return;
    if (IMAGE_EDIT_COLORS[overlay.color]) this.imageEditColor = overlay.color;
    if (overlay.type === 'text') {
      const fontSize = Number(overlay.fontSize) || 0.038;
      this.imageEditTextSize = fontSize < 0.032 ? 'small' : (fontSize > 0.047 ? 'large' : 'medium');
      this.imageEditTextBold = overlay.bold === true;
      this.imageEditTextBackground = overlay.background === true;
      return;
    }
    let width = Number(overlay.width) || 0.005;
    if (overlay.type === 'path' && overlay.mode === 'highlighter') width /= 3;
    this.imageEditThickness = width < 0.004 ? 'thin' : (width > 0.007 ? 'thick' : 'medium');
  }

  setImageEditColor(color) {
    if (!IMAGE_EDIT_COLORS[color]) return false;
    this.imageEditColor = color;
    const selectedId = this.imageEditSelectedOverlayId;
    const selected = this.imageEditRecipe.overlays.find(item => item.id === selectedId);
    if (selected) this.commitImageEditMutation(recipe => {
      const overlay = recipe.overlays.find(item => item.id === selectedId);
      if (overlay) overlay.color = color;
    });
    this.renderImageEditContext();
    return true;
  }

  setImageEditThickness(thickness) {
    if (!['thin', 'medium', 'thick'].includes(thickness)) return false;
    this.imageEditThickness = thickness;
    const selectedId = this.imageEditSelectedOverlayId;
    const selected = this.imageEditRecipe.overlays.find(item => item.id === selectedId);
    if (selected && selected.type !== 'text') {
      const width = this.imageEditThicknessValue() * (selected.type === 'path' && selected.mode === 'highlighter' ? 3 : 1);
      this.commitImageEditMutation(recipe => {
        const overlay = recipe.overlays.find(item => item.id === selectedId);
        if (overlay && overlay.type !== 'text') overlay.width = width;
      });
    }
    this.renderImageEditContext();
    this.scheduleImageEditOverlayRender();
    return true;
  }

  imageEditThicknessValue() {
    return this.imageEditThickness === 'thin' ? 0.0025 : (this.imageEditThickness === 'thick' ? 0.009 : 0.005);
  }

  imageEditEraserRadius() {
    return this.imageEditThickness === 'thin' ? 0.012 : (this.imageEditThickness === 'thick' ? 0.04 : 0.024);
  }

  imageEditMetricPoint(point, size) {
    const minimum = Math.max(1, Math.min(size.width, size.height));
    return {x: point.x * size.width / minimum, y: point.y * size.height / minimum};
  }

  imageEditPointSegmentDistance(point, start, end) {
    const dx = end.x - start.x; const dy = end.y - start.y;
    const lengthSquared = dx * dx + dy * dy;
    if (lengthSquared <= 1e-12) return Math.hypot(point.x - start.x, point.y - start.y);
    const t = Math.max(0, Math.min(1, ((point.x - start.x) * dx + (point.y - start.y) * dy) / lengthSquared));
    return Math.hypot(point.x - (start.x + t * dx), point.y - (start.y + t * dy));
  }

  imageEditSegmentDistance(a, b, c, d) {
    const cross = (p, q, r) => (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    const abC = cross(a, b, c); const abD = cross(a, b, d);
    const cdA = cross(c, d, a); const cdB = cross(c, d, b);
    const epsilon = 1e-9;
    const between = (value, left, right) => value >= Math.min(left, right) - epsilon && value <= Math.max(left, right) + epsilon;
    const onSegment = (p, q, r) => Math.abs(cross(p, q, r)) <= epsilon && between(r.x, p.x, q.x) && between(r.y, p.y, q.y);
    const intersects = ((abC > epsilon && abD < -epsilon) || (abC < -epsilon && abD > epsilon))
      && ((cdA > epsilon && cdB < -epsilon) || (cdA < -epsilon && cdB > epsilon));
    if (intersects || onSegment(a, b, c) || onSegment(a, b, d) || onSegment(c, d, a) || onSegment(c, d, b)) return 0;
    return Math.min(
      this.imageEditPointSegmentDistance(a, c, d),
      this.imageEditPointSegmentDistance(b, c, d),
      this.imageEditPointSegmentDistance(c, a, b),
      this.imageEditPointSegmentDistance(d, a, b),
    );
  }

  imageEditSegmentHitsBounds(start, end, bounds, radius, size) {
    if (!bounds) return false;
    const left = bounds.x1; const right = bounds.x2;
    const top = bounds.y1; const bottom = bounds.y2;
    const inside = point => point.x >= left && point.x <= right && point.y >= top && point.y <= bottom;
    if (inside(start) || inside(end)) return true;
    const a = this.imageEditMetricPoint(start, size); const b = this.imageEditMetricPoint(end, size);
    const edges = [
      [{x: left, y: top}, {x: right, y: top}],
      [{x: right, y: top}, {x: right, y: bottom}],
      [{x: right, y: bottom}, {x: left, y: bottom}],
      [{x: left, y: bottom}, {x: left, y: top}],
    ];
    return edges.some(([p, q]) => this.imageEditSegmentDistance(a, b, this.imageEditMetricPoint(p, size), this.imageEditMetricPoint(q, size)) <= radius);
  }

  eraseImageEditSegment(start, end) {
    const size = this.imageEditSourceSize();
    if (!size) return false;
    const eraserRadius = this.imageEditEraserRadius();
    const a = this.imageEditMetricPoint(start, size); const b = this.imageEditMetricPoint(end, size);
    let changed = false;
    const next = [];
    for (const overlay of this.imageEditRecipe.overlays) {
      if (overlay.type === 'path' && Array.isArray(overlay.points) && overlay.points.length >= 2) {
        const threshold = eraserRadius + Math.max(0, Number(overlay.width) || 0) / 2;
        const pieces = []; let current = []; let pathChanged = false;
        for (let index = 1; index < overlay.points.length; index += 1) {
          const p0 = overlay.points[index - 1]; const p1 = overlay.points[index];
          const hit = this.imageEditSegmentDistance(a, b, this.imageEditMetricPoint(p0, size), this.imageEditMetricPoint(p1, size)) <= threshold;
          if (hit) {
            pathChanged = true;
            if (current.length >= 2) pieces.push(current);
            current = [];
          } else if (current.length === 0) {
            current = [p0, p1];
          } else {
            current.push(p1);
          }
        }
        if (current.length >= 2) pieces.push(current);
        if (pathChanged) {
          changed = true;
          pieces.forEach((points, index) => next.push({...overlay, id: index === 0 ? overlay.id : `overlay_${Date.now()}_${this.imageEditOverlaySequence++}`, points}));
        } else {
          next.push(overlay);
        }
        continue;
      }
      let hit = false;
      if (overlay.type === 'line' || overlay.type === 'arrow') {
        const p0 = this.imageEditMetricPoint({x: overlay.x1, y: overlay.y1}, size);
        const p1 = this.imageEditMetricPoint({x: overlay.x2, y: overlay.y2}, size);
        hit = this.imageEditSegmentDistance(a, b, p0, p1) <= eraserRadius + Math.max(0, Number(overlay.width) || 0) / 2;
      } else {
        hit = this.imageEditSegmentHitsBounds(start, end, imageEditOverlayBounds(overlay), eraserRadius, size);
      }
      if (hit) changed = true; else next.push(overlay);
    }
    if (changed) {
      this.imageEditRecipe.overlays = next;
      this.imageEditSelectedOverlayId = null;
    }
    return changed;
  }

  imageEditTextSizeValue() {
    return this.imageEditTextSize === 'small' ? 0.025 : (this.imageEditTextSize === 'large' ? 0.055 : 0.038);
  }

  commitImageEditMutation(mutator) {
    if (this.imageEditSaving) return false;
    const before = cloneImageEditRecipe(this.imageEditRecipe);
    mutator(this.imageEditRecipe);
    this.imageEditRecipe = sanitizeImageEditRecipe(this.imageEditRecipe);
    if (imageEditRecipeEqual(before, this.imageEditRecipe)) return false;
    this.imageEditUndoStack.push(before);
    if (this.imageEditUndoStack.length > 100) this.imageEditUndoStack.shift();
    this.imageEditRedoStack = [];
    this.renderImageEditSurface();
    this.updateImageEditControls();
    return true;
  }

  finishImageEditInteractionHistory(before) {
    if (!before || imageEditRecipeEqual(before, this.imageEditRecipe)) return;
    this.imageEditUndoStack.push(before);
    if (this.imageEditUndoStack.length > 100) this.imageEditUndoStack.shift();
    this.imageEditRedoStack = [];
    this.updateImageEditControls();
  }

  undoImageEdit() {
    if (this.imageEditSaving || this.imageEditUndoStack.length === 0) return false;
    this.finishImageEditTextEditing(true);
    this.imageEditRedoStack.push(cloneImageEditRecipe(this.imageEditRecipe));
    this.imageEditRecipe = this.imageEditUndoStack.pop();
    this.imageEditSelectedOverlayId = null;
    this.renderImageEditContext();
    this.renderImageEditSurface();
    this.updateImageEditControls();
    return true;
  }

  redoImageEdit() {
    if (this.imageEditSaving || this.imageEditRedoStack.length === 0) return false;
    this.finishImageEditTextEditing(true);
    this.imageEditUndoStack.push(cloneImageEditRecipe(this.imageEditRecipe));
    this.imageEditRecipe = this.imageEditRedoStack.pop();
    this.imageEditSelectedOverlayId = null;
    this.renderImageEditContext();
    this.renderImageEditSurface();
    this.updateImageEditControls();
    return true;
  }

  updateImageEditControls() {
    if (!this.imageEditToolbar) return;
    this.imageEditUndoButton.disabled = this.imageEditSaving || this.imageEditUndoStack.length === 0;
    this.imageEditRedoButton.disabled = this.imageEditSaving || this.imageEditRedoStack.length === 0;
    this.imageEditCancelButton.disabled = false;
    this.imageEditCancelButton.textContent = 'Cancel';
    this.imageEditDoneButton.disabled = this.imageEditSaving || !this.imageEditSourceReady;
    const stageLabels = {rendering: 'Rendering…', saving: 'Saving…', committing: 'Saving…', complete: 'Complete'};
    this.imageEditDoneButton.textContent = this.imageEditSaving ? (stageLabels[this.imageEditApplyStage] || 'Applying…') : 'Done';
    for (const button of this.imageEditToolbar.querySelectorAll('[data-image-edit-tool]')) button.disabled = this.imageEditSaving || !this.imageEditSourceReady;
  }

  drawImageEditBase(context, width, height, full = false, sourceImage = this.imageEditSourceImage) {
    const size = sourceImage && sourceImage.naturalWidth > 0 && sourceImage.naturalHeight > 0
      ? {width: sourceImage.naturalWidth, height: sourceImage.naturalHeight}
      : null;
    if (!size || !context) return;
    const crop = imageEditCrop(this.imageEditRecipe, full);
    const sx = crop.x * size.width;
    const sy = crop.y * size.height;
    const sw = crop.width * size.width;
    const sh = crop.height * size.height;
    const rotation = this.imageEditRecipe.rotation;
    const localWidth = rotation === 90 || rotation === 270 ? height : width;
    const localHeight = rotation === 90 || rotation === 270 ? width : height;
    context.save();
    if (rotation === 90) { context.translate(width, 0); context.rotate(Math.PI / 2); }
    else if (rotation === 180) { context.translate(width, height); context.rotate(Math.PI); }
    else if (rotation === 270) { context.translate(0, height); context.rotate(-Math.PI / 2); }
    if (this.imageEditRecipe.flipX) { context.translate(localWidth, 0); context.scale(-1, 1); }
    if (this.imageEditRecipe.flipY) { context.translate(0, localHeight); context.scale(1, -1); }
    context.drawImage(sourceImage, sx, sy, sw, sh, 0, 0, localWidth, localHeight);
    context.restore();
  }

  imageEditDisplayGeometry() {
    const size = this.imageEditSourceSize();
    if (!size || !this.imageEditStage) return null;
    const full = this.imageEditFullSourceMode();
    const natural = imageEditCropPixelSize(this.imageEditRecipe, size.width, size.height, full);
    const stageWidth = Math.max(1, this.imageEditStage.clientWidth - 32);
    const stageHeight = Math.max(1, this.imageEditStage.clientHeight - 32);
    const scale = Math.min(stageWidth / natural.width, stageHeight / natural.height);
    const width = Math.max(1, natural.width * scale);
    const height = Math.max(1, natural.height * scale);
    return {
      full, naturalWidth: natural.width, naturalHeight: natural.height,
      width, height,
      left: (this.imageEditStage.clientWidth - width) / 2,
      top: (this.imageEditStage.clientHeight - height) / 2,
    };
  }

  renderImageEditOverlayLayer() {
    if (!this.imageEditSurface || this.imageEditSurface.hidden || !this.imageEditSourceReady || !this.imageEditDisplayRect) return;
    this.renderImageEditOverlays(this.imageEditDisplayRect);
    this.positionImageEditTextEditor();
  }

  scheduleImageEditOverlayRender() {
    if (this.imageEditOverlayRenderFrame) return;
    this.imageEditOverlayRenderFrame = window.requestAnimationFrame(() => {
      this.imageEditOverlayRenderFrame = 0;
      this.renderImageEditOverlayLayer();
    });
  }

  renderImageEditSurface() {
    if (!this.imageEditSurface || this.imageEditSurface.hidden || !this.imageEditSourceReady) {
      this.updateImageEditControls();
      return;
    }
    const geometry = this.imageEditDisplayGeometry();
    const size = this.imageEditSourceSize();
    if (!geometry || !size) return;
    this.imageEditDisplayRect = geometry;
    const dpr = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
    let backingWidth = Math.max(1, Math.round(geometry.width * dpr));
    let backingHeight = Math.max(1, Math.round(geometry.height * dpr));
    const maxEdge = Math.max(backingWidth, backingHeight);
    const maxPixels = 8_000_000;
    let backingScale = maxEdge > 3072 ? 3072 / maxEdge : 1;
    if (backingWidth * backingHeight * backingScale * backingScale > maxPixels) {
      backingScale = Math.min(backingScale, Math.sqrt(maxPixels / (backingWidth * backingHeight)));
    }
    backingWidth = Math.max(1, Math.round(backingWidth * backingScale));
    backingHeight = Math.max(1, Math.round(backingHeight * backingScale));
    if (this.imageEditCanvas.width !== backingWidth) this.imageEditCanvas.width = backingWidth;
    if (this.imageEditCanvas.height !== backingHeight) this.imageEditCanvas.height = backingHeight;
    Object.assign(this.imageEditCanvas.style, {left: `${geometry.left}px`, top: `${geometry.top}px`, width: `${geometry.width}px`, height: `${geometry.height}px`});
    Object.assign(this.imageEditSvg.style, {left: `${geometry.left}px`, top: `${geometry.top}px`, width: `${geometry.width}px`, height: `${geometry.height}px`});
    this.imageEditSvg.setAttribute('viewBox', `0 0 ${geometry.naturalWidth} ${geometry.naturalHeight}`);
    const context = this.imageEditCanvas.getContext('2d');
    context.clearRect(0, 0, backingWidth, backingHeight);
    this.drawImageEditBase(context, backingWidth, backingHeight, geometry.full);
    this.renderImageEditOverlays(geometry);
    this.positionImageEditTextEditor();
    this.updateImageEditControls();
  }

  imageEditSvgElement(name, attrs = {}) {
    const element = document.createElementNS('http://www.w3.org/2000/svg', name);
    for (const [key, value] of Object.entries(attrs)) element.setAttribute(key, String(value));
    return element;
  }

  imageEditOutputPoint(sourcePoint, geometry) {
    const normalized = imageEditSourceToOutput(sourcePoint, this.imageEditRecipe, geometry.full);
    return {x: normalized.x * geometry.naturalWidth, y: normalized.y * geometry.naturalHeight};
  }

  imageEditSourceRectOutputBounds(rect, geometry) {
    const points = [
      {x: rect.x1, y: rect.y1}, {x: rect.x2, y: rect.y1},
      {x: rect.x2, y: rect.y2}, {x: rect.x1, y: rect.y2},
    ].map(point => this.imageEditOutputPoint(point, geometry));
    return {
      x1: Math.min(...points.map(point => point.x)),
      y1: Math.min(...points.map(point => point.y)),
      x2: Math.max(...points.map(point => point.x)),
      y2: Math.max(...points.map(point => point.y)),
    };
  }

  renderImageEditOverlays(geometry) {
    const svg = this.imageEditSvg;
    svg.replaceChildren();
    const size = this.imageEditSourceSize();
    if (!size) return;
    const sourceMin = Math.min(size.width, size.height);
    for (const overlay of this.imageEditRecipe.overlays) {
      const color = IMAGE_EDIT_COLORS[overlay.color] || IMAGE_EDIT_COLORS.red;
      const strokeWidth = Math.max(1, overlay.width * sourceMin);
      let element = null;
      if (overlay.type === 'path') {
        const points = overlay.points.map(point => this.imageEditOutputPoint(point, geometry));
        const d = points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ');
        element = this.imageEditSvgElement('path', {d, fill: 'none', stroke: color, 'stroke-width': strokeWidth, 'stroke-linecap': 'round', 'stroke-linejoin': 'round', opacity: overlay.opacity || 1});
      } else if (overlay.type === 'line' || overlay.type === 'arrow') {
        const start = this.imageEditOutputPoint({x: overlay.x1, y: overlay.y1}, geometry);
        const end = this.imageEditOutputPoint({x: overlay.x2, y: overlay.y2}, geometry);
        element = this.imageEditSvgElement('line', {x1: start.x, y1: start.y, x2: end.x, y2: end.y, stroke: color, 'stroke-width': strokeWidth, 'stroke-linecap': 'round'});
        if (overlay.type === 'arrow') {
          const angle = Math.atan2(end.y - start.y, end.x - start.x);
          const head = Math.max(strokeWidth * 4, Math.min(geometry.naturalWidth, geometry.naturalHeight) * 0.025);
          const p1 = {x: end.x - Math.cos(angle - Math.PI / 6) * head, y: end.y - Math.sin(angle - Math.PI / 6) * head};
          const p2 = {x: end.x - Math.cos(angle + Math.PI / 6) * head, y: end.y - Math.sin(angle + Math.PI / 6) * head};
          const arrowHead = this.imageEditSvgElement('path', {d: `M ${p1.x} ${p1.y} L ${end.x} ${end.y} L ${p2.x} ${p2.y}`, fill: 'none', stroke: color, 'stroke-width': strokeWidth, 'stroke-linecap': 'round', 'stroke-linejoin': 'round'});
          arrowHead.dataset.imageEditOverlayId = overlay.id;
          svg.append(arrowHead);
        }
      } else if (overlay.type === 'rect' || overlay.type === 'ellipse') {
        const bounds = this.imageEditSourceRectOutputBounds(imageEditOverlayBounds(overlay), geometry);
        if (overlay.type === 'rect') element = this.imageEditSvgElement('rect', {x: bounds.x1, y: bounds.y1, width: Math.max(1, bounds.x2 - bounds.x1), height: Math.max(1, bounds.y2 - bounds.y1), fill: 'none', stroke: color, 'stroke-width': strokeWidth});
        else element = this.imageEditSvgElement('ellipse', {cx: (bounds.x1 + bounds.x2) / 2, cy: (bounds.y1 + bounds.y2) / 2, rx: Math.max(0.5, (bounds.x2 - bounds.x1) / 2), ry: Math.max(0.5, (bounds.y2 - bounds.y1) / 2), fill: 'none', stroke: color, 'stroke-width': strokeWidth});
      } else if (overlay.type === 'text') {
        if (overlay.id === this.imageEditTextEditingId) continue;
        const bounds = this.imageEditSourceRectOutputBounds(imageEditOverlayBounds(overlay), geometry);
        const group = this.imageEditSvgElement('g');
        if (overlay.background) group.append(this.imageEditSvgElement('rect', {x: bounds.x1, y: bounds.y1, width: Math.max(1, bounds.x2 - bounds.x1), height: Math.max(1, bounds.y2 - bounds.y1), rx: 4, fill: 'rgba(255,255,255,0.82)'}));
        const text = this.imageEditSvgElement('text', {x: bounds.x1 + 4, y: bounds.y1 + Math.max(12, overlay.fontSize * sourceMin), fill: color, 'font-size': Math.max(8, overlay.fontSize * sourceMin), 'font-family': '-apple-system, BlinkMacSystemFont, sans-serif', 'font-weight': overlay.bold ? '700' : '400'});
        const lines = (clampImageEditText(overlay.text) || 'Text').split('\n');
        lines.forEach((line, index) => {
          const span = this.imageEditSvgElement('tspan', {x: bounds.x1 + 4, dy: index === 0 ? 0 : '1.2em'});
          span.textContent = line || ' ';
          text.append(span);
        });
        group.append(text);
        element = group;
      }
      if (element) {
        element.dataset.imageEditOverlayId = overlay.id;
        element.classList.add('prosemirror-image-edit-object');
        if (overlay.id === this.imageEditSelectedOverlayId) element.classList.add('is-selected');
        svg.append(element);
      }
    }

    if (this.imageEditActiveTool === 'crop') this.renderImageEditCropOverlay(geometry);
    if (this.imageEditSelectedOverlayId) this.renderImageEditSelection(geometry);
    this.renderImageEditEraserCursor(geometry);
  }

  renderImageEditCropOverlay(geometry) {
    const crop = this.imageEditRecipe.crop;
    const bounds = this.imageEditSourceRectOutputBounds({x1: crop.x, y1: crop.y, x2: crop.x + crop.width, y2: crop.y + crop.height}, geometry);
    const w = geometry.naturalWidth; const h = geometry.naturalHeight;
    const dim = 'rgba(0,0,0,0.42)';
    const rects = [
      [0, 0, w, Math.max(0, bounds.y1)],
      [0, bounds.y2, w, Math.max(0, h - bounds.y2)],
      [0, bounds.y1, Math.max(0, bounds.x1), Math.max(0, bounds.y2 - bounds.y1)],
      [bounds.x2, bounds.y1, Math.max(0, w - bounds.x2), Math.max(0, bounds.y2 - bounds.y1)],
    ];
    for (const [x, y, width, height] of rects) this.imageEditSvg.append(this.imageEditSvgElement('rect', {x, y, width, height, fill: dim, 'pointer-events': 'none'}));
    const cropRect = this.imageEditSvgElement('rect', {x: bounds.x1, y: bounds.y1, width: Math.max(1, bounds.x2 - bounds.x1), height: Math.max(1, bounds.y2 - bounds.y1), fill: 'transparent', stroke: '#ffffff', 'stroke-width': 1.5, 'vector-effect': 'non-scaling-stroke'});
    cropRect.dataset.imageEditCropMove = 'true';
    cropRect.classList.add('prosemirror-image-edit-crop-rect');
    this.imageEditSvg.append(cropRect);
    const handleRadius = Math.max(4, 6 * geometry.naturalWidth / Math.max(1, geometry.width));
    const handles = {
      nw: [crop.x, crop.y], ne: [crop.x + crop.width, crop.y],
      se: [crop.x + crop.width, crop.y + crop.height], sw: [crop.x, crop.y + crop.height],
      n: [crop.x + crop.width / 2, crop.y], e: [crop.x + crop.width, crop.y + crop.height / 2],
      s: [crop.x + crop.width / 2, crop.y + crop.height], w: [crop.x, crop.y + crop.height / 2],
    };
    for (const [name, [x, y]] of Object.entries(handles)) {
      const point = this.imageEditOutputPoint({x, y}, geometry);
      const handle = this.imageEditSvgElement('circle', {cx: point.x, cy: point.y, r: handleRadius, fill: '#ffffff', stroke: '#111111', 'stroke-width': 1, 'vector-effect': 'non-scaling-stroke'});
      handle.dataset.imageEditCropHandle = name;
      handle.classList.add('prosemirror-image-edit-handle');
      this.imageEditSvg.append(handle);
    }
  }

  renderImageEditSelection(geometry) {
    const overlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditSelectedOverlayId);
    const bounds = imageEditOverlayBounds(overlay);
    if (!overlay || !bounds) return;
    const output = this.imageEditSourceRectOutputBounds(bounds, geometry);
    const box = this.imageEditSvgElement('rect', {x: output.x1, y: output.y1, width: Math.max(1, output.x2 - output.x1), height: Math.max(1, output.y2 - output.y1), fill: 'none', stroke: '#ffffff', 'stroke-width': 1, 'stroke-dasharray': '5 4', 'vector-effect': 'non-scaling-stroke', 'pointer-events': 'none'});
    this.imageEditSvg.append(box);
    if (overlay.type === 'path') return;
    const handleRadius = Math.max(4, 6 * geometry.naturalWidth / Math.max(1, geometry.width));
    if (overlay.type === 'line' || overlay.type === 'arrow') {
      for (const [name, source] of [['start', {x: overlay.x1, y: overlay.y1}], ['end', {x: overlay.x2, y: overlay.y2}]]) {
        const point = this.imageEditOutputPoint(source, geometry);
        const handle = this.imageEditSvgElement('circle', {cx: point.x, cy: point.y, r: handleRadius, fill: '#ffffff', stroke: '#111111', 'stroke-width': 1, 'vector-effect': 'non-scaling-stroke'});
        handle.dataset.imageEditLineEnd = name;
        handle.classList.add('prosemirror-image-edit-handle');
        this.imageEditSvg.append(handle);
      }
      return;
    }
    const corners = {nw: {x: bounds.x1, y: bounds.y1}, ne: {x: bounds.x2, y: bounds.y1}, se: {x: bounds.x2, y: bounds.y2}, sw: {x: bounds.x1, y: bounds.y2}};
    for (const [name, source] of Object.entries(corners)) {
      const point = this.imageEditOutputPoint(source, geometry);
      const handle = this.imageEditSvgElement('circle', {cx: point.x, cy: point.y, r: handleRadius, fill: '#ffffff', stroke: '#111111', 'stroke-width': 1, 'vector-effect': 'non-scaling-stroke'});
      handle.dataset.imageEditResizeHandle = name;
      handle.classList.add('prosemirror-image-edit-handle');
      this.imageEditSvg.append(handle);
    }
  }

  imageEditClientToSource(event, full = this.imageEditFullSourceMode()) {
    const rect = this.imageEditSvg.getBoundingClientRect();
    const output = {x: clampImageEditUnit((event.clientX - rect.left) / Math.max(1, rect.width)), y: clampImageEditUnit((event.clientY - rect.top) / Math.max(1, rect.height))};
    return imageEditOutputToSource(output, this.imageEditRecipe, full);
  }

  imageEditEraserCursorGeometry(geometry) {
    if (!geometry || !this.imageEditPointerSource || !this.imageEditSourceReady) return null;
    if (this.imageEditActiveTool !== 'draw' || this.imageEditDrawMode !== 'eraser' || !this.imageEditPointerInside) return null;
    const size = this.imageEditSourceSize();
    if (!size) return null;
    const centerSource = this.imageEditPointerSource;
    const sourceMin = Math.max(1, Math.min(size.width, size.height));
    const radius = this.imageEditEraserRadius();
    const dx = radius * sourceMin / Math.max(1, size.width);
    const dy = radius * sourceMin / Math.max(1, size.height);
    const center = this.imageEditOutputPoint(centerSource, geometry);
    const xProbeSource = {
      x: clampImageEditUnit(centerSource.x + (centerSource.x + dx <= 1 ? dx : -dx)),
      y: centerSource.y,
    };
    const yProbeSource = {
      x: centerSource.x,
      y: clampImageEditUnit(centerSource.y + (centerSource.y + dy <= 1 ? dy : -dy)),
    };
    const xProbe = this.imageEditOutputPoint(xProbeSource, geometry);
    const yProbe = this.imageEditOutputPoint(yProbeSource, geometry);
    const rx = Math.max(1, Math.hypot(xProbe.x - center.x, xProbe.y - center.y));
    const ry = Math.max(1, Math.hypot(yProbe.x - center.x, yProbe.y - center.y));
    return {center, rx, ry};
  }

  renderImageEditEraserCursor(geometry) {
    const cursor = this.imageEditEraserCursorGeometry(geometry);
    if (!cursor) return;
    const group = this.imageEditSvgElement('g', {'pointer-events': 'none'});
    group.classList.add('prosemirror-image-edit-eraser-cursor');
    group.append(
      this.imageEditSvgElement('ellipse', {
        cx: cursor.center.x,
        cy: cursor.center.y,
        rx: cursor.rx,
        ry: cursor.ry,
        fill: 'rgba(255,255,255,0.08)',
        stroke: 'rgba(0,0,0,0.88)',
        'stroke-width': 3,
        'vector-effect': 'non-scaling-stroke',
      }),
      this.imageEditSvgElement('ellipse', {
        cx: cursor.center.x,
        cy: cursor.center.y,
        rx: cursor.rx,
        ry: cursor.ry,
        fill: 'none',
        stroke: '#ffffff',
        'stroke-width': 1.25,
        'vector-effect': 'non-scaling-stroke',
      }),
    );
    this.imageEditSvg.append(group);
  }

  handleImageEditPointerHover(event) {
    if (!this.imageEditSourceReady || this.imageEditSaving) return;
    this.imageEditPointerInside = true;
    if (this.imageEditActiveTool === 'draw' && this.imageEditDrawMode === 'eraser') {
      this.imageEditPointerSource = this.imageEditClientToSource(event);
      this.scheduleImageEditOverlayRender();
    }
  }

  handleImageEditPointerLeave(event) {
    if (this.imageEditInteraction && this.imageEditInteraction.pointerId === event.pointerId) return;
    this.imageEditPointerInside = false;
    this.imageEditPointerSource = null;
    this.scheduleImageEditOverlayRender();
  }

  handleImageEditPointerDown(event) {
    if (this.imageEditSaving || !this.imageEditSourceReady || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const target = event.target instanceof Element ? event.target : null;
    const overlayTarget = target?.closest?.('[data-image-edit-overlay-id]') || null;
    const source = this.imageEditClientToSource(event);
    this.imageEditPointerInside = true;
    this.imageEditPointerSource = source;
    const before = cloneImageEditRecipe(this.imageEditRecipe);
    const selectedId = overlayTarget?.dataset.imageEditOverlayId || null;
    if (this.imageEditActiveTool === 'crop') {
      const handle = target?.dataset.imageEditCropHandle || null;
      const move = target?.dataset.imageEditCropMove === 'true';
      this.imageEditInteraction = {kind: handle ? 'crop-resize' : (move ? 'crop-move' : 'crop-new'), handle, start: source, originalCrop: {...this.imageEditRecipe.crop}, before};
    } else if (this.imageEditActiveTool === 'draw' && this.imageEditDrawMode === 'eraser') {
      const changed = this.eraseImageEditSegment(source, source);
      this.imageEditInteraction = {kind: 'erase', last: source, before, changed};
      if (changed) this.scheduleImageEditOverlayRender();
    } else if (target?.dataset.imageEditLineEnd && this.imageEditSelectedOverlayId) {
      this.imageEditInteraction = {kind: 'line-resize', end: target.dataset.imageEditLineEnd, before};
    } else if (target?.dataset.imageEditResizeHandle && this.imageEditSelectedOverlayId) {
      this.imageEditInteraction = {kind: 'overlay-resize', handle: target.dataset.imageEditResizeHandle, before};
    } else if (selectedId) {
      this.imageEditSelectedOverlayId = selectedId;
      this.syncImageEditControlsFromSelection(this.imageEditRecipe.overlays.find(item => item.id === selectedId));
      this.imageEditInteraction = {kind: 'overlay-move', start: source, last: source, before};
      this.renderImageEditContext();
      this.renderImageEditSurface();
    } else if (this.imageEditActiveTool === 'draw' && this.imageEditDrawMode !== 'eraser') {
      const overlay = {id: `overlay_${Date.now()}_${this.imageEditOverlaySequence++}`, type: 'path', mode: this.imageEditDrawMode, color: this.imageEditColor, width: this.imageEditThicknessValue() * (this.imageEditDrawMode === 'highlighter' ? 3 : 1), opacity: this.imageEditDrawMode === 'highlighter' ? 0.32 : 1, points: [source, source]};
      this.imageEditRecipe.overlays.push(overlay);
      this.imageEditSelectedOverlayId = overlay.id;
      this.imageEditInteraction = {kind: 'draw', id: overlay.id, last: source, before};
      this.renderImageEditSurface();
    } else if (this.imageEditActiveTool === 'shape') {
      const overlay = {id: `overlay_${Date.now()}_${this.imageEditOverlaySequence++}`, type: this.imageEditShapeType, color: this.imageEditColor, width: this.imageEditThicknessValue(), x1: source.x, y1: source.y, x2: source.x, y2: source.y};
      this.imageEditRecipe.overlays.push(overlay);
      this.imageEditSelectedOverlayId = overlay.id;
      this.imageEditInteraction = {kind: 'shape-create', id: overlay.id, before};
      this.renderImageEditSurface();
    } else if (this.imageEditActiveTool === 'text') {
      const overlay = {id: `overlay_${Date.now()}_${this.imageEditOverlaySequence++}`, type: 'text', color: this.imageEditColor, width: 0.002, x: source.x, y: source.y, boxWidth: 0.28, boxHeight: 0.12, text: 'Text', fontSize: this.imageEditTextSizeValue(), bold: this.imageEditTextBold, background: this.imageEditTextBackground};
      this.imageEditRecipe.overlays.push(overlay);
      this.imageEditSelectedOverlayId = overlay.id;
      this.finishImageEditInteractionHistory(before);
      this.renderImageEditSurface();
      this.openImageEditTextEditor(overlay, true);
      return;
    } else {
      this.imageEditSelectedOverlayId = null;
      this.renderImageEditContext();
      this.renderImageEditSurface();
      return;
    }
    try { this.imageEditSvg.setPointerCapture?.(event.pointerId); } catch { /* capture unavailable */ }
    this.imageEditInteraction.pointerId = event.pointerId;
  }

  handleImageEditPointerMove(event) {
    if (!this.imageEditSourceReady || this.imageEditSaving) return;
    const source = this.imageEditClientToSource(event);
    this.imageEditPointerInside = true;
    if (this.imageEditActiveTool === 'draw' && this.imageEditDrawMode === 'eraser') {
      this.imageEditPointerSource = source;
      this.scheduleImageEditOverlayRender();
    }
    const interaction = this.imageEditInteraction;
    if (!interaction || interaction.pointerId !== event.pointerId) return;
    event.preventDefault();
    if (interaction.kind === 'erase') {
      if (this.eraseImageEditSegment(interaction.last, source)) interaction.changed = true;
      interaction.last = source;
    } else if (interaction.kind === 'draw') {
      const overlay = this.imageEditRecipe.overlays.find(item => item.id === interaction.id);
      if (!overlay) return;
      const last = overlay.points[overlay.points.length - 1];
      if (Math.hypot(source.x - last.x, source.y - last.y) > 0.0015) overlay.points.push(source);
    } else if (interaction.kind === 'shape-create') {
      const overlay = this.imageEditRecipe.overlays.find(item => item.id === interaction.id);
      if (overlay) { overlay.x2 = source.x; overlay.y2 = source.y; }
    } else if (interaction.kind === 'overlay-move') {
      const overlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditSelectedOverlayId);
      if (overlay) {
        imageEditMoveOverlay(overlay, source.x - interaction.last.x, source.y - interaction.last.y);
        interaction.last = source;
      }
    } else if (interaction.kind === 'line-resize') {
      const overlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditSelectedOverlayId);
      if (overlay) {
        if (interaction.end === 'start') { overlay.x1 = source.x; overlay.y1 = source.y; }
        else { overlay.x2 = source.x; overlay.y2 = source.y; }
      }
    } else if (interaction.kind === 'overlay-resize') {
      this.resizeImageEditSelectedOverlay(interaction.handle, source);
    } else if (interaction.kind === 'crop-move') {
      const crop = interaction.originalCrop;
      let dx = source.x - interaction.start.x; let dy = source.y - interaction.start.y;
      dx = Math.max(-crop.x, Math.min(1 - crop.x - crop.width, dx));
      dy = Math.max(-crop.y, Math.min(1 - crop.y - crop.height, dy));
      this.imageEditRecipe.crop = {...crop, x: crop.x + dx, y: crop.y + dy};
    } else if (interaction.kind === 'crop-resize') {
      this.resizeImageEditCrop(interaction, source);
    } else if (interaction.kind === 'crop-new') {
      this.imageEditRecipe.crop = imageEditNormalizedRect(interaction.start, source);
    }
    this.scheduleImageEditOverlayRender();
  }

  handleImageEditPointerEnd(event) {
    const interaction = this.imageEditInteraction;
    if (!interaction || interaction.pointerId !== event.pointerId) return;
    event.preventDefault();
    if (interaction.kind.startsWith('crop') && this.imageEditCropAspect !== 'free') this.constrainImageEditCropToAspect();
    if (interaction.kind === 'shape-create') {
      const overlay = this.imageEditRecipe.overlays.find(item => item.id === interaction.id);
      if (overlay && Math.hypot(overlay.x2 - overlay.x1, overlay.y2 - overlay.y1) < 0.003) {
        this.imageEditRecipe.overlays = this.imageEditRecipe.overlays.filter(item => item.id !== interaction.id);
        this.imageEditSelectedOverlayId = null;
      }
    }
    this.imageEditRecipe = sanitizeImageEditRecipe(this.imageEditRecipe);
    this.finishImageEditInteractionHistory(interaction.before);
    this.imageEditInteraction = null;
    if (this.imageEditOverlayRenderFrame) {
      window.cancelAnimationFrame(this.imageEditOverlayRenderFrame);
      this.imageEditOverlayRenderFrame = 0;
    }
    try { this.imageEditSvg.releasePointerCapture?.(event.pointerId); } catch { /* capture already released */ }
    this.renderImageEditSurface();
  }

  handleImageEditDoubleClick(event) {
    const target = event.target instanceof Element ? event.target : null;
    const overlayTarget = target?.closest?.('[data-image-edit-overlay-id]') || null;
    const id = overlayTarget?.dataset.imageEditOverlayId;
    if (!id) return;
    const overlay = this.imageEditRecipe.overlays.find(item => item.id === id);
    if (overlay?.type !== 'text') return;
    event.preventDefault();
    event.stopPropagation();
    this.imageEditSelectedOverlayId = id;
    this.openImageEditTextEditor(overlay, false);
  }

  resizeImageEditSelectedOverlay(handle, source) {
    const overlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditSelectedOverlayId);
    const bounds = imageEditOverlayBounds(overlay);
    if (!overlay || !bounds) return;
    let left = bounds.x1; let right = bounds.x2; let top = bounds.y1; let bottom = bounds.y2;
    if (handle.includes('w')) left = source.x;
    if (handle.includes('e')) right = source.x;
    if (handle.includes('n')) top = source.y;
    if (handle.includes('s')) bottom = source.y;
    if (left > right) [left, right] = [right, left];
    if (top > bottom) [top, bottom] = [bottom, top];
    if (overlay.type === 'text') {
      overlay.x = left; overlay.y = top;
      overlay.boxWidth = Math.max(0.03, right - left);
      overlay.boxHeight = Math.max(0.02, bottom - top);
    } else if (overlay.type === 'rect' || overlay.type === 'ellipse') {
      overlay.x1 = left; overlay.y1 = top; overlay.x2 = right; overlay.y2 = bottom;
    }
  }

  resizeImageEditCrop(interaction, source) {
    const crop = interaction.originalCrop;
    let left = crop.x; let right = crop.x + crop.width; let top = crop.y; let bottom = crop.y + crop.height;
    const handle = interaction.handle || '';
    if (handle.includes('w')) left = source.x;
    if (handle.includes('e')) right = source.x;
    if (handle.includes('n')) top = source.y;
    if (handle.includes('s')) bottom = source.y;
    if (left > right) [left, right] = [right, left];
    if (top > bottom) [top, bottom] = [bottom, top];
    this.imageEditRecipe.crop = {x: clampImageEditUnit(left), y: clampImageEditUnit(top), width: Math.max(0.005, Math.min(1 - left, right - left)), height: Math.max(0.005, Math.min(1 - top, bottom - top))};
  }

  setImageEditCropAspect(aspect) {
    this.imageEditCropAspect = aspect;
    if (aspect !== 'free') this.commitImageEditMutation(() => this.constrainImageEditCropToAspect());
    this.renderImageEditContext();
  }

  imageEditCropAspectRatio() {
    if (this.imageEditCropAspect === 'free') return null;
    const size = this.imageEditSourceSize();
    if (!size) return null;
    let finalRatio;
    if (this.imageEditCropAspect === 'original') finalRatio = (this.imageEditRecipe.rotation === 90 || this.imageEditRecipe.rotation === 270) ? size.height / size.width : size.width / size.height;
    else if (this.imageEditCropAspect === '1:1') finalRatio = 1;
    else if (this.imageEditCropAspect === '4:3') finalRatio = 4 / 3;
    else if (this.imageEditCropAspect === '3:2') finalRatio = 3 / 2;
    else if (this.imageEditCropAspect === '16:9') finalRatio = 16 / 9;
    else return null;
    const sourcePixelRatio = (this.imageEditRecipe.rotation === 90 || this.imageEditRecipe.rotation === 270) ? 1 / finalRatio : finalRatio;
    return sourcePixelRatio * size.height / size.width;
  }

  constrainImageEditCropToAspect() {
    const normalizedRatio = this.imageEditCropAspectRatio();
    if (!normalizedRatio || !Number.isFinite(normalizedRatio)) return;
    const crop = this.imageEditRecipe.crop;
    const cx = crop.x + crop.width / 2; const cy = crop.y + crop.height / 2;
    let width = crop.width; let height = width / normalizedRatio;
    if (height > crop.height) { height = crop.height; width = height * normalizedRatio; }
    const maxWidth = 2 * Math.min(cx, 1 - cx); const maxHeight = 2 * Math.min(cy, 1 - cy);
    if (width > maxWidth) { width = maxWidth; height = width / normalizedRatio; }
    if (height > maxHeight) { height = maxHeight; width = height * normalizedRatio; }
    this.imageEditRecipe.crop = {x: clampImageEditUnit(cx - width / 2), y: clampImageEditUnit(cy - height / 2), width: Math.max(0.005, width), height: Math.max(0.005, height)};
  }

  deleteSelectedImageEditOverlay() {
    const id = this.imageEditSelectedOverlayId;
    if (!id) return false;
    const changed = this.commitImageEditMutation(recipe => { recipe.overlays = recipe.overlays.filter(item => item.id !== id); });
    this.imageEditSelectedOverlayId = null;
    this.renderImageEditContext();
    this.renderImageEditSurface();
    return changed;
  }

  applyImageEditTextStyle() {
    const overlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditSelectedOverlayId && item.type === 'text');
    if (!overlay) return;
    const before = cloneImageEditRecipe(this.imageEditRecipe);
    overlay.fontSize = this.imageEditTextSizeValue();
    overlay.bold = this.imageEditTextBold;
    overlay.background = this.imageEditTextBackground;
    overlay.color = this.imageEditColor;
    this.finishImageEditInteractionHistory(before);
    this.renderImageEditSurface();
  }

  openImageEditTextEditor(overlay, selectAll = false) {
    if (!overlay || overlay.type !== 'text') return;
    this.finishImageEditTextEditing(true);
    this.imageEditSelectedOverlayId = overlay.id;
    this.imageEditTextEditingId = overlay.id;
    this.imageEditTextBefore = cloneImageEditRecipe(this.imageEditRecipe);
    this.imageEditTextEditor.value = clampImageEditText(overlay.text);
    this.imageEditTextEditor.hidden = false;
    this.positionImageEditTextEditor();
    window.requestAnimationFrame(() => {
      this.imageEditTextEditor?.focus({preventScroll: true});
      if (selectAll) this.imageEditTextEditor?.select();
    });
  }

  positionImageEditTextEditor() {
    if (!this.imageEditTextEditingId || !this.imageEditDisplayRect || this.imageEditTextEditor.hidden) return;
    const overlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditTextEditingId);
    if (!overlay) return;
    const geometry = this.imageEditDisplayRect;
    const bounds = this.imageEditSourceRectOutputBounds(imageEditOverlayBounds(overlay), geometry);
    const svgRect = this.imageEditSvg?.getBoundingClientRect();
    if (!svgRect || svgRect.width <= 0 || svgRect.height <= 0) return;
    let left = svgRect.left + (bounds.x1 / geometry.naturalWidth) * svgRect.width;
    let top = svgRect.top + (bounds.y1 / geometry.naturalHeight) * svgRect.height;
    let width = Math.max(80, ((bounds.x2 - bounds.x1) / geometry.naturalWidth) * svgRect.width);
    let height = Math.max(40, ((bounds.y2 - bounds.y1) / geometry.naturalHeight) * svgRect.height);
    width = Math.min(width, Math.max(40, svgRect.right - left));
    height = Math.min(height, Math.max(34, svgRect.bottom - top));
    if (left + width > svgRect.right) left = Math.max(svgRect.left, svgRect.right - width);
    if (top + height > svgRect.bottom) top = Math.max(svgRect.top, svgRect.bottom - height);
    left = Math.max(svgRect.left, left);
    top = Math.max(svgRect.top, top);
    const sourceSize = this.imageEditSourceSize();
    const sourceMin = sourceSize ? Math.max(1, Math.min(sourceSize.width, sourceSize.height)) : 1;
    const naturalFontSize = Math.max(8, overlay.fontSize * sourceMin);
    const displayScale = Math.min(svgRect.width / Math.max(1, geometry.naturalWidth), svgRect.height / Math.max(1, geometry.naturalHeight));
    const fontSize = Math.max(11, Math.min(96, naturalFontSize * displayScale));
    Object.assign(this.imageEditTextEditor.style, {
      left: `${left}px`, top: `${top}px`, width: `${width}px`, height: `${height}px`,
      color: IMAGE_EDIT_COLORS[overlay.color] || IMAGE_EDIT_COLORS.black,
      fontWeight: overlay.bold ? '700' : '400', fontSize: `${fontSize}px`, lineHeight: '1.2',
      background: overlay.background ? 'rgba(255,255,255,0.92)' : 'rgba(255,255,255,0.12)',
    });
  }

  handleImageEditTextInput() {
    const overlay = this.imageEditRecipe.overlays.find(item => item.id === this.imageEditTextEditingId && item.type === 'text');
    if (!overlay) return;
    const cursor = this.imageEditTextEditor.selectionStart;
    const text = clampImageEditText(this.imageEditTextEditor.value);
    if (this.imageEditTextEditor.value !== text) {
      this.imageEditTextEditor.value = text;
      const nextCursor = Math.min(Number.isFinite(cursor) ? cursor : text.length, text.length);
      this.imageEditTextEditor.setSelectionRange(nextCursor, nextCursor);
    }
    overlay.text = text;
    this.scheduleImageEditOverlayRender();
  }

  finishImageEditTextEditing(commit) {
    const id = this.imageEditTextEditingId;
    if (!id) return;
    const before = this.imageEditTextBefore;
    const overlay = this.imageEditRecipe.overlays.find(item => item.id === id && item.type === 'text');
    if (!commit && before) this.imageEditRecipe = before;
    else if (overlay && overlay.text.trim() === '') this.imageEditRecipe.overlays = this.imageEditRecipe.overlays.filter(item => item.id !== id);
    if (commit && before) this.finishImageEditInteractionHistory(before);
    this.imageEditTextEditingId = null;
    this.imageEditTextBefore = null;
    if (this.imageEditTextEditor) this.imageEditTextEditor.hidden = true;
    this.renderImageEditSurface();
  }

  handleImageEditKeyDown(event) {
    if (!this.imageEditSurface || this.imageEditSurface.hidden || event.defaultPrevented) return;
    if (event.target instanceof HTMLTextAreaElement && this.imageEditTextEditingId && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'z') return;
    if ((event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === 'z') {
      event.preventDefault(); event.stopPropagation();
      if (event.shiftKey) this.redoImageEdit(); else this.undoImageEdit();
      return;
    }
    if (event.key === 'Escape') {
      if (this.imageEditTextEditingId) return;
      event.preventDefault(); event.stopPropagation();
      this.controller?.exitImageEditFocus(this, true, false);
      return;
    }
    if ((event.key === 'Delete' || event.key === 'Backspace') && this.imageEditSelectedOverlayId && !(event.target instanceof HTMLTextAreaElement)) {
      event.preventDefault(); event.stopPropagation(); this.deleteSelectedImageEditOverlay(); return;
    }
    if (event.key === 'Tab') {
      const focusable = [...this.imageEditSurface.querySelectorAll('button:not([disabled]), textarea:not([hidden]), [tabindex="0"]')].filter(element => element instanceof HTMLElement && element.offsetParent !== null);
      if (focusable.length === 0) return;
      const index = focusable.indexOf(document.activeElement);
      const next = event.shiftKey ? (index <= 0 ? focusable.length - 1 : index - 1) : (index < 0 || index >= focusable.length - 1 ? 0 : index + 1);
      event.preventDefault(); focusable[next].focus({preventScroll: true});
    }
  }

  handleImageEditViewportChange() {
    if (!this.imageEditSurface || this.imageEditSurface.hidden) return;
    this.renderImageEditSurface();
  }

  releaseImageEditWorkingRasterForCommit() {
    if (this.imageEditOverlayRenderFrame) {
      window.cancelAnimationFrame(this.imageEditOverlayRenderFrame);
      this.imageEditOverlayRenderFrame = 0;
    }
    if (this.imageEditCanvas) {
      this.imageEditCanvas.width = 1;
      this.imageEditCanvas.height = 1;
    }
    if (this.imageEditSourceImage) this.imageEditSourceImage.removeAttribute('src');
    this.imageEditSourceReady = false;
  }

  restoreImageEditWorkingRasterAfterFailure() {
    const index = this.imageEditWorkingCandidateIndex >= 0 ? this.imageEditWorkingCandidateIndex : 0;
    this.setImageEditWorkingCandidate(index);
  }

  terminateImageEditRenderWorker() {
    const cancel = this.imageEditRenderWorkerCancel;
    this.imageEditRenderWorkerCancel = null;
    if (this.imageEditRenderWorker) {
      try { this.imageEditRenderWorker.terminate(); } catch { /* worker already stopped */ }
      this.imageEditRenderWorker = null;
    }
    if (this.imageEditRenderWorkerUrl) {
      try { URL.revokeObjectURL(this.imageEditRenderWorkerUrl); } catch { /* already revoked */ }
      this.imageEditRenderWorkerUrl = '';
    }
    if (cancel) cancel();
  }

  setImageEditApplyStage(stage) {
    const next = ['idle', 'rendering', 'saving', 'committing', 'complete'].includes(stage) ? stage : 'idle';
    this.imageEditApplyStage = next;
    if (this.imageEditStage) {
      const labels = {rendering: 'Rendering image…', saving: 'Saving image…', committing: 'Finishing image edit…', complete: 'Complete'};
      if (next === 'idle') {
        this.imageEditStage.classList.remove('is-applying');
        delete this.imageEditStage.dataset.applyLabel;
      } else {
        this.imageEditStage.classList.add('is-applying');
        this.imageEditStage.dataset.applyLabel = labels[next] || 'Applying image edit…';
      }
    }
    this.updateImageEditControls();
  }

  abortImageEditStageRequest() {
    if (!this.imageEditStageAbortController) return;
    try { this.imageEditStageAbortController.abort(); } catch { /* already aborted */ }
    this.imageEditStageAbortController = null;
  }

  async discardStagedImageEdit() {
    const token = this.imageEditStagedToken;
    this.imageEditStagedToken = '';
    if (!token) return false;
    try {
      await fetch(`/rustnoteattachment/image-edit-stage/${encodeURIComponent(token)}`, {
        method: 'DELETE',
        cache: 'no-store',
      });
    } catch {
      // Process-local staging is also cleaned at startup if cancellation races shutdown.
    }
    return true;
  }

  async stageImageEditBinary(buffer, mimeType) {
    if (!(buffer instanceof ArrayBuffer) || buffer.byteLength <= 0) throw new Error('The rendered image is empty.');
    if (!['image/png', 'image/jpeg'].includes(mimeType)) throw new Error('The rendered image format is unsupported.');
    const attachmentId = String(this.node.attrs.attachmentId || '');
    if (!attachmentId) throw new Error('The image attachment is unavailable.');
    this.abortImageEditStageRequest();
    const abortController = typeof AbortController === 'function' ? new AbortController() : null;
    this.imageEditStageAbortController = abortController;
    let response;
    try {
      response = await fetch(`/rustnoteattachment/image-edit-stage/${encodeURIComponent(attachmentId)}`, {
        method: 'POST',
        headers: {'Content-Type': mimeType},
        body: buffer,
        cache: 'no-store',
        signal: abortController?.signal,
      });
    } finally {
      if (this.imageEditStageAbortController === abortController) this.imageEditStageAbortController = null;
    }
    if (!response.ok) {
      let message = `Could not stage the edited image (${response.status}).`;
      try {
        const failure = await response.json();
        if (failure && typeof failure.error === 'string' && failure.error.trim()) message = failure.error.trim();
      } catch { /* keep status-based message */ }
      throw new Error(message);
    }
    let detail;
    try { detail = await response.json(); } catch { throw new Error('Image staging returned an invalid response.'); }
    const token = String(detail?.token || '');
    if (!/^[0-9a-fA-F]{32}$/.test(token)) throw new Error('Image staging returned an invalid token.');
    this.imageEditStagedToken = token;
    return token;
  }

  cancelImageEditApply({restore = true} = {}) {
    if (!this.imageEditSaving) return false;
    this.imageEditSaving = false;
    this.terminateImageEditRenderWorker();
    this.abortImageEditStageRequest();
    this.clearImageEditCommitTimeout();
    this.controller?.cancelPendingImageEditForView?.(this);
    void this.discardStagedImageEdit();
    this.setImageEditApplyStage('idle');
    if (restore && this.imageEditSurface && !this.imageEditSurface.hidden) this.restoreImageEditWorkingRasterAfterFailure();
    this.updateImageEditControls();
    return true;
  }

  armImageEditCommitTimeout() {
    if (this.imageEditCommitTimer) window.clearTimeout(this.imageEditCommitTimer);
    this.imageEditCommitTimer = window.setTimeout(() => {
      this.imageEditCommitTimer = 0;
      if (!this.imageEditSaving) return;
      this.imageEditSaving = false;
      this.terminateImageEditRenderWorker();
      this.abortImageEditStageRequest();
      this.controller?.cancelPendingImageEditForView?.(this);
      void this.discardStagedImageEdit();
      this.setImageEditApplyStage('idle');
      this.restoreImageEditWorkingRasterAfterFailure();
      this.updateImageEditControls();
      attachmentStatus('Image editing timed out while saving. No document change was applied; retry or cancel.');
    }, 30000);
  }

  clearImageEditCommitTimeout() {
    if (!this.imageEditCommitTimer) return;
    window.clearTimeout(this.imageEditCommitTimer);
    this.imageEditCommitTimer = 0;
  }

  imageEditWorkerSupported() {
    return typeof Worker === 'function'
      && typeof Blob === 'function'
      && typeof URL !== 'undefined'
      && typeof URL.createObjectURL === 'function'
      && typeof OffscreenCanvas === 'function'
      && typeof createImageBitmap === 'function';
  }

  buildImageEditRenderPlan(sourceImage, outputScale = 1) {
    const size = sourceImage && sourceImage.naturalWidth > 0 && sourceImage.naturalHeight > 0
      ? {width: sourceImage.naturalWidth, height: sourceImage.naturalHeight}
      : null;
    if (!size) throw new Error('The source image is unavailable.');
    const naturalOutput = this.validateImageEditOutputSize(size.width, size.height);
    const scale = Math.max(0.000001, Math.min(1, Number(outputScale) || 1));
    const width = Math.max(1, Math.round(naturalOutput.width * scale));
    const height = Math.max(1, Math.round(naturalOutput.height * scale));
    const crop = this.imageEditRecipe.crop;
    const rotation = this.imageEditRecipe.rotation;
    const localWidth = rotation === 90 || rotation === 270 ? height : width;
    const localHeight = rotation === 90 || rotation === 270 ? width : height;
    const geometry = {full: false, naturalWidth: width, naturalHeight: height};
    const sourceMin = Math.min(size.width, size.height);
    const overlays = [];

    for (const overlay of this.imageEditRecipe.overlays) {
      const color = IMAGE_EDIT_COLORS[overlay.color] || IMAGE_EDIT_COLORS.red;
      const strokeWidth = Math.max(1, overlay.width * sourceMin * scale);
      if (overlay.type === 'path') {
        overlays.push({
          type: 'path', color, strokeWidth, opacity: overlay.opacity || 1,
          points: overlay.points.map(point => this.imageEditOutputPoint(point, geometry)),
        });
      } else if (overlay.type === 'line' || overlay.type === 'arrow') {
        const start = this.imageEditOutputPoint({x: overlay.x1, y: overlay.y1}, geometry);
        const end = this.imageEditOutputPoint({x: overlay.x2, y: overlay.y2}, geometry);
        let arrowHead = null;
        if (overlay.type === 'arrow') {
          const angle = Math.atan2(end.y - start.y, end.x - start.x);
          const head = Math.max(strokeWidth * 4, Math.min(width, height) * 0.025);
          arrowHead = {
            p1: {x: end.x - Math.cos(angle - Math.PI / 6) * head, y: end.y - Math.sin(angle - Math.PI / 6) * head},
            p2: {x: end.x - Math.cos(angle + Math.PI / 6) * head, y: end.y - Math.sin(angle + Math.PI / 6) * head},
          };
        }
        overlays.push({type: overlay.type, color, strokeWidth, start, end, arrowHead});
      } else if (overlay.type === 'rect' || overlay.type === 'ellipse') {
        overlays.push({
          type: overlay.type, color, strokeWidth,
          bounds: this.imageEditSourceRectOutputBounds(imageEditOverlayBounds(overlay), geometry),
        });
      } else if (overlay.type === 'text') {
        overlays.push({
          type: 'text', color, strokeWidth,
          bounds: this.imageEditSourceRectOutputBounds(imageEditOverlayBounds(overlay), geometry),
          fontSize: Math.max(8, overlay.fontSize * sourceMin * scale),
          bold: overlay.bold === true,
          background: overlay.background === true,
          lines: clampImageEditText(overlay.text).split('\n'),
        });
      }
    }

    return {
      width,
      height,
      base: {
        sx: crop.x * size.width,
        sy: crop.y * size.height,
        sw: crop.width * size.width,
        sh: crop.height * size.height,
        rotation,
        flipX: this.imageEditRecipe.flipX === true,
        flipY: this.imageEditRecipe.flipY === true,
        localWidth,
        localHeight,
      },
      overlays,
    };
  }

  imageEditRenderWorkerSource() {
    return `
'use strict';
const drawRenderPlan = ${imageEditDrawRenderPlan.toString()};
const MAX_PIXELS = ${IMAGE_EDIT_MAX_OUTPUT_PIXELS};
const MAX_EDGE = ${IMAGE_EDIT_MAX_OUTPUT_EDGE};
self.onmessage = async event => {
  const bitmap = event.data && event.data.bitmap;
  const plan = event.data && event.data.plan;
  try {
    if (!bitmap || !plan || typeof OffscreenCanvas !== 'function') throw new Error('Background image renderer is unavailable.');
    const pixels = plan.width * plan.height;
    if (plan.width <= 0 || plan.height <= 0 || plan.width > MAX_EDGE || plan.height > MAX_EDGE || pixels > MAX_PIXELS) {
      throw new Error('This image is too large to render safely at full resolution. Reduce the crop area or image dimensions and retry.');
    }
    const canvas = new OffscreenCanvas(plan.width, plan.height);
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Could not create the background image renderer.');
    drawRenderPlan(context, bitmap, plan);
    if (typeof bitmap.close === 'function') bitmap.close();
    const outputType = event.data && event.data.mimeType === 'image/jpeg' ? 'image/jpeg' : 'image/png';
    const quality = outputType === 'image/jpeg' ? 0.94 : undefined;
    const blob = await canvas.convertToBlob({type: outputType, quality});
    const buffer = await blob.arrayBuffer();
    self.postMessage({ok: true, buffer, mimeType: outputType}, [buffer]);
  } catch (error) {
    try { if (bitmap && typeof bitmap.close === 'function') bitmap.close(); } catch (_) {}
    self.postMessage({ok: false, error: error instanceof Error ? error.message : String(error || 'Could not render the edited image.')});
  }
};`;
  }

  async renderImageEditInWorker(sourceImage, plan) {
    if (!this.imageEditWorkerSupported()) return null;
    let bitmap;
    try {
      bitmap = await createImageBitmap(sourceImage);
    } catch {
      return null;
    }
    this.terminateImageEditRenderWorker();
    const workerBlob = new Blob([this.imageEditRenderWorkerSource()], {type: 'text/javascript'});
    const workerUrl = URL.createObjectURL(workerBlob);
    let worker;
    try {
      worker = new Worker(workerUrl);
    } catch (error) {
      URL.revokeObjectURL(workerUrl);
      try { bitmap.close?.(); } catch { /* bitmap already closed */ }
      throw error;
    }
    this.imageEditRenderWorker = worker;
    this.imageEditRenderWorkerUrl = workerUrl;
    return new Promise((resolve, reject) => {
      let settled = false;
      const finish = (result, error) => {
        if (settled) return;
        settled = true;
        this.imageEditRenderWorkerCancel = null;
        this.terminateImageEditRenderWorker();
        if (error) reject(error); else resolve(result);
      };
      this.imageEditRenderWorkerCancel = () => {
        if (settled) return;
        settled = true;
        reject(new Error('Image rendering was cancelled.'));
      };
      worker.onmessage = event => {
        const message = event.data || {};
        if (message.ok === true && message.buffer instanceof ArrayBuffer && message.buffer.byteLength > 0) {
          finish({buffer: message.buffer, mimeType: String(message.mimeType || 'image/png')}, null);
        } else {
          finish(null, new Error(String(message.error || 'Could not render the edited image in the background.')));
        }
      };
      worker.onerror = event => finish(null, new Error(String(event?.message || 'The background image renderer failed.')));
      try {
        const format = this.imageEditOutputFormat();
        worker.postMessage({bitmap, plan, mimeType: format.mimeType}, [bitmap]);
      } catch (error) {
        try { bitmap.close?.(); } catch { /* bitmap already detached */ }
        finish(null, error instanceof Error ? error : new Error('Could not start the background image renderer.'));
      }
    });
  }

  async renderImageEditFallbackBinary() {
    const sourceImage = this.imageEditSourceImage;
    const size = sourceImage && sourceImage.naturalWidth > 0 && sourceImage.naturalHeight > 0
      ? {width: sourceImage.naturalWidth, height: sourceImage.naturalHeight}
      : null;
    if (!size) throw new Error('The working image is unavailable.');
    const naturalOutput = this.validateImageEditOutputSize(size.width, size.height);
    const naturalPixels = naturalOutput.width * naturalOutput.height;
    const scale = Math.min(
      1,
      IMAGE_EDIT_FALLBACK_MAX_EDGE / Math.max(naturalOutput.width, naturalOutput.height),
      Math.sqrt(IMAGE_EDIT_FALLBACK_MAX_PIXELS / Math.max(1, naturalPixels)),
    );
    const plan = this.buildImageEditRenderPlan(sourceImage, scale);
    const canvas = document.createElement('canvas');
    canvas.width = plan.width; canvas.height = plan.height;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Could not create the safe image renderer.');
    imageEditDrawRenderPlan(context, sourceImage, plan);
    const format = this.imageEditOutputFormat();
    const blob = await new Promise((resolve, reject) => {
      canvas.toBlob(value => value ? resolve(value) : reject(new Error('Could not encode the edited image.')), format.mimeType, format.mimeType === 'image/jpeg' ? 0.94 : undefined);
    });
    canvas.width = 1; canvas.height = 1;
    const buffer = await blob.arrayBuffer();
    return {buffer, mimeType: format.mimeType};
  }

  imageEditOutputFormat() {
    const sourceFormat = String(this.imageEditSourceRuntime?.imageFormat || '').toLowerCase();
    const hasAlpha = this.imageEditSourceRuntime?.imageHasAlpha === true;
    if (!hasAlpha && ['jpeg', 'heic', 'heif'].includes(sourceFormat)) {
      return {mimeType: 'image/jpeg', extension: 'jpg'};
    }
    return {mimeType: 'image/png', extension: 'png'};
  }

  imageEditCommitPayload(stageToken, mimeType) {
    const sourceName = String(this.imageEditSourceRuntime?.fileName || this.node.attrs.displayName || 'image');
    const stem = sourceName.replace(/\.[^.]+$/, '').trim() || 'image';
    const resolvedMimeType = mimeType === 'image/jpeg' ? 'image/jpeg' : 'image/png';
    const extension = resolvedMimeType === 'image/jpeg' ? 'jpg' : 'png';
    return {
      attachment_id: this.node.attrs.attachmentId,
      source_attachment_id: this.imageEditSourceAttachmentId,
      file_name: `${stem} edited.${extension}`,
      mime_type: resolvedMimeType,
      stage_token: stageToken,
      edit_recipe: JSON.stringify(this.imageEditRecipe),
    };
  }

  async commitImageEdit() {
    if (this.imageEditSaving || !this.imageEditSourceReady || !this.controller?.editable) return false;
    this.finishImageEditTextEditing(true);
    this.imageEditSaving = true;
    this.setImageEditApplyStage('rendering');
    this.armImageEditCommitTimeout();
    try {
      await new Promise(resolve => window.requestAnimationFrame(() => window.requestAnimationFrame(resolve)));
      const rendered = await this.renderImageEditOutputBinary();
      if (!this.imageEditSaving) return false;
      this.releaseImageEditWorkingRasterForCommit();
      this.setImageEditApplyStage('saving');
      const stageToken = await this.stageImageEditBinary(rendered.buffer, rendered.mimeType);
      if (!this.imageEditSaving) {
        void this.discardStagedImageEdit();
        return false;
      }
      this.setImageEditApplyStage('committing');
      const payload = this.imageEditCommitPayload(stageToken, rendered.mimeType);
      if (!this.controller.requestImageEditCommit(this, payload)) {
        void this.discardStagedImageEdit();
        throw new Error('Could not send the staged image edit to Rustnote storage.');
      }
      return true;
    } catch (error) {
      if (!this.imageEditSaving) return false;
      this.imageEditSaving = false;
      this.clearImageEditCommitTimeout();
      this.terminateImageEditRenderWorker();
      this.abortImageEditStageRequest();
      void this.discardStagedImageEdit();
      this.setImageEditApplyStage('idle');
      this.restoreImageEditWorkingRasterAfterFailure();
      this.updateImageEditControls();
      attachmentStatus(error instanceof Error ? error.message : 'Could not render or save the edited image.');
      return false;
    }
  }

  async loadFullResolutionImageForEdit() {
    const candidates = [this.imageEditPrimaryPreviewUrl, this.imageEditCompatiblePreviewUrl]
      .filter(Boolean)
      .filter((url, index, values) => values.indexOf(url) === index);
    for (const url of candidates) {
      try {
        const image = await new Promise((resolve, reject) => {
          const candidate = new Image();
          candidate.draggable = false;
          const timer = window.setTimeout(() => reject(new Error('Image source timed out while decoding.')), 10000);
          candidate.onload = () => { window.clearTimeout(timer); resolve(candidate); };
          candidate.onerror = () => { window.clearTimeout(timer); reject(new Error('Image source could not be decoded.')); };
          candidate.src = url;
        });
        if (image.naturalWidth > 0 && image.naturalHeight > 0) return image;
      } catch {
        // Try the full-resolution compatible rendition when the canonical source
        // is not directly decodable by this WebView.
      }
    }
    throw new Error('The full-resolution source image could not be loaded.');
  }

  validateImageEditOutputSize(width, height) {
    const output = imageEditCropPixelSize(this.imageEditRecipe, width, height, false);
    const pixels = output.width * output.height;
    if (output.width > IMAGE_EDIT_MAX_OUTPUT_EDGE || output.height > IMAGE_EDIT_MAX_OUTPUT_EDGE || pixels > IMAGE_EDIT_MAX_OUTPUT_PIXELS) {
      throw new Error('This image is too large to render safely at full resolution. Reduce the crop area or image dimensions and retry.');
    }
    return output;
  }

  preflightImageEditOutputSize() {
    const width = Number(this.imageEditSourceRuntime?.imagePixelWidth) || 0;
    const height = Number(this.imageEditSourceRuntime?.imagePixelHeight) || 0;
    if (width <= 0 || height <= 0) return null;
    return this.validateImageEditOutputSize(width, height);
  }

  async renderImageEditOutputBinary() {
    this.preflightImageEditOutputSize();
    if (this.imageEditWorkerSupported()) {
      const sourceImage = await this.loadFullResolutionImageForEdit();
      try {
        const plan = this.buildImageEditRenderPlan(sourceImage, 1);
        const rendered = await this.renderImageEditInWorker(sourceImage, plan);
        if (rendered?.buffer instanceof ArrayBuffer && rendered.buffer.byteLength > 0) return rendered;
      } catch (error) {
        attachmentStatus(`Background full-resolution rendering failed; using the safe working rendition instead. ${error instanceof Error ? error.message : ''}`.trim());
      } finally {
        sourceImage.removeAttribute('src');
      }
    }
    attachmentStatus('Full-resolution background rendering is unavailable in this WebView; saving the high-quality working rendition.');
    return this.renderImageEditFallbackBinary();
  }

  completeImageEditCommit(descriptor) {
    this.imageEditSaving = false;
    this.clearImageEditCommitTimeout();
    this.abortImageEditStageRequest();
    this.imageEditStagedToken = '';
    this.setImageEditApplyStage('complete');
    const attrs = this.controller?.registerAttachmentDescriptor(descriptor);
    if (!attrs) { this.setImageEditApplyStage('idle'); this.restoreImageEditWorkingRasterAfterFailure(); this.updateImageEditControls(); return false; }
    const position = this.safePosition();
    if (position === null) { this.setImageEditApplyStage('idle'); this.restoreImageEditWorkingRasterAfterFailure(); this.updateImageEditControls(); return false; }
    const current = this.view.state.doc.nodeAt(position);
    if (!current || current.type !== schema.nodes.attachment || current.attrs.kind !== 'image') { this.setImageEditApplyStage('idle'); this.restoreImageEditWorkingRasterAfterFailure(); this.updateImageEditControls(); return false; }
    const replacementAttrs = {...current.attrs, ...attrs, caption: current.attrs.caption, altText: current.attrs.altText, alignment: current.attrs.alignment, widthPercent: current.attrs.widthPercent, sizeMode: current.attrs.sizeMode};
    const applied = this.controller.runDiscreteUserAction(() => {
      let transaction = this.view.state.tr.setNodeMarkup(position, undefined, replacementAttrs);
      transaction = transaction.setSelection(NodeSelection.create(transaction.doc, position));
      this.view.dispatch(transaction.scrollIntoView());
      return true;
    });
    if (applied) this.controller.exitImageEditFocus(this, true, true);
    else { this.setImageEditApplyStage('idle'); this.restoreImageEditWorkingRasterAfterFailure(); this.updateImageEditControls(); }
    return applied;
  }

  failImageEditCommit(message) {
    this.imageEditSaving = false;
    this.clearImageEditCommitTimeout();
    this.abortImageEditStageRequest();
    void this.discardStagedImageEdit();
    this.setImageEditApplyStage('idle');
    this.restoreImageEditWorkingRasterAfterFailure();
    this.updateImageEditControls();
    attachmentStatus(String(message || 'Could not apply image edit.'));
    return true;
  }

  resetToOriginal() {
    if (!this.controller?.editable) return false;
    const currentRuntime = this.controller.attachmentRegistry.get(this.node.attrs.attachmentId);
    const sourceId = currentRuntime?.sourceAttachmentId;
    if (!sourceId) return false;
    const source = this.controller.attachmentRegistry.get(sourceId);
    if (!source || source.available !== true) return false;
    const descriptor = {
      id: sourceId,
      file_name: source.fileName || 'image',
      mime_type: source.mimeType || '',
      byte_size: source.byteSize || 0,
      available: true,
      preview_url: source.previewUrl || '',
      compatible_preview_url: source.compatiblePreviewUrl || '',
      screen_preview_url: source.screenPreviewUrl || '',
      image_pixel_width: source.imagePixelWidth,
      image_pixel_height: source.imagePixelHeight,
      image_format: source.imageFormat || '',
      image_orientation: source.imageOrientation,
      image_has_alpha: source.imageHasAlpha,
      image_animated: source.imageAnimated,
    };
    const attrs = this.controller.registerAttachmentDescriptor(descriptor);
    const position = this.safePosition();
    if (!attrs || position === null) return false;
    const current = this.view.state.doc.nodeAt(position);
    if (!current || current.type !== schema.nodes.attachment || current.attrs.kind !== 'image') return false;
    const replacementAttrs = {...current.attrs, ...attrs, caption: current.attrs.caption, altText: current.attrs.altText, alignment: current.attrs.alignment, widthPercent: current.attrs.widthPercent, sizeMode: current.attrs.sizeMode};
    return this.controller.runDiscreteUserAction(() => {
      let transaction = this.view.state.tr.setNodeMarkup(position, undefined, replacementAttrs);
      transaction = transaction.setSelection(NodeSelection.create(transaction.doc, position));
      this.view.dispatch(transaction.scrollIntoView());
      return true;
    });
  }

  imageFocusRuntime() {
    return this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
  }

  imageFocusNaturalSize() {
    const runtimeSize = imageRuntimePixelSize(this.imageFocusRuntime());
    if (runtimeSize) return runtimeSize;
    if (this.imageFocusImage && this.imageFocusImage.naturalWidth > 0 && this.imageFocusImage.naturalHeight > 0) {
      return {width: this.imageFocusImage.naturalWidth, height: this.imageFocusImage.naturalHeight};
    }
    return null;
  }

  imageFocusDesiredResolution(scale = this.imageFocusScale, mode = this.imageFocusMode) {
    const runtime = this.imageFocusRuntime();
    const screenUrl = runtime && typeof runtime.screenPreviewUrl === 'string' ? runtime.screenPreviewUrl : '';
    if (!screenUrl) return 'full';
    if (mode === 'actual') return 'full';
    const size = this.imageFocusNaturalSize();
    const resolvedScale = Number.isFinite(scale) ? scale : this.imageFocusFitScale();
    if (!size || !Number.isFinite(resolvedScale)) return 'screen';
    const displayedLongEdge = Math.max(size.width, size.height) * resolvedScale;
    const threshold = this.imageFocusResolution === 'full'
      ? IMAGE_SCREEN_RENDITION_LONG_EDGE * 0.8
      : IMAGE_SCREEN_RENDITION_LONG_EDGE;
    return displayedLongEdge > threshold ? 'full' : 'screen';
  }

  setImageFocusCandidate(index) {
    const candidates = this.imageFocusCandidates || [];
    if (index < 0 || index >= candidates.length) {
      this.imageFocusCandidateIndex = -1;
      this.imageFocusPreviewUrl = '';
      this.imageFocusImage.removeAttribute('src');
      return false;
    }
    const url = candidates[index];
    this.imageFocusCandidateIndex = index;
    this.imageFocusPreviewUrl = url;
    this.imageFocusImage.hidden = true;
    this.imageFocusMissing.hidden = false;
    this.imageFocusMissing.textContent = url === this.imageFocusScreenPreviewUrl
      ? 'Preparing image…'
      : (url === this.imageFocusCompatiblePreviewUrl ? 'Preparing compatible preview…' : 'Loading image…');
    if (this.imageFocusImage.src !== url) {
      this.imageFocusImage.src = url;
    } else if (this.imageFocusImage.complete && this.imageFocusImage.naturalWidth > 0) {
      this.imageFocusImage.hidden = false;
      this.imageFocusMissing.hidden = true;
      if (!Number.isFinite(this.imageFocusScale)) this.imageFocusScale = this.imageFocusFitScale();
      this.applyImageFocusTransform();
    }
    return true;
  }

  advanceImageFocusCandidate() {
    return this.setImageFocusCandidate(this.imageFocusCandidateIndex + 1);
  }

  loadImageFocusResolution(force = false) {
    const runtime = this.imageFocusRuntime();
    if (!runtime || runtime.available !== true) return false;
    const desired = this.imageFocusDesiredResolution();
    const candidates = desired === 'screen'
      ? [this.imageFocusScreenPreviewUrl, this.imageFocusPrimaryPreviewUrl, this.imageFocusCompatiblePreviewUrl]
      : [this.imageFocusPrimaryPreviewUrl, this.imageFocusCompatiblePreviewUrl];
    const unique = candidates.filter(Boolean).filter((url, index, values) => values.indexOf(url) === index);
    const sameCandidates = unique.length === this.imageFocusCandidates.length
      && unique.every((url, index) => url === this.imageFocusCandidates[index]);
    if (!force && desired === this.imageFocusResolution && sameCandidates && this.imageFocusCandidateIndex >= 0) return false;
    this.imageFocusResolution = desired;
    this.imageFocusCandidates = unique;
    this.imageFocusCandidateIndex = -1;
    if (unique.length === 0) {
      this.imageFocusImage.removeAttribute('src');
      this.imageFocusImage.hidden = true;
      this.imageFocusMissing.hidden = false;
      this.imageFocusMissing.textContent = 'Image preview unavailable';
      return false;
    }
    return this.setImageFocusCandidate(0);
  }

  imageFocusFitScale() {
    const size = this.imageFocusNaturalSize();
    if (!size || !this.imageFocusStage) return 1;
    const width = Math.max(1, this.imageFocusStage.clientWidth);
    const height = Math.max(1, this.imageFocusStage.clientHeight);
    return Math.max(0.05, Math.min(1, width / size.width, height / size.height));
  }

  clampImageFocusPan() {
    const size = this.imageFocusNaturalSize();
    if (!size || !this.imageFocusStage || !Number.isFinite(this.imageFocusScale)) {
      this.imageFocusPanX = 0;
      this.imageFocusPanY = 0;
      return;
    }
    const scaledWidth = size.width * this.imageFocusScale;
    const scaledHeight = size.height * this.imageFocusScale;
    const maxX = Math.max(0, (scaledWidth - this.imageFocusStage.clientWidth) / 2);
    const maxY = Math.max(0, (scaledHeight - this.imageFocusStage.clientHeight) / 2);
    this.imageFocusPanX = Math.max(-maxX, Math.min(maxX, Number(this.imageFocusPanX) || 0));
    this.imageFocusPanY = Math.max(-maxY, Math.min(maxY, Number(this.imageFocusPanY) || 0));
  }

  applyImageFocusTransform() {
    if (!this.imageFocusImage || this.imageFocusImage.hidden) return;
    const size = this.imageFocusNaturalSize();
    if (!size) return;
    if (!Number.isFinite(this.imageFocusScale)) this.imageFocusScale = this.imageFocusFitScale();
    this.clampImageFocusPan();
    this.imageFocusImage.style.width = `${Math.max(1, size.width * this.imageFocusScale)}px`;
    this.imageFocusImage.style.height = `${Math.max(1, size.height * this.imageFocusScale)}px`;
    this.imageFocusImage.style.left = `calc(50% + ${this.imageFocusPanX}px)`;
    this.imageFocusImage.style.top = `calc(50% + ${this.imageFocusPanY}px)`;
    if (this.imageFocusStage) this.imageFocusStage.classList.toggle('is-pannable', this.canPanImageFocus());
    this.updateImageFocusControls();
  }

  setImageFocusScale(scale, {mode = 'custom', clientX = null, clientY = null} = {}) {
    if (!this.imageFocusStage) return false;
    const previous = Number.isFinite(this.imageFocusScale) ? this.imageFocusScale : this.imageFocusFitScale();
    const next = Math.max(0.05, Math.min(16, Number(scale) || previous));
    if (Number.isFinite(clientX) && Number.isFinite(clientY) && previous > 0) {
      const rect = this.imageFocusStage.getBoundingClientRect();
      const offsetX = clientX - (rect.left + rect.width / 2);
      const offsetY = clientY - (rect.top + rect.height / 2);
      const ratio = next / previous;
      this.imageFocusPanX = offsetX - (offsetX - this.imageFocusPanX) * ratio;
      this.imageFocusPanY = offsetY - (offsetY - this.imageFocusPanY) * ratio;
    }
    this.imageFocusMode = mode;
    this.imageFocusScale = next;
    const resolutionChanged = this.loadImageFocusResolution();
    if (!resolutionChanged) this.applyImageFocusTransform();
    this.noteImageFocusActivity();
    return true;
  }

  setImageFocusFit() {
    this.imageFocusPanX = 0;
    this.imageFocusPanY = 0;
    return this.setImageFocusScale(this.imageFocusFitScale(), {mode: 'fit'});
  }

  setImageFocusActual() {
    this.imageFocusPanX = 0;
    this.imageFocusPanY = 0;
    return this.setImageFocusScale(1, {mode: 'actual'});
  }

  zoomImageFocusBy(factor, clientX = null, clientY = null) {
    const base = Number.isFinite(this.imageFocusScale) ? this.imageFocusScale : this.imageFocusFitScale();
    return this.setImageFocusScale(base * factor, {mode: 'custom', clientX, clientY});
  }

  toggleImageFocusFitActual() {
    if (this.imageFocusMode === 'fit') return this.setImageFocusActual();
    return this.setImageFocusFit();
  }

  canPanImageFocus() {
    const size = this.imageFocusNaturalSize();
    if (!size || !this.imageFocusStage || !Number.isFinite(this.imageFocusScale)) return false;
    return size.width * this.imageFocusScale > this.imageFocusStage.clientWidth + 1
      || size.height * this.imageFocusScale > this.imageFocusStage.clientHeight + 1;
  }

  panImageFocusBy(deltaX, deltaY) {
    if (!this.canPanImageFocus()) return false;
    this.imageFocusPanX += Number(deltaX) || 0;
    this.imageFocusPanY += Number(deltaY) || 0;
    this.applyImageFocusTransform();
    this.noteImageFocusActivity();
    return true;
  }

  handleImageFocusWheel(event) {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden) return;
    this.noteImageFocusActivity();
    if (this.imageFocusGestureStartScale !== null && (event.ctrlKey || event.metaKey)) return;
    if (event.ctrlKey || event.metaKey) {
      event.preventDefault();
      const factor = Math.exp(-Math.max(-80, Math.min(80, event.deltaY)) * 0.01);
      this.zoomImageFocusBy(factor, event.clientX, event.clientY);
      return;
    }
    if (this.canPanImageFocus()) {
      event.preventDefault();
      this.panImageFocusBy(-event.deltaX, -event.deltaY);
    }
  }

  handleImageFocusPointerDown(event) {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden || event.button > 0) return;
    this.noteImageFocusActivity();
    this.imageFocusPointers.set(event.pointerId, {x: event.clientX, y: event.clientY});
    if (this.imageFocusStage && typeof this.imageFocusStage.setPointerCapture === 'function') this.imageFocusStage.setPointerCapture(event.pointerId);
    if (this.imageFocusPointers.size >= 2) {
      const points = [...this.imageFocusPointers.values()].slice(0, 2);
      this.imageFocusPinch = {
        distance: Math.max(1, Math.hypot(points[1].x - points[0].x, points[1].y - points[0].y)),
        scale: Number.isFinite(this.imageFocusScale) ? this.imageFocusScale : this.imageFocusFitScale(),
      };
      this.imageFocusDragPointerId = null;
      this.imageFocusDragOrigin = null;
      event.preventDefault();
      return;
    }
    if (this.canPanImageFocus()) {
      this.imageFocusDragPointerId = event.pointerId;
      this.imageFocusDragOrigin = {x: event.clientX, y: event.clientY, panX: this.imageFocusPanX, panY: this.imageFocusPanY};
      if (this.imageFocusStage) this.imageFocusStage.classList.add('is-panning');
      event.preventDefault();
    }
  }

  handleImageFocusPointerMove(event) {
    if (!this.imageFocusPointers.has(event.pointerId)) return;
    this.imageFocusPointers.set(event.pointerId, {x: event.clientX, y: event.clientY});
    if (this.imageFocusPointers.size >= 2 && this.imageFocusPinch) {
      const points = [...this.imageFocusPointers.values()].slice(0, 2);
      const distance = Math.max(1, Math.hypot(points[1].x - points[0].x, points[1].y - points[0].y));
      const centerX = (points[0].x + points[1].x) / 2;
      const centerY = (points[0].y + points[1].y) / 2;
      this.setImageFocusScale(this.imageFocusPinch.scale * (distance / this.imageFocusPinch.distance), {
        mode: 'custom', clientX: centerX, clientY: centerY,
      });
      event.preventDefault();
      return;
    }
    if (event.pointerId === this.imageFocusDragPointerId && this.imageFocusDragOrigin) {
      this.imageFocusPanX = this.imageFocusDragOrigin.panX + event.clientX - this.imageFocusDragOrigin.x;
      this.imageFocusPanY = this.imageFocusDragOrigin.panY + event.clientY - this.imageFocusDragOrigin.y;
      this.applyImageFocusTransform();
      event.preventDefault();
    }
  }

  handleImageFocusPointerEnd(event) {
    if (this.imageFocusPointers) this.imageFocusPointers.delete(event.pointerId);
    if (event.pointerId === this.imageFocusDragPointerId) {
      this.imageFocusDragPointerId = null;
      this.imageFocusDragOrigin = null;
      if (this.imageFocusStage) this.imageFocusStage.classList.remove('is-panning');
    }
    if ((!this.imageFocusPointers || this.imageFocusPointers.size < 2)) this.imageFocusPinch = null;
    try { if (this.imageFocusStage && typeof this.imageFocusStage.releasePointerCapture === 'function') this.imageFocusStage.releasePointerCapture(event.pointerId); } catch { /* pointer capture already released */ }
  }

  handleImageFocusGestureStart(event) {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden) return;
    event.preventDefault();
    this.imageFocusGestureStartScale = Number.isFinite(this.imageFocusScale) ? this.imageFocusScale : this.imageFocusFitScale();
    this.noteImageFocusActivity();
  }

  handleImageFocusGestureChange(event) {
    if (this.imageFocusGestureStartScale === null || !Number.isFinite(Number(event.scale))) return;
    event.preventDefault();
    this.setImageFocusScale(this.imageFocusGestureStartScale * Number(event.scale), {
      mode: 'custom', clientX: event.clientX, clientY: event.clientY,
    });
  }

  handleImageFocusGestureEnd(event) {
    if (this.imageFocusGestureStartScale === null) return;
    event.preventDefault();
    this.imageFocusGestureStartScale = null;
  }

  handleImageFocusKeyDown(event) {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden || event.defaultPrevented) return;
    if (event.key === 'Tab') {
      const focusable = [...this.imageFocusSurface.querySelectorAll('button:not([disabled]), [href], [tabindex="0"]')]
        .filter(element => element instanceof HTMLElement && element.offsetParent !== null);
      if (focusable.length === 0) {
        event.preventDefault();
        this.imageFocusSurface.focus({preventScroll: true});
        return;
      }
      const current = document.activeElement;
      const index = focusable.indexOf(current);
      const nextIndex = event.shiftKey
        ? (index <= 0 ? focusable.length - 1 : index - 1)
        : (index < 0 || index >= focusable.length - 1 ? 0 : index + 1);
      event.preventDefault();
      focusable[nextIndex].focus({preventScroll: true});
      return;
    }
    const target = event.target;
    if (target instanceof Element && target.closest('.prosemirror-image-focus-toolbar, .prosemirror-image-focus-details')) return;
    const step = event.shiftKey ? 120 : 40;
    let handled = false;
    if (event.key === 'ArrowLeft') handled = this.panImageFocusBy(step, 0);
    else if (event.key === 'ArrowRight') handled = this.panImageFocusBy(-step, 0);
    else if (event.key === 'ArrowUp') handled = this.panImageFocusBy(0, step);
    else if (event.key === 'ArrowDown') handled = this.panImageFocusBy(0, -step);
    if (handled) {
      event.preventDefault();
      event.stopPropagation();
    }
  }

  updateImageFocusControls() {
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
    const displayAvailable = Boolean(
      runtime
      && runtime.available === true
      && (runtime.screenPreviewUrl || runtime.previewUrl || runtime.compatiblePreviewUrl)
      && this.imageFocusImage
      && !this.imageFocusImage.hidden
      && this.imageFocusImage.naturalWidth > 0,
    );
    this.imageFocusZoomOutButton && (this.imageFocusZoomOutButton.disabled = !displayAvailable || (Number.isFinite(this.imageFocusScale) && this.imageFocusScale <= 0.0501));
    this.imageFocusZoomInButton && (this.imageFocusZoomInButton.disabled = !displayAvailable || (Number.isFinite(this.imageFocusScale) && this.imageFocusScale >= 15.999));
    if (this.imageFocusFitButton) {
      this.imageFocusFitButton.disabled = !displayAvailable;
      this.imageFocusFitButton.setAttribute('aria-pressed', this.imageFocusMode === 'fit' ? 'true' : 'false');
      this.imageFocusFitButton.classList.toggle('is-active', this.imageFocusMode === 'fit');
    }
    if (this.imageFocusActualButton) {
      this.imageFocusActualButton.disabled = !displayAvailable;
      this.imageFocusActualButton.setAttribute('aria-pressed', this.imageFocusMode === 'actual' ? 'true' : 'false');
      this.imageFocusActualButton.classList.toggle('is-active', this.imageFocusMode === 'actual');
    }
    if (this.imageFocusEditButton) this.imageFocusEditButton.disabled = !(this.controller && this.controller.editable) || !displayAvailable;
    if (this.imageFocusCopyButton) this.imageFocusCopyButton.disabled = !displayAvailable;
    const hasOriginalSource = Boolean(runtime && runtime.sourceAttachmentId);
    const sourceAvailable = hasOriginalSource && this.controller?.attachmentRegistry.get(runtime.sourceAttachmentId)?.available === true;
    if (this.imageFocusSaveDisplayedButton) {
      this.imageFocusSaveDisplayedButton.textContent = hasOriginalSource ? 'Save edited image…' : 'Save Image As…';
      this.imageFocusSaveDisplayedButton.disabled = !(runtime && runtime.available === true);
    }
    if (this.imageFocusSaveOriginalButton) {
      this.imageFocusSaveOriginalButton.hidden = !hasOriginalSource;
      this.imageFocusSaveOriginalButton.disabled = !sourceAvailable;
    }
    if (this.imageFocusOpenButton) this.imageFocusOpenButton.disabled = !(runtime && runtime.available === true) || (hasOriginalSource && !sourceAvailable);
    if (this.imageFocusResetOriginalButton) {
      this.imageFocusResetOriginalButton.hidden = !hasOriginalSource;
      this.imageFocusResetOriginalButton.disabled = !(this.controller && this.controller.editable) || !sourceAvailable;
    }
  }

  noteImageFocusActivity() {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden) return;
    this.imageFocusLastActivity = Date.now();
    this.imageFocusSurface.classList.remove('is-ui-hidden');
    this.scheduleImageFocusChromeHide();
  }

  imageFocusChromeMustStayVisible() {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden) return false;
    if (this.imageFocusDetailsPanel && !this.imageFocusDetailsPanel.hidden) return true;
    if (this.imageFocusToolbar && (this.imageFocusToolbar.contains(document.activeElement) || this.imageFocusToolbar.matches(':hover'))) return true;
    return (this.imageMenus || []).some(({menu}) => this.imageFocusSurface.contains(menu) && !menu.hidden);
  }

  scheduleImageFocusChromeHide() {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden || this.imageFocusChromeTimer !== null) return;
    const check = () => {
      this.imageFocusChromeTimer = null;
      if (!this.imageFocusSurface || this.imageFocusSurface.hidden) return;
      if (this.imageFocusChromeMustStayVisible()) {
        this.imageFocusChromeTimer = window.setTimeout(check, 900);
        return;
      }
      const remaining = 2400 - (Date.now() - this.imageFocusLastActivity);
      if (remaining > 0) {
        this.imageFocusChromeTimer = window.setTimeout(check, remaining);
        return;
      }
      this.imageFocusSurface.classList.add('is-ui-hidden');
    };
    this.imageFocusChromeTimer = window.setTimeout(check, 2400);
  }

  openImageFocusDetails() {
    if (!this.imageFocusDetailsPanel) return;
    this.closeImageMenus();
    this.populateImageDetailGrid(this.imageFocusDetailsGrid);
    const description = String(this.node.attrs.altText || '').trim();
    this.imageFocusDescription.textContent = description ? `Description: ${description}` : 'No accessibility description.';
    this.imageFocusDetailsPanel.hidden = false;
    this.noteImageFocusActivity();
    window.requestAnimationFrame(() => { if (this.imageFocusDetailsPanel) this.imageFocusDetailsPanel.focus({preventScroll: true}); });
  }

  closeImageFocusDetails() {
    if (!this.imageFocusDetailsPanel) return;
    this.imageFocusDetailsPanel.hidden = true;
    this.noteImageFocusActivity();
    window.requestAnimationFrame(() => {
      const button = this.imageFocusToolbar && this.imageFocusToolbar.querySelector('[aria-label="More image actions"]');
      if (button) button.focus({preventScroll: true});
    });
  }

  handleImageFocusViewportChange() {
    if (!this.imageFocusSurface || this.imageFocusSurface.hidden) return;
    if (this.imageFocusMode === 'fit') this.setImageFocusFit();
    else this.applyImageFocusTransform();
  }

  applyImageFocusState(active) {
    const next = Boolean(active);
    if (next && !this.imageFocusSurface) this.buildImageFocusSurface();
    if (!this.imageFocusSurface) return;
    const entering = next && this.imageFocusSurface.hidden;
    this.imageFocusSurface.hidden = !next;
    if (!next) {
      window.clearTimeout(this.imageFocusChromeTimer);
      this.imageFocusChromeTimer = null;
      if (this.imageFocusPointers) this.imageFocusPointers.clear();
      this.imageFocusPinch = null;
      this.imageFocusDragPointerId = null;
      this.imageFocusDragOrigin = null;
      this.imageFocusGestureStartScale = null;
      if (this.imageFocusStage) this.imageFocusStage.classList.remove('is-panning');
      this.closeImageMenus();
      if (this.imageFocusDetailsPanel) this.imageFocusDetailsPanel.hidden = true;
      this.imageFocusSurface.classList.remove('is-ui-hidden');
      this.imageFocusPreviewUrl = '';
      this.imageFocusPrimaryPreviewUrl = '';
      this.imageFocusCompatiblePreviewUrl = '';
      this.imageFocusScreenPreviewUrl = '';
      this.imageFocusResolution = '';
      this.imageFocusCandidates = [];
      this.imageFocusCandidateIndex = -1;
      this.imageFocusImage.removeAttribute('src');
      this.imageFocusImage.removeAttribute('style');
      this.imageFocusImage.hidden = true;
      this.imageFocusMissing.hidden = true;
      this.imageFocusMode = 'fit';
      this.imageFocusScale = Number.NaN;
      this.imageFocusPanX = 0;
      this.imageFocusPanY = 0;
      return;
    }
    const runtime = this.imageFocusRuntime();
    const available = Boolean(runtime && runtime.available === true);
    const previewUrl = available && typeof runtime.previewUrl === 'string' ? runtime.previewUrl : '';
    const compatiblePreviewUrl = available && typeof runtime.compatiblePreviewUrl === 'string'
      ? runtime.compatiblePreviewUrl
      : '';
    const screenPreviewUrl = available && typeof runtime.screenPreviewUrl === 'string'
      ? runtime.screenPreviewUrl
      : '';
    this.imageFocusSurface.setAttribute('aria-label', `Image focus: ${this.node.attrs.displayName || 'Image'}`);
    this.imageFocusImage.alt = this.node.attrs.altText || '';
    if (entering) {
      this.imageFocusMode = 'fit';
      this.imageFocusScale = Number.NaN;
      this.imageFocusPanX = 0;
      this.imageFocusPanY = 0;
    }

    const previewChanged = previewUrl !== this.imageFocusPrimaryPreviewUrl
      || compatiblePreviewUrl !== this.imageFocusCompatiblePreviewUrl
      || screenPreviewUrl !== this.imageFocusScreenPreviewUrl;
    if (previewChanged) {
      this.imageFocusPrimaryPreviewUrl = previewUrl;
      this.imageFocusCompatiblePreviewUrl = compatiblePreviewUrl;
      this.imageFocusScreenPreviewUrl = screenPreviewUrl;
      this.imageFocusResolution = '';
      this.imageFocusCandidates = [];
      this.imageFocusCandidateIndex = -1;
    }

    if (!previewUrl && !compatiblePreviewUrl && !screenPreviewUrl) {
      this.imageFocusPreviewUrl = '';
      this.imageFocusImage.removeAttribute('src');
      this.imageFocusImage.hidden = true;
      this.imageFocusMissing.hidden = false;
      this.imageFocusMissing.textContent = available ? 'Image preview unavailable' : 'Missing image';
    } else {
      const changed = this.loadImageFocusResolution(previewChanged);
      if (!changed && this.imageFocusImage.complete && this.imageFocusImage.naturalWidth > 0) {
        this.imageFocusImage.hidden = false;
        this.imageFocusMissing.hidden = true;
        if (this.imageFocusMode === 'fit') this.setImageFocusFit();
        else this.applyImageFocusTransform();
      }
    }
    this.updateImageFocusControls();
    this.noteImageFocusActivity();
    window.requestAnimationFrame(() => { if (this.imageFocusSurface) this.imageFocusSurface.focus({preventScroll: true}); });
  }

  setInlineImageFocusPlaceholder(active) {
    this.dom.classList.toggle('prosemirror-image-focus-placeholder', Boolean(active));
  }

  inlineImagePinned() {
    return this.imageNearViewport
      || this.dom.classList.contains('ProseMirror-selectednode')
      || (this.controller?.focusMode === 'image' && this.controller.focusedImageView === this)
      || (this.controller?.focusMode === 'image_edit' && this.controller.focusedImageEditView === this);
  }

  inlineImagePreviewCandidates(runtime) {
    if (!runtime || runtime.available !== true) return [];
    const candidates = [
      typeof runtime.screenPreviewUrl === 'string' ? runtime.screenPreviewUrl : '',
      typeof runtime.previewUrl === 'string' ? runtime.previewUrl : '',
      typeof runtime.compatiblePreviewUrl === 'string' ? runtime.compatiblePreviewUrl : '',
    ].filter(Boolean);
    return candidates.filter((url, index) => candidates.indexOf(url) === index);
  }

  setInlineImagePreviewCandidate(index) {
    const candidates = this.imagePreviewCandidates || [];
    if (index < 0 || index >= candidates.length) {
      this.imagePreviewCandidateIndex = -1;
      this.imagePreviewLoaded = false;
      this.image.removeAttribute('src');
      return false;
    }
    const url = candidates[index];
    this.imagePreviewCandidateIndex = index;
    this.imagePreviewLoaded = false;
    this.image.hidden = true;
    this.missing.hidden = false;
    this.missing.textContent = url === this.imageScreenPreviewUrl
      ? 'Preparing image…'
      : (url === this.imageCompatiblePreviewUrl ? 'Preparing compatible preview…' : 'Loading image…');
    if (this.image.src !== url) this.image.src = url;
    return true;
  }

  advanceInlineImagePreviewCandidate() {
    return this.setInlineImagePreviewCandidate(this.imagePreviewCandidateIndex + 1);
  }

  updateInlineImagePlaceholderGeometry(runtime) {
    if (this.imagePreviewLoaded) {
      this.imageFrame.style.removeProperty('min-height');
      return;
    }
    const pixelSize = imageRuntimePixelSize(runtime);
    if (!pixelSize) {
      this.imageFrame.style.removeProperty('min-height');
      return;
    }
    const {width, height} = pixelSize;
    const editorWidth = Math.max(1, this.view.dom.clientWidth || this.view.dom.getBoundingClientRect().width || width);
    const maxHeight = Math.max(1, Math.min(window.innerHeight * 0.72, 900));
    let targetWidth = Math.min(width, editorWidth);
    if (this.node.attrs.sizeMode === 'percent') {
      targetWidth = editorWidth * Math.max(0.2, Math.min(1, Number(this.node.attrs.widthPercent) / 100 || 1));
    }
    const scale = Math.min(1, targetWidth / width, maxHeight / height);
    const displayWidth = Math.max(1, width * scale);
    const displayHeight = Math.max(1, height * scale);
    if (this.node.attrs.sizeMode !== 'percent') {
      this.imageShell.style.width = `${Math.min(editorWidth, displayWidth)}px`;
    }
    this.imageFrame.style.minHeight = `${Math.min(maxHeight, displayHeight)}px`;
  }

  render() {
    const attrs = this.node.attrs;
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(attrs.attachmentId)
      : null;
    const available = runtime && runtime.available === true;
    const editable = Boolean(this.controller && this.controller.editable);
    this.dom.dataset.attachmentId = attrs.attachmentId;
    this.dom.dataset.attachmentKind = attrs.kind;
    this.dom.dataset.attachmentAvailable = available ? 'true' : 'false';
    this.dom.classList.toggle('prosemirror-attachment-missing', !available);

    if (attrs.kind === 'file') {
      this.icon.textContent = 'FILE';
      this.name.textContent = attrs.displayName;
      const typeLabel = attrs.mediaType || 'FILE';
      this.meta.textContent = available
        ? `${typeLabel} · ${attachmentSizeLabel(attrs.byteSize)}`
        : `Missing attachment · ${attachmentSizeLabel(attrs.byteSize)}`;
      this.openButton.disabled = !available;
      this.deleteButton.disabled = !editable;
      this.dom.setAttribute('aria-label', available ? `Attachment: ${attrs.displayName}` : `Missing attachment: ${attrs.displayName}`);
      return;
    }

    if (attrs.kind === 'pdf') {
      this.icon.textContent = 'PDF';
      this.name.textContent = attrs.displayName;
      this.meta.textContent = available
        ? `PDF · ${attachmentSizeLabel(attrs.byteSize)}`
        : `Missing PDF · ${attachmentSizeLabel(attrs.byteSize)}`;
      this.previewButton.disabled = !available;
      this.openButton.disabled = !available;
      this.deleteButton.disabled = !editable;
      if (!available && !this.pdfPreview.hidden) this.togglePdfPreview(false);
      else if (available && !this.pdfPreview.hidden) {
        this.installPdfViewerDocument();
      }
      this.dom.setAttribute('aria-label', available ? `PDF: ${attrs.displayName}` : `Missing PDF: ${attrs.displayName}`);
      return;
    }

    if (attrs.kind === 'audio' || attrs.kind === 'video') {
      this.icon.textContent = attrs.kind === 'video' ? 'VIDEO' : 'AUDIO';
      this.name.textContent = attrs.displayName;
      const runtimeMediaType = runtime && typeof runtime.mimeType === 'string' ? runtime.mimeType.trim() : '';
      const typeLabel = runtimeMediaType || attrs.mediaType || attrs.kind.toUpperCase();
      this.meta.textContent = available
        ? `${typeLabel} · ${attachmentSizeLabel(attrs.byteSize)}`
        : `Missing ${attrs.kind} · ${attachmentSizeLabel(attrs.byteSize)}`;
      const mediaCandidateState = available ? this.mediaCandidates(runtime, attrs) : {candidates: [], hasAnyPreview: false};
      this.setMediaPreviewCandidates(mediaCandidateState.candidates);
      const mediaCandidate = this.currentMediaPreviewCandidate();
      const previewUrl = mediaCandidate?.url || '';
      this.currentPreviewUrl = previewUrl;

      this.media.setAttribute('aria-label', attrs.displayName);
      this.mediaFallbackButton.disabled = !available;
      this.openButton.disabled = !available;
      this.deleteButton.disabled = !editable;

      if (!previewUrl) {
        this.releaseMediaSource();
        const compatibilityPending = mediaCandidateState.compatibilityPotential
          && !['failed', 'unsupported'].includes(mediaCandidateState.compatibilityState);
        if (compatibilityPending && (this.mediaNearViewport || this.mediaPlaybackPinned)) {
          this.controller?.ensureMediaCompatibility(this);
          this.setMediaStatus(
            attrs.kind === 'video' ? 'Preparing compatible video…' : 'Preparing compatible audio…',
            true,
          );
        } else if (compatibilityPending) {
          this.setMediaStatus(
            attrs.kind === 'video' ? 'Video preview loads when nearby' : 'Audio preview loads when nearby',
            true,
          );
        } else {
          this.setMediaStatus(
            mediaCandidateState.hasAnyPreview ? 'Preview unavailable on this device' : (attrs.kind === 'video' ? 'Video unavailable' : 'Audio unavailable'),
            mediaCandidateState.hasAnyPreview,
          );
        }
      } else if (this.mediaNearViewport || this.mediaPlaybackPinned) {
        if (this.mediaAttachedPreviewUrl !== previewUrl) {
          this.setMediaStatus(
            mediaCandidate?.source === 'compatible'
              ? (attrs.kind === 'video' ? 'Loading compatible video…' : 'Loading compatible audio…')
              : (attrs.kind === 'video' ? 'Loading video…' : 'Loading audio…'),
          );
          this.activateMediaSource(previewUrl);
        } else if (this.mediaPreviewLoaded) {
          this.media.hidden = false;
          this.mediaMissing.hidden = true;
          this.mediaFallbackButton.hidden = true;
          this.mediaControls.hidden = false;
          this.updateMediaControls();
          this.updateMediaAdvancedControls();
        } else {
          this.setMediaStatus(
            mediaCandidate?.source === 'compatible'
              ? (attrs.kind === 'video' ? 'Loading compatible video…' : 'Loading compatible audio…')
              : (attrs.kind === 'video' ? 'Loading video…' : 'Loading audio…'),
          );
        }
      } else {
        this.releaseMediaSource();
        this.setMediaStatus(attrs.kind === 'video' ? 'Video preview loads when nearby' : 'Audio preview loads when nearby');
      }

      this.renderMediaAnnotations();
      this.dom.setAttribute('aria-label', available ? `${attrs.kind}: ${attrs.displayName}` : `Missing ${attrs.kind}: ${attrs.displayName}`);
      return;
    }

    const previewUrl = available && runtime && typeof runtime.previewUrl === 'string' ? runtime.previewUrl : '';
    const compatiblePreviewUrl = available && runtime && typeof runtime.compatiblePreviewUrl === 'string'
      ? runtime.compatiblePreviewUrl
      : '';
    const screenPreviewUrl = available && runtime && typeof runtime.screenPreviewUrl === 'string'
      ? runtime.screenPreviewUrl
      : '';
    const previewChanged = previewUrl !== this.imagePrimaryPreviewUrl
      || compatiblePreviewUrl !== this.imageCompatiblePreviewUrl
      || screenPreviewUrl !== this.imageScreenPreviewUrl;
    if (previewChanged) {
      this.imagePrimaryPreviewUrl = previewUrl;
      this.imageCompatiblePreviewUrl = compatiblePreviewUrl;
      this.imageScreenPreviewUrl = screenPreviewUrl;
      this.imagePreviewCandidates = this.inlineImagePreviewCandidates(runtime);
      this.imagePreviewCandidateIndex = -1;
      this.imagePreviewLoaded = false;
      this.image.removeAttribute('src');
    }

    const shouldLoad = available && this.inlineImagePinned();
    if (!shouldLoad) {
      if (this.image.getAttribute('src')) this.image.removeAttribute('src');
      this.imagePreviewCandidateIndex = -1;
      this.imagePreviewLoaded = false;
      this.image.hidden = true;
      this.missing.hidden = false;
      this.missing.textContent = available ? 'Image' : 'Missing image';
    } else if (this.imagePreviewCandidates.length === 0) {
      this.image.hidden = true;
      this.missing.hidden = false;
      this.missing.textContent = available ? 'Image preview unavailable' : 'Missing image';
    } else if (this.imagePreviewCandidateIndex < 0) {
      this.setInlineImagePreviewCandidate(0);
    } else if (!this.imagePreviewLoaded) {
      this.image.hidden = true;
      this.missing.hidden = false;
    }
    this.image.alt = attrs.altText || '';
    const sizeMode = attrs.sizeMode === 'percent' ? 'percent' : 'auto';
    this.imageShell.dataset.sizeMode = sizeMode;
    if (sizeMode === 'percent') {
      this.imageShell.style.width = `${attrs.widthPercent}%`;
    } else {
      this.imageShell.style.removeProperty('width');
    }
    this.updateInlineImagePlaceholderGeometry(runtime);
    this.imageShell.dataset.alignment = attrs.alignment;
    this.captionInput.value = attrs.caption;
    this.captionInput.disabled = !editable;
    this.captionInput.classList.toggle('is-editing', Boolean(this.captionEditing));
    this.altInput.value = attrs.altText;
    this.altInput.disabled = !editable;
    this.resizeHandle.disabled = !editable;
    this.replaceButton.disabled = !editable;
    this.editButton.disabled = !editable;
    this.deleteButton.disabled = !editable;
    this.openButton.disabled = !available;
    this.viewButton.disabled = !available;
    this.copyButton.disabled = !available || !this.imagePreviewLoaded;
    const hasOriginalSource = Boolean(runtime && runtime.sourceAttachmentId);
    const sourceAvailable = hasOriginalSource && this.controller?.attachmentRegistry.get(runtime.sourceAttachmentId)?.available === true;
    if (this.saveDisplayedButton) {
      this.saveDisplayedButton.textContent = hasOriginalSource ? 'Save edited image…' : 'Save Image As…';
      this.saveDisplayedButton.disabled = !available;
    }
    if (this.saveOriginalButton) {
      this.saveOriginalButton.hidden = !hasOriginalSource;
      this.saveOriginalButton.disabled = !sourceAvailable;
    }
    this.detailsButton.disabled = false;
    if (this.resetOriginalButton) {
      this.resetOriginalButton.hidden = !hasOriginalSource;
      this.resetOriginalButton.disabled = !editable || !sourceAvailable;
    }
    if (this.openButton) this.openButton.disabled = !available || (hasOriginalSource && !sourceAvailable);
    for (const [alignment, button] of this.alignmentButtons) {
      button.disabled = !editable;
      button.setAttribute('aria-checked', alignment === attrs.alignment ? 'true' : 'false');
      button.classList.toggle('is-active', alignment === attrs.alignment);
    }
    for (const [key, button] of this.sizeButtons) {
      const active = key === 'auto'
        ? sizeMode === 'auto'
        : sizeMode === 'percent' && Number(key) === attrs.widthPercent;
      button.disabled = !editable;
      button.setAttribute('aria-checked', active ? 'true' : 'false');
      button.classList.toggle('is-active', active);
    }
    if (this.controller && this.controller.focusMode === 'image' && this.controller.focusedImageView === this) {
      this.applyImageFocusState(true);
    }
    this.dom.setAttribute(
      'aria-label',
      available ? `Image: ${attrs.displayName}` : `Missing image: ${attrs.displayName}`,
    );
  }

  updateAttrs(patch) {
    if (!this.controller?.editable) return false;
    const position = this.safePosition();
    if (position === null) return false;
    const attrs = {...this.node.attrs, ...patch};
    return this.controller.runDiscreteUserAction(() => {
      let transaction = this.view.state.tr.setNodeMarkup(position, undefined, attrs);
      transaction = transaction.setSelection(NodeSelection.create(transaction.doc, position));
      this.view.dispatch(transaction.scrollIntoView());
      return true;
    });
  }

  beginResize(event) {
    if (!this.controller?.editable || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const editorRect = this.view.dom.getBoundingClientRect();
    const startWidth = this.imageShell.getBoundingClientRect().width;
    const startX = event.clientX;
    const pointerId = event.pointerId;
    this.resizeHandle.setPointerCapture?.(pointerId);
    const onMove = moveEvent => {
      const nextWidth = startWidth + (moveEvent.clientX - startX);
      const percent = Math.round((nextWidth / Math.max(1, editorRect.width)) * 100);
      const clamped = Math.max(specLimit('attachment_width_percent_min'), Math.min(specLimit('attachment_width_percent_max'), percent));
      this.imageShell.style.width = `${clamped}%`;
      this.pendingWidthPercent = clamped;
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove, true);
      window.removeEventListener('pointerup', onUp, true);
      window.removeEventListener('pointercancel', onUp, true);
      const widthPercent = this.pendingWidthPercent;
      this.pendingWidthPercent = null;
      if (Number.isInteger(widthPercent) && (widthPercent !== this.node.attrs.widthPercent || this.node.attrs.sizeMode !== 'percent')) {
        this.updateAttrs({widthPercent, sizeMode: 'percent'});
      } else {
        this.render();
      }
    };
    window.addEventListener('pointermove', onMove, true);
    window.addEventListener('pointerup', onUp, true);
    window.addEventListener('pointercancel', onUp, true);
  }

  openOriginal() {
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
    if (runtime?.available !== true) return false;
    const attachmentId = runtime.sourceAttachmentId || this.node.attrs.attachmentId;
    const sourceRuntime = this.controller.attachmentRegistry.get(attachmentId);
    if (sourceRuntime?.available !== true) return false;
    return emitAttachmentCommand({kind: 'file_action', action: 'open', attachment_id: attachmentId});
  }

  revealInFinder() {
    const runtime = this.controller && this.controller.attachmentRegistry
      ? this.controller.attachmentRegistry.get(this.node.attrs.attachmentId)
      : null;
    if (runtime?.available !== true) return false;
    emitAttachmentCommand({kind: 'file_action', action: 'reveal', attachment_id: this.node.attrs.attachmentId});
    return true;
  }

  deleteNode() {
    if (!this.controller?.editable) return false;
    const position = this.safePosition();
    if (position === null) return false;
    return Boolean(this.controller.runDiscreteUserAction(() => {
      let transaction = this.view.state.tr.delete(position, position + this.node.nodeSize);
      transaction = transaction.setSelection(TextSelection.near(transaction.doc.resolve(Math.min(position, transaction.doc.content.size)), -1));
      this.view.dispatch(transaction.scrollIntoView());
      return true;
    }));
  }

  safePosition() {
    try {
      const position = this.getPos();
      return Number.isInteger(position) ? position : null;
    } catch {
      return null;
    }
  }

  update(node) {
    if (node.type !== schema.nodes.attachment || node.attrs.kind !== this.node.attrs.kind) return false;
    this.node = node;
    this.dom.setAttribute('aria-label', `Attachment: ${node.attrs.displayName || 'file'}. Press Shift+F10 for actions.`);
    this.render();
    return true;
  }

  selectNode() {
    this.dom.classList.add('ProseMirror-selectednode');
    if (this.node.attrs.kind === 'image') this.render();
  }

  deselectNode() {
    this.dom.classList.remove('ProseMirror-selectednode');
    this.closeImageMenus();
    this.captionEditing = false;
    if (this.node.attrs.kind === 'image') this.render();
  }

  stopEvent(event) {
    const target = event.target;
    if (!(target instanceof Element)) return false;
    if (this.node.attrs.kind === 'image') {
      return Boolean(target.closest('.prosemirror-image-controls, .prosemirror-image-caption, .prosemirror-image-resize-handle'));
    }
    if (this.node.attrs.kind === 'pdf') {
      return Boolean(target.closest('.prosemirror-pdf-controls, .prosemirror-pdf-preview'));
    }
    return Boolean(target.closest(
      '.prosemirror-attachment-actions, .prosemirror-media-frame, .prosemirror-media-caption, .prosemirror-media-annotation-summary-bar, .prosemirror-media-annotations, audio, video',
    ));
  }

  ignoreMutation() {
    return true;
  }

  destroy() {
    if (this.controller?.attachmentContextView === this) {
      this.controller.closeAttachmentContextMenu(false);
    }
    if (this.imageVisibilityObserver) this.imageVisibilityObserver.disconnect();
    if (this.mediaVisibilityObserver) this.mediaVisibilityObserver.disconnect();
    if (this.media && this.mediaPreviewLoaded && !this.media.ended) this.persistMediaPlaybackPosition(true);
    if (this.media) this.releaseMediaSource();
    if (this.mediaFullscreenChangeHandler) {
      document.removeEventListener('fullscreenchange', this.mediaFullscreenChangeHandler);
      document.removeEventListener('webkitfullscreenchange', this.mediaFullscreenChangeHandler);
    }
    if (this.mediaEnterPictureInPictureHandler) this.media?.removeEventListener('enterpictureinpicture', this.mediaEnterPictureInPictureHandler);
    if (this.mediaLeavePictureInPictureHandler) this.media?.removeEventListener('leavepictureinpicture', this.mediaLeavePictureInPictureHandler);
    if (this.mediaWebkitPresentationModeHandler) this.media?.removeEventListener('webkitpresentationmodechanged', this.mediaWebkitPresentationModeHandler);
    if (this.mediaMoreOutsideClickHandler) document.removeEventListener('pointerdown', this.mediaMoreOutsideClickHandler);
    window.clearTimeout(this.pdfBootstrapTimer);
    if (this.pdfFocusActive) this.controller?.exitPdfFocus(this);
    if (this.controller?.focusMode === 'image' && this.controller.focusedImageView === this) {
      this.controller.exitImageFocus(this, false);
    }
    if (this.controller?.focusMode === 'image_edit' && this.controller.focusedImageEditView === this) {
      this.controller.exitImageEditFocus(this, false, false);
    }
    if (this.pdfFrame) this.pdfFrame.remove();
    if (this.pdfMessageHandler) window.removeEventListener('message', this.pdfMessageHandler);
    if (this.imageFocusSurface) this.imageFocusSurface.remove();
    if (this.imageEditSurface) this.imageEditSurface.remove();
    this.controller?.attachmentViews?.delete(this);
  }
}
export {AttachmentView, attachmentNodeAttrs};

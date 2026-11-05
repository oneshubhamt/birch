import {Fragment, Schema} from "prosemirror-model";
import {tableNodes} from "prosemirror-tables";
import {attachmentImportPlaceholderDom, canonicalAttachmentKind} from "../attachments/import.js";
import {BULLET_LIST_MARKERS, DEFAULT_FONT_SIZE_POINTS, FONT_FAMILIES, FONT_SIZE_MAX, FONT_SIZE_MIN, HIGHLIGHT_COLORS, LINE_SPACINGS, ORDERED_LIST_STYLES, TEXT_ALIGNMENTS, TEXT_COLORS} from "../config.js";
import {DOCUMENT_SPEC, ENVELOPE_SPEC, assertExactImplementationVocabulary, markAttrDefault, nodeAttrDefault, nodeRule, specLimit, truncateUtf8, utf8ByteLength, withMarkContract, withNodeContract} from "./spec.js";
import {normalizeSafeHref, safeHref, validAttachmentId, validNoteId, validateSerializedEditorState} from "./validation.js";



const tableNodeSpecs = tableNodes({
  tableGroup: nodeRule('table').prosemirror.group,
  cellContent: nodeRule('table_cell').prosemirror.content,
  cellAttributes: {},
});

const nodeImplementations = {
  doc: {},
  paragraph: {
    parseDOM: [{
      tag: 'p',
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        const boundary = element.dataset.atomicBoundary === 'true';
        return {
          lineSpacing: boundary ? null : normalizedLineSpacing(element.dataset.lineSpacing),
          ...(boundary ? defaultTextblockLayoutAttrs() : textblockLayoutAttrsFromDOM(element)),
          boundary,
        };
      },
    }],
    toDOM(node) {
      return ['p', textblockDOMAttrs('prosemirror-paragraph', node), 0];
    },
  },
  heading: {
    defining: true,
    parseDOM: Array.from({length: specLimit('heading_level_max') - specLimit('heading_level_min') + 1}, (_, index) => specLimit('heading_level_min') + index).map(level => ({
      tag: `h${level}`,
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        return {
          level,
          lineSpacing: normalizedLineSpacing(element.dataset.lineSpacing),
          ...textblockLayoutAttrsFromDOM(element),
        };
      },
    })),
    toDOM(node) {
      const level = Math.max(specLimit('heading_level_min'), Math.min(specLimit('heading_level_max'), Number(node.attrs.level) || specLimit('heading_level_min')));
      return [`h${level}`, textblockDOMAttrs(`prosemirror-heading-h${level}`, node), 0];
    },
  },
  blockquote: {
    defining: true,
    parseDOM: [{tag: 'blockquote'}],
    toDOM() {
      return ['blockquote', {class: 'prosemirror-quote'}, 0];
    },
  },
  code_block: {
    code: true,
    defining: true,
    parseDOM: [{
      tag: 'pre',
      preserveWhitespace: 'full',
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        return {
          language: element.dataset.language || null,
          lineSpacing: normalizedLineSpacing(element.dataset.lineSpacing),
        };
      },
    }],
    toDOM(node) {
      const attrs = textblockDOMAttrs('prosemirror-code', node);
      attrs['data-context-long-press'] = 'true';
      attrs['aria-haspopup'] = 'menu';
      attrs['aria-keyshortcuts'] = 'Shift+F10';
      if (typeof node.attrs.language === 'string' && node.attrs.language !== '') {
        attrs['data-language'] = node.attrs.language;
      }
      return ['pre', attrs, ['code', 0]];
    },
  },
  bullet_list: {
    parseDOM: [{
      tag: 'ul:not([data-task-list])',
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        return {marker: BULLET_LIST_MARKERS.includes(element.dataset.listMarker) ? element.dataset.listMarker : nodeAttrDefault('bullet_list', 'marker')};
      },
    }],
    toDOM(node) {
      const marker = BULLET_LIST_MARKERS.includes(node.attrs.marker) ? node.attrs.marker : nodeAttrDefault('bullet_list', 'marker');
      return ['ul', {
        class: marker === 'dash' ? 'prosemirror-list-ul prosemirror-list-dash' : 'prosemirror-list-ul',
        'data-list-marker': marker,
      }, 0];
    },
  },
  ordered_list: {
    parseDOM: [{
      tag: 'ol',
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        const start = element.getAttribute('start');
        const style = element.dataset.listStyle;
        return {
          order: start === null ? 1 : Number(start) || 1,
          style: ORDERED_LIST_STYLES.includes(style) ? style : nodeAttrDefault('ordered_list', 'style'),
          restart: element.dataset.listRestart === 'true',
        };
      },
    }],
    toDOM(node) {
      const attrs = {
        class: 'prosemirror-list-ol',
        'data-list-style': node.attrs.style || nodeAttrDefault('ordered_list', 'style'),
      };
      if (node.attrs.order !== 1) {
        attrs.start = node.attrs.order;
      }
      if (node.attrs.restart) {
        attrs['data-list-restart'] = 'true';
      }
      return ['ol', attrs, 0];
    },
  },
  task_list: {
    parseDOM: [{tag: 'ul[data-task-list]'}],
    toDOM() {
      return ['ul', {class: 'prosemirror-list-check', 'data-task-list': 'true'}, 0];
    },
  },
  list_item: {
    defining: true,
    parseDOM: [{tag: 'li:not([data-task-item])'}],
    toDOM() {
      return ['li', {class: 'prosemirror-list-item'}, 0];
    },
  },
  task_item: {
    defining: true,
    parseDOM: [{
      tag: 'li[data-task-item]',
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        return {checked: element.getAttribute('data-checked') === 'true'};
      },
    }],
    toDOM(node) {
      const checked = Boolean(node.attrs.checked);
      return [
        'li',
        {
          class: checked
            ? 'prosemirror-list-item prosemirror-list-item-checked'
            : 'prosemirror-list-item prosemirror-list-item-unchecked',
          'data-task-item': 'true',
          'data-checked': checked ? 'true' : 'false',
        },
        0,
      ];
    },
  },
  horizontal_rule: {
    atom: true,
    selectable: true,
    parseDOM: [{tag: 'hr'}],
    toDOM() {
      return ['hr', {class: 'prosemirror-horizontal-rule', 'data-rustnote-horizontal-rule': 'true'}];
    },
  },
  attachment_pending: {
    atom: true,
    selectable: false,
    draggable: false,
    createGapCursor: true,
    toDOM(node) {
      const items = Array.isArray(node.attrs.items) && node.attrs.items.length > 0
        ? node.attrs.items
        : [{
          file_name: node.attrs.label,
          kind: canonicalAttachmentKind(node.attrs.picker),
          status: node.attrs.status,
        }];
      const dom = attachmentImportPlaceholderDom(items);
      dom.dataset.rustnoteAttachmentPending = 'true';
      dom.dataset.attachmentRequestId = node.attrs.requestId;
      dom.dataset.attachmentPicker = canonicalAttachmentKind(node.attrs.picker);
      return dom;
    },
  },
  attachment: {
    atom: true,
    selectable: true,
    draggable: true,
    createGapCursor: true,
    parseDOM: [{
      tag: '[data-rustnote-attachment]',
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        return {
          attachmentId: element.dataset.attachmentId || '',
          kind: element.dataset.attachmentKind || nodeAttrDefault('attachment', 'kind'),
          displayName: element.dataset.attachmentName || nodeAttrDefault('attachment', 'displayName'),
          mediaType: element.dataset.attachmentMediaType || null,
          byteSize: Number(element.dataset.attachmentByteSize) || 0,
          caption: element.dataset.attachmentCaption || '',
          altText: element.dataset.attachmentAltText || '',
          alignment: element.dataset.attachmentAlignment || nodeAttrDefault('attachment', 'alignment'),
          widthPercent: Number(element.dataset.attachmentWidthPercent) || nodeAttrDefault('attachment', 'widthPercent'),
          sizeMode: element.dataset.attachmentSizeMode === 'percent' ? 'percent' : nodeAttrDefault('attachment', 'sizeMode'),
          mediaAnnotations: parseMediaAnnotationsDataset(element.dataset.attachmentMediaAnnotations),
          mediaTrimStartMs: element.dataset.attachmentMediaTrimStartMs ? Number(element.dataset.attachmentMediaTrimStartMs) : null,
          mediaTrimEndMs: element.dataset.attachmentMediaTrimEndMs ? Number(element.dataset.attachmentMediaTrimEndMs) : null,
          nodeSchemaVersion: Number(element.dataset.attachmentSchemaVersion) || nodeAttrDefault('attachment', 'nodeSchemaVersion'),
        };
      },
    }],
    toDOM(node) {
      const attrs = {
        'data-rustnote-attachment': 'true',
        'data-attachment-id': node.attrs.attachmentId,
        'data-attachment-kind': node.attrs.kind,
        'data-attachment-name': node.attrs.displayName,
        'data-attachment-media-type': node.attrs.mediaType || '',
        'data-attachment-byte-size': String(node.attrs.byteSize),
        'data-attachment-caption': node.attrs.caption,
        'data-attachment-alt-text': node.attrs.altText,
        'data-attachment-alignment': node.attrs.alignment,
        'data-attachment-width-percent': String(node.attrs.widthPercent),
        'data-attachment-size-mode': node.attrs.sizeMode,
        'data-attachment-schema-version': String(node.attrs.nodeSchemaVersion),
      };
      const annotations = normalizedMediaAnnotations(node.attrs.mediaAnnotations);
      if (annotations.length > 0) attrs['data-attachment-media-annotations'] = JSON.stringify(annotations);
      if (Number.isSafeInteger(node.attrs.mediaTrimStartMs) && node.attrs.mediaTrimStartMs > 0) {
        attrs['data-attachment-media-trim-start-ms'] = String(node.attrs.mediaTrimStartMs);
      }
      if (Number.isSafeInteger(node.attrs.mediaTrimEndMs) && node.attrs.mediaTrimEndMs > 0) {
        attrs['data-attachment-media-trim-end-ms'] = String(node.attrs.mediaTrimEndMs);
      }
      return ['div', attrs];
    },
  },
  ...tableNodeSpecs,
  math_inline: {
    selectable: true,
    code: true,
    parseDOM: [{tag: 'span[data-rustnote-math-inline]'}],
    toDOM() {
      return ['span', {'data-rustnote-math-inline': 'true'}, 0];
    },
  },
  math_display: {
    selectable: true,
    defining: true,
    code: true,
    createGapCursor: true,
    parseDOM: [{tag: 'div[data-rustnote-math-display]'}],
    toDOM() {
      return ['div', {'data-rustnote-math-display': 'true'}, 0];
    },
  },
  text: {},
  hard_break: {
    selectable: false,
    parseDOM: [{tag: 'br'}],
    toDOM() {
      return ['br'];
    },
  },
};

assertExactImplementationVocabulary('node', nodeImplementations, DOCUMENT_SPEC.nodes);
const nodes = Object.fromEntries(
  Object.entries(nodeImplementations).map(([name, implementation]) => [name, withNodeContract(name, implementation)]),
);

const markImplementations = {
  strong: {
    parseDOM: [
      {tag: 'strong'},
      {tag: 'b', getAttrs: node => node.style.fontWeight !== 'normal' && null},
      {style: 'font-weight=400', clearMark: mark => mark.type.name === 'strong'},
      {style: 'font-weight', getAttrs: value => /^(bold(er)?|[5-9]\d{2,})$/.test(String(value)) && null},
    ],
    toDOM() {
      return ['strong', {class: 'prosemirror-text-bold'}, 0];
    },
  },
  em: {
    parseDOM: [{tag: 'i'}, {tag: 'em'}, {style: 'font-style=italic'}],
    toDOM() {
      return ['em', {class: 'prosemirror-text-italic'}, 0];
    },
  },
  underline: {
    parseDOM: [{tag: 'u'}, {style: 'text-decoration', getAttrs: value => String(value).includes('underline') && null}],
    toDOM() {
      return ['u', {class: 'prosemirror-text-underline'}, 0];
    },
  },
  strike: {
    parseDOM: [
      {tag: 's'},
      {tag: 'del'},
      {style: 'text-decoration', getAttrs: value => String(value).includes('line-through') && null},
    ],
    toDOM() {
      return ['s', {class: 'prosemirror-text-strikethrough'}, 0];
    },
  },
  superscript: {
    parseDOM: [{tag: 'sup'}],
    toDOM() {
      return ['sup', {class: 'prosemirror-text-superscript'}, 0];
    },
  },
  subscript: {
    parseDOM: [{tag: 'sub'}],
    toDOM() {
      return ['sub', {class: 'prosemirror-text-subscript'}, 0];
    },
  },
  font_family: {
    parseDOM: [{
      tag: 'span[data-font-family]',
      getAttrs(dom) {
        const family = /** @type {HTMLElement} */ (dom).dataset.fontFamily;
        return FONT_FAMILIES.includes(family) ? {family} : false;
      },
    }],
    toDOM(mark) {
      return ['span', {
        class: 'prosemirror-text-font-family',
        'data-font-family': mark.attrs.family,
      }, 0];
    },
  },
  font_size: {
    parseDOM: [{
      tag: 'span[data-font-size]',
      getAttrs(dom) {
        const size = normalizedFontSize(/** @type {HTMLElement} */ (dom).dataset.fontSize);
        return size === null ? false : {size};
      },
    }],
    toDOM(mark) {
      const size = normalizedFontSize(mark.attrs.size) || 16;
      return ['span', {
        class: 'prosemirror-text-font-size',
        'data-font-size': String(size),
        style: `font-size: ${size}pt`,
      }, 0];
    },
  },
  text_color: {
    parseDOM: [{
      tag: 'span[data-text-color]',
      getAttrs(dom) {
        const color = /** @type {HTMLElement} */ (dom).dataset.textColor;
        return TEXT_COLORS.includes(color) && color !== 'default' ? {color} : false;
      },
    }],
    toDOM(mark) {
      return ['span', {
        class: 'prosemirror-text-color',
        'data-text-color': mark.attrs.color,
      }, 0];
    },
  },
  highlight: {
    parseDOM: [{
      tag: 'mark',
      getAttrs(dom) {
        const color = /** @type {HTMLElement} */ (dom).dataset.highlightColor;
        return {color: HIGHLIGHT_COLORS.includes(color) ? color : markAttrDefault('highlight', 'color')};
      },
    }],
    toDOM(mark) {
      const color = HIGHLIGHT_COLORS.includes(mark.attrs.color) ? mark.attrs.color : markAttrDefault('highlight', 'color');
      return ['mark', {
        class: 'prosemirror-text-highlight',
        'data-highlight-color': color,
      }, 0];
    },
  },
  link: {
    inclusive: false,
    parseDOM: [{
      tag: 'a[href]',
      getAttrs(dom) {
        const element = /** @type {HTMLAnchorElement} */ (dom);
        const href = element.getAttribute('href');
        return safeHref(href)
          ? {href, title: element.getAttribute('title')}
          : false;
      },
    }],
    toDOM(node) {
      return [
        'a',
        {
          class: 'prosemirror-link',
          href: node.attrs.href,
          title: node.attrs.title,
          rel: 'noopener noreferrer',
          target: '_blank',
          'data-context-long-press': 'true',
          'aria-keyshortcuts': 'Shift+F10',
          'aria-haspopup': 'menu',
        },
        0,
      ];
    },
  },
  note_link: {
    inclusive: false,
    parseDOM: [{
      tag: 'span[data-rustnote-note-link]',
      getAttrs(dom) {
        const noteId = /** @type {HTMLElement} */ (dom).dataset.noteId;
        return validNoteId(noteId) ? {noteId} : false;
      },
    }],
    toDOM(mark) {
      return [
        'span',
        {
          class: 'prosemirror-note-link',
          'data-rustnote-note-link': 'true',
          'data-note-id': mark.attrs.noteId,
          role: 'link',
          tabindex: '0',
        },
        0,
      ];
    },
  },
  media_link: {
    inclusive: false,
    parseDOM: [{
      tag: 'span[data-rustnote-media-link]',
      getAttrs(dom) {
        const element = /** @type {HTMLElement} */ (dom);
        const attrs = {
          noteId: element.dataset.noteId || '',
          attachmentId: element.dataset.attachmentId || '',
          startMs: Number(element.dataset.mediaStartMs),
          endMs: element.dataset.mediaEndMs ? Number(element.dataset.mediaEndMs) : null,
        };
        return validMediaLinkAttrs(attrs) ? attrs : false;
      },
    }],
    toDOM(mark) {
      const attrs = {
        class: 'prosemirror-note-link prosemirror-media-link',
        'data-rustnote-media-link': 'true',
        'data-note-id': mark.attrs.noteId,
        'data-attachment-id': mark.attrs.attachmentId,
        'data-media-start-ms': String(mark.attrs.startMs),
        role: 'link',
        tabindex: '0',
      };
      if (Number.isSafeInteger(mark.attrs.endMs)) attrs['data-media-end-ms'] = String(mark.attrs.endMs);
      return ['span', attrs, 0];
    },
  },
  code: {
    code: true,
    parseDOM: [{tag: 'code'}],
    toDOM() {
      return ['code', {class: 'prosemirror-text-code'}, 0];
    },
  },
};

assertExactImplementationVocabulary('mark', markImplementations, DOCUMENT_SPEC.marks);
const marks = Object.fromEntries(
  Object.entries(markImplementations).map(([name, implementation]) => [name, withMarkContract(name, implementation)]),
);

function normalizedFontSize(value) {
  if (value === null || value === undefined || String(value).trim() === '') {
    return null;
  }
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) {
    return null;
  }
  return Math.max(FONT_SIZE_MIN, Math.min(FONT_SIZE_MAX, Math.round(numeric)));
}

function normalizedLineSpacing(value) {
  const normalized = String(value || '');
  return LINE_SPACINGS.includes(normalized) ? normalized : null;
}

function normalizedTextAlignment(value) {
  const normalized = String(value || 'left').toLowerCase();
  return TEXT_ALIGNMENTS.includes(normalized) ? normalized : 'left';
}

function normalizedTextblockIndent(value, maximum) {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return 0;
  return Math.max(0, Math.min(maximum, Math.round(numeric)));
}

function defaultTextblockLayoutAttrs() {
  return {textAlignment: 'left', leftIndent: 0, rightIndent: 0, firstLineIndent: 0, dropCap: false};
}

function textblockLayoutAttrsFromDOM(element) {
  const firstLineIndent = normalizedTextblockIndent(
    element.dataset.firstLineIndent,
    specLimit('paragraph_first_line_indent_max'),
  );
  return {
    textAlignment: normalizedTextAlignment(element.dataset.textAlignment || element.style.textAlign),
    leftIndent: normalizedTextblockIndent(element.dataset.leftIndent, specLimit('paragraph_side_indent_max')),
    rightIndent: normalizedTextblockIndent(element.dataset.rightIndent, specLimit('paragraph_side_indent_max')),
    firstLineIndent,
    dropCap: firstLineIndent === 0 && element.dataset.dropCap === 'true',
  };
}

function defaultFontSizeForBlockStyle(style) {
  return DEFAULT_FONT_SIZE_POINTS[style] || DEFAULT_FONT_SIZE_POINTS.paragraph;
}

function textblockDOMAttrs(className, node) {
  const attrs = {class: className};
  const lineSpacing = normalizedLineSpacing(node.attrs.lineSpacing);
  if (lineSpacing !== null) attrs['data-line-spacing'] = lineSpacing;

  const textAlignment = normalizedTextAlignment(node.attrs.textAlignment);
  const leftIndent = normalizedTextblockIndent(node.attrs.leftIndent, specLimit('paragraph_side_indent_max'));
  const rightIndent = normalizedTextblockIndent(node.attrs.rightIndent, specLimit('paragraph_side_indent_max'));
  const firstLineIndent = normalizedTextblockIndent(node.attrs.firstLineIndent, specLimit('paragraph_first_line_indent_max'));
  if (textAlignment !== 'left') attrs['data-text-alignment'] = textAlignment;
  if (leftIndent > 0) attrs['data-left-indent'] = String(leftIndent);
  if (rightIndent > 0) attrs['data-right-indent'] = String(rightIndent);
  if (firstLineIndent > 0) attrs['data-first-line-indent'] = String(firstLineIndent);
  if (node.attrs.dropCap === true) attrs['data-drop-cap'] = 'true';

  const layoutStyles = [];
  if (leftIndent > 0) layoutStyles.push(`--rustnote-left-indent:${leftIndent}pt`);
  if (rightIndent > 0) layoutStyles.push(`--rustnote-right-indent:${rightIndent}pt`);
  if (firstLineIndent > 0) layoutStyles.push(`--rustnote-first-line-indent:${firstLineIndent}pt`);
  if (layoutStyles.length > 0) attrs.style = layoutStyles.join(';');

  if (node.type.name === 'paragraph' && node.attrs.boundary === true) {
    attrs['data-atomic-boundary'] = 'true';
  }
  return attrs;
}

const schema = new Schema({nodes, marks});



const MEDIA_ANNOTATION_LIMIT = specLimit('media_annotation_limit');
const MEDIA_ANNOTATION_LABEL_MAX = specLimit('media_annotation_label_max_bytes');
const MEDIA_ANNOTATION_TAG_MAX = specLimit('media_annotation_tag_max_bytes');
const MEDIA_ANNOTATION_NOTE_MAX = specLimit('media_annotation_note_max_bytes');
const MEDIA_RESUME_PERSIST_INTERVAL_MS = 10_000;
const MEDIA_RESUME_STORAGE_PREFIX = 'rustnote.media-resume.v1:';
const MEDIA_RESUME_MAX_ENTRIES = 512;
const MEDIA_RESUME_MAX_AGE_MS = 180 * 24 * 60 * 60 * 1000;
const MEDIA_RESUME_PRUNE_WRITE_INTERVAL = 32;
const MEDIA_DEEP_LINK_STORAGE_KEY = 'rustnote.pending-media-link.v1';
let mediaResumeWritesSincePrune = 0;
let mediaResumePrunedThisSession = false;



function mediaResumeStorageKey(attachmentId) {
  return validAttachmentId(attachmentId) ? `${MEDIA_RESUME_STORAGE_PREFIX}${attachmentId}` : '';
}

function normalizedMediaResumeRecord(raw) {
  if (typeof raw !== 'string' || raw.trim() === '') return null;
  const legacy = Number(raw);
  if (Number.isSafeInteger(legacy) && legacy > 0) {
    return {positionMs: legacy, updatedAt: Date.now()};
  }
  try {
    const parsed = JSON.parse(raw);
    const positionMs = Number(parsed?.positionMs);
    const updatedAt = Number(parsed?.updatedAt);
    if (!Number.isSafeInteger(positionMs) || positionMs <= 0) return null;
    if (!Number.isSafeInteger(updatedAt) || updatedAt <= 0) return null;
    return {positionMs, updatedAt};
  } catch {
    return null;
  }
}

function pruneMediaResumeStorage(force = false) {
  if (!force && mediaResumePrunedThisSession) return false;
  const now = Date.now();
  const records = [];
  try {
    for (let index = localStorage.length - 1; index >= 0; index -= 1) {
      const key = localStorage.key(index);
      if (typeof key !== 'string' || !key.startsWith(MEDIA_RESUME_STORAGE_PREFIX)) continue;
      const raw = localStorage.getItem(key);
      const record = normalizedMediaResumeRecord(raw);
      if (!record || now - record.updatedAt > MEDIA_RESUME_MAX_AGE_MS) {
        localStorage.removeItem(key);
        continue;
      }
      if (typeof raw === 'string' && !raw.trim().startsWith('{')) {
        localStorage.setItem(key, JSON.stringify(record));
      }
      records.push({key, updatedAt: record.updatedAt});
    }
    records.sort((left, right) => right.updatedAt - left.updatedAt);
    for (const record of records.slice(MEDIA_RESUME_MAX_ENTRIES)) localStorage.removeItem(record.key);
    mediaResumePrunedThisSession = true;
    mediaResumeWritesSincePrune = 0;
    return true;
  } catch {
    return false;
  }
}

function readMediaResumePosition(attachmentId) {
  const key = mediaResumeStorageKey(attachmentId);
  if (!key) return 0;
  try {
    const record = normalizedMediaResumeRecord(localStorage.getItem(key));
    return record ? record.positionMs : 0;
  } catch {
    return 0;
  }
}

function writeMediaResumePosition(attachmentId, positionMs) {
  const key = mediaResumeStorageKey(attachmentId);
  if (!key) return false;
  const value = Math.max(0, Math.round(Number(positionMs) || 0));
  try {
    if (value > 0) {
      localStorage.setItem(key, JSON.stringify({positionMs: value, updatedAt: Date.now()}));
      mediaResumeWritesSincePrune += 1;
      if (mediaResumeWritesSincePrune >= MEDIA_RESUME_PRUNE_WRITE_INTERVAL) {
        mediaResumePrunedThisSession = false;
        pruneMediaResumeStorage(true);
      }
    } else {
      localStorage.removeItem(key);
    }
    return true;
  } catch {
    return false;
  }
}

function normalizedMediaAnnotation(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const id = typeof value.id === 'string' ? value.id.trim() : '';
  const startMs = Number(value.startMs);
  const endMs = value.endMs === null || value.endMs === undefined ? null : Number(value.endMs);
  const label = typeof value.label === 'string' ? truncateUtf8(value.label, MEDIA_ANNOTATION_LABEL_MAX) : '';
  const tag = typeof value.tag === 'string' ? truncateUtf8(value.tag, MEDIA_ANNOTATION_TAG_MAX) : '';
  const note = typeof value.note === 'string' ? truncateUtf8(value.note, MEDIA_ANNOTATION_NOTE_MAX) : '';
  if (!id || utf8ByteLength(id) > specLimit('media_annotation_id_max_bytes') || /[\u0000-\u001f\u007f-\u009f]/u.test(id)) return null;
  if (!Number.isSafeInteger(startMs) || startMs < 0) return null;
  if (endMs !== null && (!Number.isSafeInteger(endMs) || endMs <= startMs)) return null;
  return {id, startMs, endMs, label, tag, note};
}

function normalizedMediaAnnotations(value) {
  if (!Array.isArray(value)) return [];
  const seen = new Set();
  const annotations = [];
  for (const candidate of value.slice(0, MEDIA_ANNOTATION_LIMIT)) {
    const annotation = normalizedMediaAnnotation(candidate);
    if (!annotation || seen.has(annotation.id)) continue;
    seen.add(annotation.id);
    annotations.push(annotation);
  }
  annotations.sort((left, right) => {
    const timeOrder = left.startMs - right.startMs || (left.endMs || left.startMs) - (right.endMs || right.startMs);
    if (timeOrder !== 0) return timeOrder;
    return left.id === right.id ? 0 : (left.id < right.id ? -1 : 1);
  });
  return annotations;
}

function parseMediaAnnotationsDataset(value) {
  if (typeof value !== 'string' || value.trim() === '') return null;
  try {
    const annotations = normalizedMediaAnnotations(JSON.parse(value));
    return annotations.length > 0 ? annotations : null;
  } catch {
    return null;
  }
}

function validMediaLinkAttrs(attrs) {
  return Boolean(
    attrs
    && validNoteId(attrs.noteId)
    && validAttachmentId(attrs.attachmentId)
    && Number.isSafeInteger(attrs.startMs)
    && attrs.startMs >= 0
    && (attrs.endMs === null || (Number.isSafeInteger(attrs.endMs) && attrs.endMs > attrs.startMs)),
  );
}

function mediaDeepLinkUri(attrs) {
  if (!validMediaLinkAttrs(attrs)) return '';
  const query = new URLSearchParams({start: String(attrs.startMs)});
  if (Number.isSafeInteger(attrs.endMs)) query.set('end', String(attrs.endMs));
  return `rustnote://media/${encodeURIComponent(attrs.noteId)}/${encodeURIComponent(attrs.attachmentId)}?${query.toString()}`;
}

function parseMediaDeepLinkUri(value) {
  if (typeof value !== 'string' || !value.startsWith('rustnote://media/')) return null;
  try {
    const parsed = new URL(value);
    if (parsed.protocol !== 'rustnote:' || parsed.hostname !== 'media') return null;
    const segments = parsed.pathname.split('/').filter(Boolean).map(segment => decodeURIComponent(segment));
    if (segments.length !== 2) return null;
    const attrs = {
      noteId: segments[0],
      attachmentId: segments[1],
      startMs: Number(parsed.searchParams.get('start')),
      endMs: parsed.searchParams.has('end') ? Number(parsed.searchParams.get('end')) : null,
    };
    return validMediaLinkAttrs(attrs) ? attrs : null;
  } catch {
    return null;
  }
}

function normalizedBulletMarker(value) {
  return BULLET_LIST_MARKERS.includes(value) ? value : nodeAttrDefault('bullet_list', 'marker');
}

function bulletListAttrs(marker = nodeAttrDefault('bullet_list', 'marker')) {
  return {marker: normalizedBulletMarker(marker)};
}

function canCanonicalizeAdjacentLists(left, right) {
  if (!left || !right || left.type !== right.type) {
    return false;
  }
  if (left.type === schema.nodes.bullet_list) {
    return normalizedBulletMarker(left.attrs.marker) === normalizedBulletMarker(right.attrs.marker);
  }
  if (left.type === schema.nodes.task_list) {
    return true;
  }
  if (left.type !== schema.nodes.ordered_list) {
    return false;
  }
  return (left.attrs.style || nodeAttrDefault('ordered_list', 'style')) === (right.attrs.style || nodeAttrDefault('ordered_list', 'style'))
    && !right.attrs.restart;
}

function canonicalizeListTree(node) {
  if (node.isLeaf) {
    return node;
  }
  const children = [];
  let changed = false;
  node.forEach(child => {
    const normalized = canonicalizeListTree(child);
    changed = changed || normalized !== child;
    const previous = children.at(-1);
    if (!canCanonicalizeAdjacentLists(previous, normalized)) {
      children.push(normalized);
      return;
    }
    changed = true;
    children[children.length - 1] = previous.type.create(
      previous.attrs,
      [...previous.content.content, ...normalized.content.content],
      previous.marks,
    );
  });
  return changed ? node.copy(Fragment.fromArray(children)) : node;
}



function parseEnvelope(serialized) {
  if (typeof serialized !== 'string' || serialized.trim() === '') {
    throw new Error('Rustnote requires a canonical ProseMirror editor state.');
  }
  const parsed = validateSerializedEditorState(serialized);
  return canonicalizeListTree(schema.nodeFromJSON(parsed.document));
}

function envelopeForDoc(doc) {
  return {
    schema_version: ENVELOPE_SPEC.schema_version,
    editor_engine: ENVELOPE_SPEC.editor_engine,
    document_schema: ENVELOPE_SPEC.document_schema,
    document_schema_version: ENVELOPE_SPEC.document_schema_version,
    document: doc.toJSON(),
  };
}
export {MEDIA_ANNOTATION_LABEL_MAX, MEDIA_ANNOTATION_LIMIT, MEDIA_ANNOTATION_NOTE_MAX, MEDIA_ANNOTATION_TAG_MAX, MEDIA_DEEP_LINK_STORAGE_KEY, MEDIA_RESUME_PERSIST_INTERVAL_MS, bulletListAttrs, defaultFontSizeForBlockStyle, envelopeForDoc, mediaDeepLinkUri, normalizeSafeHref, normalizedBulletMarker, normalizedFontSize, normalizedLineSpacing, normalizedTextblockIndent, normalizedMediaAnnotation, normalizedMediaAnnotations, parseEnvelope, parseMediaDeepLinkUri, pruneMediaResumeStorage, readMediaResumePosition, safeHref, schema, validAttachmentId, validMediaLinkAttrs, validNoteId, writeMediaResumePosition};

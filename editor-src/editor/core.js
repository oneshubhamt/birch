import {attachmentImportPlugin, mediaTimeLabelStatic} from "../attachments/import.js";
import {AUTO_INDENT_SUPPRESSION_META, DEFAULT_AUTO_FIRST_LINE_INDENT, PARAGRAPH_FIRST_LINE_INDENT_MAX, findHighlightKey, FONT_SIZE_MAX, FONT_SIZE_MIN, linkPasteKey, LIST_CONTEXT_ID, MATRIX_DIMENSION_MAX, placeholderKey, preservedSelectionKey, SCIENTIFIC_CLIPBOARD_MIME, SCIENTIFIC_CLIPBOARD_VERSION, SCIENTIFIC_GUIDED_ROW_LIMIT, SCIENTIFIC_NODE_TYPES, SCIENTIFIC_NOTATION_BY_LATEX_COMMAND, SCIENTIFIC_NOTATION_BY_UNICODE, SCIENTIFIC_NOTATION_REGISTRY, SCIENTIFIC_RENDER_EXPANDED_SOURCE_LIMIT, SCIENTIFIC_RENDER_FALLBACK_BATCH, SCIENTIFIC_RENDER_OPTIONS, SCIENTIFIC_RENDER_ROOT_MARGIN, SCIENTIFIC_RENDER_SOURCE_LIMIT, SCIENTIFIC_RENDER_VISIBLE_BATCH, SCIENTIFIC_TEMPLATE_GROUPS, scientificSelectionKey, TABLE_DEFAULT_COLUMN_WIDTH, TABLE_MIN_COLUMN_WIDTH, tableBoundaryKey, TEXT_COLOR_LABELS} from "../config.js";
import {bulletListAttrs, envelopeForDoc, normalizedBulletMarker, normalizedLineSpacing, normalizedTextblockIndent, normalizeSafeHref, parseMediaDeepLinkUri, safeHref, schema} from "../document/schema.js";
import {activeEditorController} from "../runtime/state.js";
import katex from "../vendor/katex-0.16.27.mjs";
import {mhchemParser} from "../vendor/mhchem-parser-4.2.1.mjs";
import {baseKeymap, chainCommands, exitCode, splitBlock, toggleMark} from "prosemirror-commands";
import {dropCursor} from "prosemirror-dropcursor";
import {gapCursor, GapCursor} from "prosemirror-gapcursor";
import {closeHistory, history, redo, redoDepth, undo, undoDepth} from "prosemirror-history";
import {ellipsis, emDash, InputRule, inputRules, smartQuotes, textblockTypeInputRule, wrappingInputRule} from "prosemirror-inputrules";
import {keymap} from "prosemirror-keymap";
import {Fragment, Slice} from "prosemirror-model";
import {editorClipboardProvenancePlugin, plainTextPastePlugin, requestPlainTextPaste} from "./clipboard.js";
import {atomicBoundaryKeymap, editorBoundaryKeymap, emDashSequenceKeymap, lineStartEmDashInputRule} from "./interactions.js";
import {adjacentOrderedContext, commonSelectedListContext, liftSelectedListItems, listContextAtResolved, listKeymap, orderedListAttrs, selectedListContexts, selectedListType, sinkSelectedListItems} from "./list_system.js";
import {NodeSelection, Plugin, PluginKey, TextSelection} from "prosemirror-state";
import {cellAround, CellSelection, columnResizing, findTable, goToNextCell, tableEditing, TableMap} from "prosemirror-tables";
import {Mapping} from "prosemirror-transform";
import {Decoration, DecorationSet} from "prosemirror-view";





function isScientificNode(node) {
  return Boolean(node && SCIENTIFIC_NODE_TYPES.includes(node.type.name));
}

function scientificNodeText(type, source = '') {
  const text = String(source || '');
  return type.create(null, text === '' ? null : schema.text(text));
}

function scientificTemplateCaretOffset(source) {
  const text = String(source || '');
  const emptyGroup = text.indexOf('{}');
  if (emptyGroup >= 0) return emptyGroup + 1;
  const lineBreak = text.indexOf('\n');
  if (lineBreak >= 0) return lineBreak + 1;
  for (const command of ['\\ce{', '\\pu{']) {
    const commandStart = text.indexOf(command);
    if (commandStart >= 0) return commandStart + command.length;
  }
  const rightDelimiter = text.indexOf('\\right');
  if (rightDelimiter > 0) return Math.max(0, rightDelimiter - 1);
  return text.length;
}

function findBalancedCommandEnd(source, openBraceIndex) {
  let depth = 0;
  for (let index = openBraceIndex; index < source.length; index += 1) {
    const character = source[index];
    if (character === '\\') {
      index += 1;
      continue;
    }
    if (character === '{') depth += 1;
    if (character === '}') {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function expandMhchemSource(source) {
  const value = String(source || '');
  let output = '';
  let cursor = 0;
  while (cursor < value.length) {
    const ce = value.indexOf('\\ce{', cursor);
    const pu = value.indexOf('\\pu{', cursor);
    const start = ce < 0 ? pu : pu < 0 ? ce : Math.min(ce, pu);
    if (start < 0) {
      output += value.slice(cursor);
      break;
    }
    output += value.slice(cursor, start);
    const kind = start === ce ? 'ce' : 'pu';
    const brace = start + kind.length + 1;
    const end = findBalancedCommandEnd(value, brace);
    if (end < 0) {
      output += value.slice(start);
      break;
    }
    const body = value.slice(brace + 1, end);
    try {
      output += mhchemParser.toTex(body, kind);
    } catch {
      output += value.slice(start, end + 1);
    }
    cursor = end + 1;
  }
  return output;
}

function scientificSourceTooLarge(source) {
  return String(source || '').length > SCIENTIFIC_RENDER_SOURCE_LIMIT;
}

function scientificExpandedSourceTooLarge(source) {
  return String(source || '').length > SCIENTIFIC_RENDER_EXPANDED_SOURCE_LIMIT;
}

function showScientificRenderLimit(target, displayMode, detail) {
  target.dataset.mathTooLarge = 'true';
  target.textContent = displayMode
    ? 'Display equation is too large to render'
    : 'Equation is too large to render';
  target.title = detail || 'The scientific source is preserved and can still be edited, copied, or removed.';
}

function renderScientificSource(target, source, displayMode) {
  target.replaceChildren();
  target.removeAttribute('title');
  const raw = String(source || '');
  const empty = raw.trim() === '';
  const tooLarge = scientificSourceTooLarge(raw);
  target.dataset.mathEmpty = empty ? 'true' : 'false';
  target.dataset.mathTooLarge = tooLarge ? 'true' : 'false';
  target.dataset.mathError = 'false';
  if (empty) {
    target.textContent = displayMode ? 'Display equation' : 'Equation';
    return;
  }
  if (tooLarge) {
    showScientificRenderLimit(target, displayMode);
    return;
  }
  try {
    const expanded = expandMhchemSource(raw);
    if (scientificExpandedSourceTooLarge(expanded)) {
      showScientificRenderLimit(
        target,
        displayMode,
        'The chemistry expansion is too large to render safely. The original scientific source is preserved and can still be edited, copied, or removed.',
      );
      return;
    }
    katex.render(expanded, target, {
      ...SCIENTIFIC_RENDER_OPTIONS,
      displayMode,
    });
    target.dataset.mathError = target.querySelector('.katex-error') ? 'true' : 'false';
  } catch (error) {
    target.textContent = raw;
    target.title = error instanceof Error ? error.message : 'Invalid scientific notation';
    target.dataset.mathError = 'true';
  }
}

function parseScientificComposerSource(rawValue, fallbackDisplayMode = false) {
  const raw = String(rawValue || '').trim();
  const displayDollar = raw.match(/^\$\$\s*([\s\S]*?)\s*\$\$$/);
  if (displayDollar) return {source: displayDollar[1].trim(), displayMode: true};
  const displayBracket = raw.match(/^\\\[\s*([\s\S]*?)\s*\\\]$/);
  if (displayBracket) return {source: displayBracket[1].trim(), displayMode: true};
  const inlineDollar = raw.match(/^\$([^\n]*?)\$$/);
  if (inlineDollar) return {source: inlineDollar[1].trim(), displayMode: false};
  const inlineParen = raw.match(/^\\\(\s*([\s\S]*?)\s*\\\)$/);
  if (inlineParen) return {source: inlineParen[1].trim(), displayMode: false};
  return {source: raw, displayMode: Boolean(fallbackDisplayMode)};
}

function insertIntoTextarea(textarea, source) {
  const start = Number.isFinite(textarea.selectionStart) ? textarea.selectionStart : textarea.value.length;
  const end = Number.isFinite(textarea.selectionEnd) ? textarea.selectionEnd : start;
  const text = String(source || '');
  textarea.value = `${textarea.value.slice(0, start)}${text}${textarea.value.slice(end)}`;
  const caret = start + scientificTemplateCaretOffset(text);
  textarea.setSelectionRange(caret, caret);
  textarea.dispatchEvent(new Event('input', {bubbles: true}));
  textarea.focus({preventScroll: true});
}

const GUIDED_STRUCTURE_LABELS = Object.freeze({
  expression: 'Expression',
  scripts: 'Scripts',
  fraction: 'Fraction',
  root: 'Root',
  integral: 'Integral',
  summation: 'Summation',
  product: 'Product',
  matrix: 'Matrix',
  cases: 'Cases',
  aligned: 'Aligned equations',
  chemistry: 'Chemical equation',
});
const GUIDED_STRUCTURE_ALIASES = Object.freeze({
  superscript: Object.freeze(['scripts', 'superscript']),
  subscript: Object.freeze(['scripts', 'subscript']),
  scripts: Object.freeze(['scripts', 'scripts']),
  determinant: Object.freeze(['matrix', 'determinant']),
});

function resolveGuidedStructure(requestedKind, fallbackVariant = '') {
  return GUIDED_STRUCTURE_ALIASES[requestedKind] || [requestedKind, fallbackVariant];
}
const MATRIX_ENVIRONMENTS = Object.freeze([
  ['bmatrix', 'Square'],
  ['pmatrix', 'Round'],
  ['Bmatrix', 'Curly'],
  ['vmatrix', 'Determinant'],
  ['Vmatrix', 'Double bars'],
  ['matrix', 'None'],
]);
const SCIENTIFIC_INTEGRAL_OPERATORS = Object.freeze([
  ['\\int', 'Single'],
  ['\\iint', 'Double'],
  ['\\iiint', 'Triple'],
  ['\\oint', 'Contour'],
]);
const SCIENTIFIC_ALIGNED_RELATIONS = Object.freeze([
  ...SCIENTIFIC_NOTATION_REGISTRY
    .filter(entry => entry.aligned)
    .sort((left, right) => left.alignedOrder - right.alignedOrder)
    .map(entry => [entry.latex, entry.aligned]),
  ['<', '<'],
  ['>', '>'],
]);
function scientificAlignedRelationAliases() {
  const aliases = {};
  for (const entry of SCIENTIFIC_NOTATION_REGISTRY) {
    if (!entry.aligned) continue;
    for (const alias of entry.latexAliases || []) aliases[alias] = entry.latex;
  }
  return Object.freeze(aliases);
}
const SCIENTIFIC_ALIGNED_RELATION_ALIASES = scientificAlignedRelationAliases();

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function normalizedAlignedRelation(value) {
  const source = String(value || '=');
  const normalized = SCIENTIFIC_ALIGNED_RELATION_ALIASES[source] || source;
  return SCIENTIFIC_ALIGNED_RELATIONS.some(([candidate]) => candidate === normalized)
    ? normalized
    : '=';
}

const SCIENTIFIC_ALIGNED_RELATION_PATTERN = new RegExp(
  `^(${[
    ...SCIENTIFIC_ALIGNED_RELATIONS.map(([value]) => value),
    ...Object.keys(SCIENTIFIC_ALIGNED_RELATION_ALIASES),
  ].sort((left, right) => right.length - left.length).map(escapeRegExp).join('|')})\\s*`,
);

function cloneGuidedData(value) {
  return JSON.parse(JSON.stringify(value));
}

function defaultGuidedScientificData(kind, variant = '') {
  switch (kind) {
    case 'scripts':
      return {
        base: 'x',
        leftSuperscript: variant === 'scripts' ? 'a' : '',
        leftSubscript: variant === 'scripts' ? 'b' : '',
        superscript: variant === 'subscript' ? '' : (variant === 'superscript' ? '2' : variant === 'scripts' ? 'c' : ''),
        subscript: variant === 'subscript' ? 'i' : (variant === 'scripts' ? 'd' : ''),
      };
    case 'fraction':
      return {numerator: '', denominator: ''};
    case 'root':
      return {radicand: '', index: ''};
    case 'integral':
      return {operator: '\\int', lower: '', upper: '', integrand: '', variable: 'x'};
    case 'summation':
    case 'product':
      return {lower: '', upper: '', expression: ''};
    case 'matrix':
      return {
        rows: 2,
        columns: 2,
        environment: variant === 'determinant' ? 'vmatrix' : 'bmatrix',
        cells: [['', ''], ['', '']],
      };
    case 'cases':
      return {rows: [{expression: '', condition: ''}, {expression: '', condition: ''}]};
    case 'aligned':
      return {rows: [
        {left: '', relation: '=', right: ''},
        {left: '', relation: '=', right: ''},
      ]};
    case 'chemistry':
      return {value: '', command: 'ce'};
    case 'expression':
    default:
      return {expression: ''};
  }
}

function ensureMatrixDimensions(data, rows, columns) {
  const safeRows = clamp(Number(rows) || 1, 1, MATRIX_DIMENSION_MAX);
  const safeColumns = clamp(Number(columns) || 1, 1, MATRIX_DIMENSION_MAX);
  const prior = Array.isArray(data?.cells) ? data.cells : [];
  const cells = Array.from({length: safeRows}, (_, row) => (
    Array.from({length: safeColumns}, (_, column) => String(prior[row]?.[column] || ''))
  ));
  return {...data, rows: safeRows, columns: safeColumns, cells};
}

function guidedScientificSource(kind, data) {
  const value = data || defaultGuidedScientificData(kind);
  switch (kind) {
    case 'scripts': {
      const base = String(value.base || '');
      const left = `${value.leftSuperscript ? `^{${value.leftSuperscript}}` : ''}${value.leftSubscript ? `_{${value.leftSubscript}}` : ''}`;
      const right = `${value.superscript ? `^{${value.superscript}}` : ''}${value.subscript ? `_{${value.subscript}}` : ''}`;
      return `${left ? `{}` + left : ''}${base}${right}`;
    }
    case 'fraction':
      return `\\frac{${value.numerator || ''}}{${value.denominator || ''}}`;
    case 'root':
      return value.index
        ? `\\sqrt[${value.index}]{${value.radicand || ''}}`
        : `\\sqrt{${value.radicand || ''}}`;
    case 'integral': {
      const operator = SCIENTIFIC_INTEGRAL_OPERATORS.some(([candidate]) => candidate === value.operator)
        ? value.operator
        : '\\int';
      const lower = value.lower ? `_{${value.lower}}` : '';
      const upper = value.upper ? `^{${value.upper}}` : '';
      const differential = value.variable ? `\\, d${value.variable}` : '';
      return `${operator}${lower}${upper} ${value.integrand || ''}${differential}`.trim();
    }
    case 'summation':
    case 'product': {
      const command = kind === 'product' ? '\\prod' : '\\sum';
      const lower = value.lower ? `_{${value.lower}}` : '';
      const upper = value.upper ? `^{${value.upper}}` : '';
      return `${command}${lower}${upper} ${value.expression || ''}`.trim();
    }
    case 'matrix': {
      const matrix = ensureMatrixDimensions(value, value.rows, value.columns);
      const environment = MATRIX_ENVIRONMENTS.some(([name]) => name === matrix.environment)
        ? matrix.environment
        : 'bmatrix';
      const rows = matrix.cells.map(row => row.map(cell => String(cell || '')).join(' & '));
      return `\\begin{${environment}}\n${rows.join(' \\\\\n')}\n\\end{${environment}}`;
    }
    case 'cases': {
      const rows = (value.rows || []).map(row => `${row.expression || ''} & ${row.condition || ''}`);
      return `\\begin{cases}\n${rows.join(' \\\\\n')}\n\\end{cases}`;
    }
    case 'aligned': {
      const rows = (value.rows || []).map(row => {
        if (typeof row === 'string') return row;
        const left = String(row?.left || '');
        const relation = normalizedAlignedRelation(row?.relation);
        const right = String(row?.right || '');
        return `${left} &${relation} ${right}`.trimEnd();
      });
      return `\\begin{aligned}\n${rows.join(' \\\\\n')}\n\\end{aligned}`;
    }
    case 'chemistry': {
      const raw = normalizeChemistryComposerInput(value.value || '').trim();
      if (raw === '') return '';
      return value.command === 'pu' ? `\\pu{${raw}}` : `\\ce{${raw}}`;
    }
    case 'expression':
    default:
      return String(value.expression || '');
  }
}

function splitLatexTopLevel(source, mode) {
  const output = [];
  let current = '';
  let braceDepth = 0;
  let environmentDepth = 0;
  const text = String(source || '');
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    const escaped = index > 0 && text[index - 1] === '\\';
    if (!escaped && character === '{') braceDepth += 1;
    if (!escaped && character === '}') braceDepth = Math.max(0, braceDepth - 1);
    if (braceDepth === 0 && text.startsWith('\\begin{', index)) environmentDepth += 1;
    if (braceDepth === 0 && text.startsWith('\\end{', index)) environmentDepth = Math.max(0, environmentDepth - 1);
    const topLevel = braceDepth === 0 && environmentDepth === 0;
    const rowBreak = mode === 'rows' && topLevel && character === '\\' && text[index + 1] === '\\';
    const cellBreak = mode === 'cells' && topLevel && character === '&';
    if (rowBreak || cellBreak) {
      output.push(current.trim());
      current = '';
      if (rowBreak) index += 1;
      continue;
    }
    current += character;
  }
  output.push(current.trim());
  return output;
}

function containsNestedLatexEnvironment(source) {
  return /\\begin\s*\{/.test(String(source || ''));
}

function readLatexGroup(source, start, opener = '{', closer = '}') {
  const text = String(source || '');
  if (text[start] !== opener) return null;
  let depth = 0;
  for (let index = start; index < text.length; index += 1) {
    const character = text[index];
    const escaped = index > 0 && text[index - 1] === '\\';
    if (!escaped && character === opener) depth += 1;
    if (!escaped && character === closer) {
      depth -= 1;
      if (depth === 0) return {value: text.slice(start + 1, index), end: index + 1};
    }
  }
  return null;
}

function consumeScriptPair(source, start) {
  const output = {superscript: '', subscript: '', end: start};
  let cursor = start;
  for (let count = 0; count < 2; count += 1) {
    const marker = source[cursor];
    if (marker !== '^' && marker !== '_') break;
    const group = readLatexGroup(source, cursor + 1);
    if (!group) break;
    if (marker === '^') output.superscript = group.value;
    else output.subscript = group.value;
    cursor = group.end;
  }
  output.end = cursor;
  return output;
}

function parseGuidedScriptsSource(source) {
  const raw = String(source || '').trim();
  let cursor = 0;
  let leftSuperscript = '';
  let leftSubscript = '';
  if (raw.startsWith('{}')) {
    const left = consumeScriptPair(raw, 2);
    leftSuperscript = left.superscript;
    leftSubscript = left.subscript;
    cursor = left.end;
  }
  let baseEnd = cursor;
  let depth = 0;
  while (baseEnd < raw.length) {
    const character = raw[baseEnd];
    const escaped = baseEnd > 0 && raw[baseEnd - 1] === '\\';
    if (!escaped && character === '{') depth += 1;
    if (!escaped && character === '}') depth = Math.max(0, depth - 1);
    if (depth === 0 && (character === '^' || character === '_')) break;
    baseEnd += 1;
  }
  const right = consumeScriptPair(raw, baseEnd);
  if (right.end !== raw.length || baseEnd === cursor) return null;
  return {
    base: raw.slice(cursor, baseEnd),
    leftSuperscript,
    leftSubscript,
    superscript: right.superscript,
    subscript: right.subscript,
  };
}

function insertGuidedValueAtPath(data, path, start, end, source) {
  if (!Array.isArray(path) || path.length === 0) return false;
  let target = data;
  for (const segment of path.slice(0, -1)) {
    if (target == null || !(segment in target)) return false;
    target = target[segment];
  }
  const key = path[path.length - 1];
  if (target == null || !(key in target)) return false;
  const current = String(target[key] || '');
  const safeStart = clamp(Number.isFinite(start) ? start : current.length, 0, current.length);
  const safeEnd = clamp(Number.isFinite(end) ? end : safeStart, safeStart, current.length);
  target[key] = `${current.slice(0, safeStart)}${source}${current.slice(safeEnd)}`;
  return true;
}

function parseGuidedScientificSource(source, chemistry = false) {
  const raw = String(source || '').trim();
  if (chemistry) {
    if (raw === '') {
      return {
        kind: 'chemistry',
        data: defaultGuidedScientificData('chemistry'),
        recognised: true,
      };
    }
    const command = raw.startsWith('\\ce{') ? 'ce' : raw.startsWith('\\pu{') ? 'pu' : null;
    const group = command ? readLatexGroup(raw, command.length + 1) : null;
    const recognised = Boolean(group && group.end === raw.length);
    return {
      kind: 'chemistry',
      data: {value: recognised ? group.value : raw, command: command || 'ce'},
      recognised,
    };
  }
  const matrix = raw.match(/^\\begin\{(bmatrix|pmatrix|Bmatrix|vmatrix|Vmatrix|matrix)\}\s*([\s\S]*?)\s*\\end\{\1\}$/);
  if (matrix) {
    if (containsNestedLatexEnvironment(matrix[2])) {
      return {kind: 'expression', data: {expression: raw}, recognised: false};
    }
    const rows = splitLatexTopLevel(matrix[2], 'rows').filter((row, index, all) => row !== '' || all.length === 1);
    const cells = rows.map(row => splitLatexTopLevel(row, 'cells'));
    const columns = Math.max(1, ...cells.map(row => row.length));
    return {
      kind: 'matrix',
      data: ensureMatrixDimensions({rows: Math.max(1, cells.length), columns, environment: matrix[1], cells}, cells.length, columns),
      recognised: true,
    };
  }
  const cases = raw.match(/^\\begin\{cases\}\s*([\s\S]*?)\s*\\end\{cases\}$/);
  if (cases) {
    if (containsNestedLatexEnvironment(cases[1])) {
      return {kind: 'expression', data: {expression: raw}, recognised: false};
    }
    const rows = splitLatexTopLevel(cases[1], 'rows').map(row => {
      const cells = splitLatexTopLevel(row, 'cells');
      return {expression: cells[0] || '', condition: cells.slice(1).join(' & ') || ''};
    });
    return {kind: 'cases', data: {rows: rows.length ? rows : [{expression: '', condition: ''}]}, recognised: true};
  }
  const aligned = raw.match(/^\\begin\{aligned\}\s*([\s\S]*?)\s*\\end\{aligned\}$/);
  if (aligned) {
    if (containsNestedLatexEnvironment(aligned[1])) {
      return {kind: 'expression', data: {expression: raw}, recognised: false};
    }
    const rows = splitLatexTopLevel(aligned[1], 'rows').map(row => {
      const cells = splitLatexTopLevel(row, 'cells');
      const left = cells.shift() || '';
      const remainder = cells.join(' & ').trim();
      const relation = remainder.match(SCIENTIFIC_ALIGNED_RELATION_PATTERN);
      return {
        left,
        relation: normalizedAlignedRelation(relation?.[1]),
        right: relation ? remainder.slice(relation[0].length) : remainder,
      };
    });
    return {
      kind: 'aligned',
      data: {rows: rows.length ? rows : [{left: '', relation: '=', right: ''}]},
      recognised: true,
    };
  }
  const scripts = parseGuidedScriptsSource(raw);
  if (scripts && (scripts.leftSuperscript || scripts.leftSubscript || scripts.superscript || scripts.subscript)) {
    return {kind: 'scripts', data: scripts, recognised: true};
  }
  if (raw.startsWith('\\frac')) {
    const numerator = readLatexGroup(raw, 5);
    const denominator = numerator ? readLatexGroup(raw, numerator.end) : null;
    if (numerator && denominator && denominator.end === raw.length) {
      return {
        kind: 'fraction',
        data: {numerator: numerator.value, denominator: denominator.value},
        recognised: true,
      };
    }
  }
  if (raw.startsWith('\\sqrt')) {
    let cursor = 5;
    let index = '';
    if (raw[cursor] === '[') {
      const group = readLatexGroup(raw, cursor, '[', ']');
      if (group) {
        index = group.value;
        cursor = group.end;
      }
    }
    const radicand = readLatexGroup(raw, cursor);
    if (radicand && radicand.end === raw.length) {
      return {kind: 'root', data: {index, radicand: radicand.value}, recognised: true};
    }
  }
  const integral = raw.match(/^(\\(?:int|iint|iiint|oint))(?:_\{([^{}]*)\})?(?:\^\{([^{}]*)\})?\s*([\s\S]*?)(?:\\,\s*d([A-Za-z]+))?$/);
  if (integral) return {
    kind: 'integral',
    data: {
      operator: SCIENTIFIC_INTEGRAL_OPERATORS.some(([candidate]) => candidate === integral[1]) ? integral[1] : '\\int',
      lower: integral[2] || '',
      upper: integral[3] || '',
      integrand: (integral[4] || '').trim(),
      variable: integral[5] || '',
    },
    recognised: true,
  };
  const aggregate = raw.match(/^\\(sum|prod)(?:_\{([^{}]*)\})?(?:\^\{([^{}]*)\})?\s*([\s\S]*)$/);
  if (aggregate) return {kind: aggregate[1] === 'prod' ? 'product' : 'summation', data: {lower: aggregate[2] || '', upper: aggregate[3] || '', expression: (aggregate[4] || '').trim()}, recognised: true};
  return {kind: 'expression', data: {expression: raw}, recognised: true};
}

function normalizeGuidedScientificComparison(source) {
  return String(source || '')
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map(line => line.trimEnd())
    .join('\n')
    .trim();
}

function guidedScientificRowLimitExceeded(kind, data) {
  return (kind === 'cases' || kind === 'aligned')
    && Array.isArray(data?.rows)
    && data.rows.length > SCIENTIFIC_GUIDED_ROW_LIMIT;
}

function guidedScientificEligibility(source, chemistry = false) {
  const parsed = parseGuidedScientificSource(source, chemistry);
  if (!parsed.recognised) return {...parsed, lossless: false, guidedAvailable: false};
  const regenerated = guidedScientificSource(parsed.kind, parsed.data);
  const lossless = normalizeGuidedScientificComparison(source) === normalizeGuidedScientificComparison(regenerated);
  return {
    ...parsed,
    regenerated,
    lossless,
    guidedAvailable: lossless && !guidedScientificRowLimitExceeded(parsed.kind, parsed.data),
  };
}

function guidedScientificUnavailableMessage(eligibility) {
  if (eligibility?.lossless && guidedScientificRowLimitExceeded(eligibility.kind, eligibility.data)) {
    return `This ${GUIDED_STRUCTURE_LABELS[eligibility.kind]?.toLowerCase() || 'scientific structure'} has more than ${SCIENTIFIC_GUIDED_ROW_LIMIT} rows and is too large for Guided editing. Continue in Advanced source.`;
  }
  return 'This source cannot be represented safely in Guided mode. Continue in Advanced source.';
}

function scientificToolGroupDetails(group, onInsert, onGuided, menuSemantics = false) {
  const details = document.createElement('details');
  details.className = 'scientific-tool-group';
  const summary = document.createElement('summary');
  summary.textContent = group.label;
  if (menuSemantics) summary.setAttribute('role', 'menuitem');
  const grid = document.createElement('div');
  grid.className = 'scientific-tool-group-grid';
  grid.setAttribute('role', 'group');
  grid.setAttribute('aria-label', group.label);
  for (const [label, source, action] of group.items) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = label;
    if (menuSemantics) button.setAttribute('role', 'menuitem');
    if (source) {
      button.title = source;
      button.dataset.tooltipForce = 'true';
    }
    button.addEventListener('pointerdown', event => event.preventDefault());
    button.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      if (String(action || '').startsWith('guided:')) {
        onGuided(String(action).slice('guided:'.length));
      } else {
        onInsert(source || '');
      }
    });
    grid.appendChild(button);
  }
  details.append(summary, grid);
  return details;
}

function scientificToolGroupsPanel(groups, onInsert, onGuided, menuSemantics = false) {
  const panel = document.createElement('div');
  panel.className = 'scientific-tool-groups';
  for (const group of groups) {
    panel.appendChild(scientificToolGroupDetails(group, onInsert, onGuided, menuSemantics));
  }
  return panel;
}

const SUPERSCRIPT_CHARACTERS = Object.freeze({
  '⁰': '0', '¹': '1', '²': '2', '³': '3', '⁴': '4',
  '⁵': '5', '⁶': '6', '⁷': '7', '⁸': '8', '⁹': '9',
  '⁺': '+', '⁻': '-', '⁼': '=', '⁽': '(', '⁾': ')',
  'ⁿ': 'n', 'ⁱ': 'i',
});
const SUBSCRIPT_CHARACTERS = Object.freeze({
  '₀': '0', '₁': '1', '₂': '2', '₃': '3', '₄': '4',
  '₅': '5', '₆': '6', '₇': '7', '₈': '8', '₉': '9',
  '₊': '+', '₋': '-', '₌': '=', '₍': '(', '₎': ')',
});
const SCIENTIFIC_CHEMISTRY_UNICODE_CANDIDATES = Object.freeze(['⇌', '⇄']);
function scientificUnicodeCharacters(pasteMode = '') {
  const characters = [];
  for (const entry of SCIENTIFIC_NOTATION_REGISTRY) {
    if (pasteMode && entry.paste !== pasteMode) continue;
    characters.push(...(entry.unicode || []));
  }
  return characters;
}
function scientificCharacterPattern(characters) {
  const unique = [...new Set(characters)];
  const escaped = unique.map(character => character.replace(/[\\\]\-^]/g, '\\$&')).join('');
  return new RegExp(`[${escaped}]`, 'u');
}
const SCIENTIFIC_STRONG_UNICODE_CHARACTERS = Object.freeze([
  ...scientificUnicodeCharacters('strong'),
  ...Object.keys(SUPERSCRIPT_CHARACTERS),
  ...Object.keys(SUBSCRIPT_CHARACTERS),
  ...SCIENTIFIC_CHEMISTRY_UNICODE_CANDIDATES,
]);
const SCIENTIFIC_STRONG_UNICODE_PATTERN = scientificCharacterPattern(SCIENTIFIC_STRONG_UNICODE_CHARACTERS);
const SCIENTIFIC_PASTE_CANDIDATE_CHARACTERS = Object.freeze([
  '$', '=', '<', '>', '\\',
  ...scientificUnicodeCharacters(),
  ...Object.keys(SUPERSCRIPT_CHARACTERS),
  ...Object.keys(SUBSCRIPT_CHARACTERS),
  ...SCIENTIFIC_CHEMISTRY_UNICODE_CANDIDATES,
]);
const SCIENTIFIC_PASTE_CANDIDATE_SET = new Set(SCIENTIFIC_PASTE_CANDIDATE_CHARACTERS);
const SCIENTIFIC_PASTE_CANDIDATE_PATTERN = scientificCharacterPattern(SCIENTIFIC_PASTE_CANDIDATE_CHARACTERS);

function containsScientificCharacter(value, pattern) {
  return pattern.test(String(value || ''));
}
function scientificNotationLatexForUnicode(entry) {
  const latex = String((entry && entry.latex) || '');
  if (/^\\[A-Za-z]+$/.test(latex)) return `${latex} `;
  return latex;
}

function scientificNotationForUnicode(character) {
  return SCIENTIFIC_NOTATION_BY_UNICODE.get(character) || null;
}

function unicodeScientificToLatex(value) {
  const source = String(value || '');
  let output = '';
  for (let index = 0; index < source.length;) {
    const character = source[index];
    if (Object.prototype.hasOwnProperty.call(SUPERSCRIPT_CHARACTERS, character)) {
      let run = '';
      while (index < source.length && Object.prototype.hasOwnProperty.call(SUPERSCRIPT_CHARACTERS, source[index])) {
        run += SUPERSCRIPT_CHARACTERS[source[index]];
        index += 1;
      }
      output += `^{${run}}`;
      continue;
    }
    if (Object.prototype.hasOwnProperty.call(SUBSCRIPT_CHARACTERS, character)) {
      let run = '';
      while (index < source.length && Object.prototype.hasOwnProperty.call(SUBSCRIPT_CHARACTERS, source[index])) {
        run += SUBSCRIPT_CHARACTERS[source[index]];
        index += 1;
      }
      output += `_{${run}}`;
      continue;
    }
    const notation = scientificNotationForUnicode(character);
    output += notation ? scientificNotationLatexForUnicode(notation) : character;
    index += 1;
  }
  return output.trim();
}

function normalizeChemistryComposerInput(value) {
  return String(value || '')
    .replace(/[₀₁₂₃₄₅₆₇₈₉]/g, character => SUBSCRIPT_CHARACTERS[character] || character)
    .replaceAll('⇌', '<=>')
    .replaceAll('⇄', '<=>')
    .replaceAll('→', '->')
    .replaceAll('←', '<-');
}

function looksLikeChemicalComponent(value) {
  const source = normalizeChemistryComposerInput(value)
    .trim()
    .replace(/^\d+(?:\.\d+)?\s*/, '')
    .replace(/\[(?:[^\]\\]|\\.)*\]/g, '')
    .replace(/\((?:aq|s|l|g)\)$/i, '')
    .replace(/\^[0-9]*[+-]$/, '')
    .trim();
  if (source === 'e-' || source === 'e+') return true;
  if (source === '' || /\s/.test(source)) return false;
  return /^(?:(?:[A-Z][a-z]?|[A-Z])\d*|\([A-Za-z0-9]+\)\d*|\[[A-Za-z0-9]+\]\d*)+$/.test(source);
}

function looksLikeChemicalExpression(value) {
  const source = normalizeChemistryComposerInput(value).trim();
  if (
    source.length < 3
    || source.length > 180
    || /[.!?;]/.test(source)
    || /\b(?:plan|result|status|pending|name|ordinary|prose|therefore|because)\b/i.test(source)
  ) return false;
  const arrowMatch = source.match(/<=>|<->|->|<-/);
  if (!arrowMatch) return false;
  const sides = source.split(arrowMatch[0]);
  if (sides.length !== 2 || sides.some(side => side.trim() === '')) return false;
  return sides.every(side => {
    const components = side.split(/\s+\+\s+/).map(component => component.trim()).filter(Boolean);
    return components.length > 0 && components.every(looksLikeChemicalComponent);
  });
}

function looksLikeUnicodeScientificExpression(value) {
  const source = String(value || '').trim();
  if (
    source.length < 3
    || source.length > 220
    || source.includes('$')
    || /^[\d\s.,₹€£¥+-]+$/.test(source)
    || /[.!?;:]\s*(?:$|[A-Z])/u.test(source)
  ) return false;
  if (containsScientificCharacter(source, SCIENTIFIC_STRONG_UNICODE_PATTERN)) return true;
  if (!source.includes('=') || source.split(/\s+/).length > 14) return false;
  const sides = source.split('=');
  if (sides.length !== 2 || sides.some(side => side.trim() === '')) return false;
  const wordTokens = source.match(/[A-Za-zΑ-Ωα-ω]+/g) || [];
  if (wordTokens.length === 0 || wordTokens.some(token => token.length > 3)) return false;
  return sides.every(side => /^[\sA-Za-zΑ-Ωα-ω0-9()+\-*/^_.,]+$/.test(side));
}

function mathmlOperatorToLatex(value) {
  const notation = scientificNotationForUnicode(value);
  return notation ? scientificNotationLatexForUnicode(notation) : value;
}

function mathmlChildrenToLatex(element, separator = '') {
  return [...element.childNodes]
    .map(mathmlNodeToLatex)
    .filter(Boolean)
    .join(separator);
}

function mathmlNodeToLatex(node) {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent || '';
  if (!(node instanceof Element)) return '';
  const name = node.localName.toLowerCase();
  const children = () => [...node.children];
  if (name === 'annotation' || name === 'annotation-xml') return '';
  if (name === 'semantics') {
    const presentation = children().find(child => !['annotation', 'annotation-xml'].includes(child.localName.toLowerCase()));
    return presentation ? mathmlNodeToLatex(presentation) : '';
  }
  if (['math', 'mrow', 'mstyle', 'mpadded', 'mphantom'].includes(name)) return mathmlChildrenToLatex(node);
  if (name === 'mi') {
    const value = node.textContent || '';
    const notation = scientificNotationForUnicode(value.trim());
    return notation && notation.palette && notation.palette[0] === 'greek'
      ? scientificNotationLatexForUnicode(notation)
      : value;
  }
  if (['mn', 'mtext'].includes(name)) return node.textContent || '';
  if (name === 'mo') return mathmlOperatorToLatex((node.textContent || '').trim());
  const parts = children().map(mathmlNodeToLatex);
  if (name === 'msup' && parts.length >= 2) return `{${parts[0]}}^{${parts[1]}}`;
  if (name === 'msub' && parts.length >= 2) return `{${parts[0]}}_{${parts[1]}}`;
  if (name === 'msubsup' && parts.length >= 3) return `{${parts[0]}}_{${parts[1]}}^{${parts[2]}}`;
  if (name === 'mfrac' && parts.length >= 2) return `\\frac{${parts[0]}}{${parts[1]}}`;
  if (name === 'msqrt') return `\\sqrt{${parts.join('')}}`;
  if (name === 'mroot' && parts.length >= 2) return `\\sqrt[${parts[1]}]{${parts[0]}}`;
  if (name === 'mover' && parts.length >= 2) return `\\overset{${parts[1]}}{${parts[0]}}`;
  if (name === 'munder' && parts.length >= 2) return `\\underset{${parts[1]}}{${parts[0]}}`;
  if (name === 'munderover' && parts.length >= 3) return `{${parts[0]}}_{${parts[1]}}^{${parts[2]}}`;
  if (name === 'mfenced') {
    const open = node.getAttribute('open') || '(';
    const close = node.getAttribute('close') || ')';
    const separators = node.getAttribute('separators') || ',';
    return `\\left${open}${parts.join(separators[0] || ',')}\\right${close}`;
  }
  if (name === 'mtable') {
    const rows = children().filter(child => child.localName.toLowerCase() === 'mtr');
    const body = rows.map(row => {
      const cells = [...row.children].filter(child => child.localName.toLowerCase() === 'mtd');
      return cells.map(mathmlNodeToLatex).join(' & ');
    }).join(' \\\\\n');
    return body === '' ? '' : `\\begin{matrix}\n${body}\n\\end{matrix}`;
  }
  if (name === 'mtr' || name === 'mtd') return parts.join(name === 'mtr' ? ' & ' : '');
  return parts.join('');
}

function structuredScientificClipboardPayload(event) {
  const custom = event.clipboardData?.getData(SCIENTIFIC_CLIPBOARD_MIME) || '';
  if (custom.trim() !== '') {
    try {
      const payload = JSON.parse(custom);
      if (
        payload?.version === SCIENTIFIC_CLIPBOARD_VERSION
        && typeof payload.source === 'string'
        && payload.source.trim() !== ''
        && (payload.type === 'inline' || payload.type === 'display')
      ) {
        return {source: payload.source, displayMode: payload.type === 'display'};
      }
    } catch {
      // Continue through interoperable HTML and MathML fallbacks.
    }
  }
  const html = event.clipboardData?.getData('text/html') || '';
  if (html.trim() === '') return null;
  let documentFragment;
  try {
    documentFragment = new DOMParser().parseFromString(html, 'text/html');
  } catch {
    return null;
  }
  const annotation = documentFragment.querySelector(
    'annotation[encoding="application/x-tex"], annotation[encoding="application/x-latex"], annotation[encoding="text/latex"]',
  );
  if (annotation?.textContent?.trim()) {
    const math = annotation.closest('math');
    const parsed = parseScientificComposerSource(
      annotation.textContent,
      math?.getAttribute('display') === 'block' || Boolean(annotation.closest('.katex-display')),
    );
    return parsed.source ? parsed : null;
  }
  const script = documentFragment.querySelector('script[type^="math/tex"]');
  if (script?.textContent?.trim()) {
    const parsed = parseScientificComposerSource(
      script.textContent,
      /mode\s*=\s*display/i.test(script.getAttribute('type') || ''),
    );
    return parsed.source ? parsed : null;
  }
  const dataSource = documentFragment.querySelector('[data-tex], [data-latex], [data-math-tex]');
  if (dataSource) {
    const raw = dataSource.getAttribute('data-tex')
      || dataSource.getAttribute('data-latex')
      || dataSource.getAttribute('data-math-tex')
      || '';
    const displayMode = dataSource.getAttribute('data-rustnote-display') === 'true'
      || Boolean(dataSource.closest('.katex-display'));
    const parsed = parseScientificComposerSource(raw, displayMode);
    return parsed.source ? parsed : null;
  }
  const math = documentFragment.querySelector('math');
  if (math) {
    const altText = math.getAttribute('alttext') || '';
    const rawSource = altText.trim() || mathmlNodeToLatex(math).trim();
    const parsed = parseScientificComposerSource(rawSource, math.getAttribute('display') === 'block');
    if (parsed.source) return parsed;
  }
  return null;
}

function scientificNodeMarkdown(node) {
  const source = node.textContent;
  return node.type === schema.nodes.math_inline
    ? `$${source}$`
    : `$$\n${source}\n$$`;
}

function normalizedTableSpan(value) {
  const numeric = Number(value);
  return Number.isSafeInteger(numeric) ? Math.max(1, numeric) : 1;
}

function tableMatrix(node, serializeCell) {
  const rows = [];
  const occupied = [];
  let rowIndex = 0;
  node.forEach(row => {
    const reserved = occupied[rowIndex] || [];
    const cells = [];
    for (let column = 0; column < reserved.length; column += 1) {
      if (reserved[column]) cells[column] = '';
    }
    let column = 0;
    row.forEach(cell => {
      const colspan = normalizedTableSpan(cell.attrs.colspan);
      const rowspan = normalizedTableSpan(cell.attrs.rowspan);
      while (true) {
        let overlapsReservedColumn = false;
        for (let offset = 0; offset < colspan; offset += 1) {
          if (reserved[column + offset]) {
            overlapsReservedColumn = true;
            break;
          }
        }
        if (!overlapsReservedColumn) break;
        column += 1;
      }
      cells[column] = serializeCell(cell);
      for (let offset = 1; offset < colspan; offset += 1) {
        cells[column + offset] = '';
      }
      for (let rowOffset = 1; rowOffset < rowspan; rowOffset += 1) {
        const futureRow = rowIndex + rowOffset;
        if (!occupied[futureRow]) occupied[futureRow] = [];
        for (let columnOffset = 0; columnOffset < colspan; columnOffset += 1) {
          occupied[futureRow][column + columnOffset] = true;
        }
      }
      column += colspan;
    });
    rows.push(cells);
    rowIndex += 1;
  });
  const width = rows.reduce((maximum, row) => Math.max(maximum, row.length), 0);
  rows.forEach(row => {
    for (let column = 0; column < width; column += 1) {
      if (row[column] === undefined) row[column] = '';
    }
  });
  return {rows, width};
}

function canonicalEditorState(controller) {
  if (controller.cachedCanonicalRevision !== controller.documentRevision) {
    controller.cachedCanonicalState = JSON.stringify(envelopeForDoc(controller.view.state.doc));
    controller.cachedCanonicalRevision = controller.documentRevision;
  }
  return controller.cachedCanonicalState;
}

function editorEnvelope(controller) {
  return {
    generation: controller.generation,
    revision: controller.documentRevision,
    editor_state: canonicalEditorState(controller),
  };
}

function setButtonState(action, active, disabled = false) {
  document.querySelectorAll(`[data-live-editor-format="${action}"]`).forEach(button => {
    if (!(button instanceof HTMLElement)) {
      return;
    }
    button.classList.toggle('active', active);
    if (button.hasAttribute('aria-pressed')) {
      button.setAttribute('aria-pressed', active ? 'true' : 'false');
    }
    if (button instanceof HTMLButtonElement) {
      button.disabled = disabled;
    }
  });
}

function blockStyleLabel(style, listType = null) {
  const labels = {
    paragraph: 'Normal text',
    h1: 'Heading 1',
    h2: 'Heading 2',
    h3: 'Heading 3',
    quote: 'Quote',
    code: 'Code block',
    mixed: 'Mixed',
  };
  const listLabels = {
    bullet_list: 'Bullet list',
    ordered_list: 'Numbered list',
    task_list: 'Checklist',
    mixed: 'Mixed lists',
  };
  return style === 'list'
    ? (listLabels[listType] || 'List')
    : (labels[style] || labels.paragraph);
}

function setBlockStyleState(style, listType, disabled) {
  document.querySelectorAll('[data-live-editor-block-style]').forEach(button => {
    if (!(button instanceof HTMLButtonElement)) {
      return;
    }
    button.disabled = disabled;
    button.dataset.blockStyle = style;
    const currentLabel = blockStyleLabel(style, listType);
    const visibleLabel = button.querySelector('.block-style-label');
    if (visibleLabel instanceof HTMLElement) {
      visibleLabel.textContent = currentLabel;
    }
    button.title = `Style: ${currentLabel}`;
    button.setAttribute('aria-label', `Style: ${currentLabel}`);
  });
  document.querySelectorAll('[data-live-editor-block-option]').forEach(option => {
    if (!(option instanceof HTMLElement)) {
      return;
    }
    const action = option.dataset.liveEditorBlockOption || '';
    const normalized = action.startsWith('heading') ? `h${action.slice(-1)}` : action;
    option.setAttribute('aria-checked', normalized === style ? 'true' : 'false');
  });
}

function markActive(state, markType) {
  const {from, to, empty, $from} = state.selection;
  if (empty) {
    return Boolean(markType.isInSet(state.storedMarks || $from.marks()));
  }
  return state.doc.rangeHasMark(from, to, markType);
}

function selectedLinkContext(state) {
  const {selection} = state;
  if (!selection.empty) {
    const hrefs = new Set();
    let hasUnlinkedText = false;
    state.doc.nodesBetween(selection.from, selection.to, node => {
      if (!node.isText) return true;
      const link = schema.marks.link.isInSet(node.marks);
      if (link && safeHref(link.attrs.href)) hrefs.add(link.attrs.href);
      else if ((node.text || '').trim() !== '') hasUnlinkedText = true;
      return false;
    });
    if (!hasUnlinkedText && hrefs.size === 1) {
      return {
        from: selection.from,
        to: selection.to,
        href: [...hrefs][0],
        label: state.doc.textBetween(selection.from, selection.to, ' '),
      };
    }
    return null;
  }

  const {$from} = selection;
  if (!$from.parent.inlineContent) return null;
  const children = [];
  $from.parent.forEach((node, offset) => children.push({node, offset}));
  const cursor = $from.parentOffset;
  const candidates = children
    .map((entry, index) => ({...entry, index}))
    .filter(({node, offset}) => offset <= cursor && cursor <= offset + node.nodeSize)
    .sort((left, right) => {
      const leftBefore = left.offset + left.node.nodeSize === cursor ? 0 : 1;
      const rightBefore = right.offset + right.node.nodeSize === cursor ? 0 : 1;
      return leftBefore - rightBefore;
    });
  const active = candidates.find(({node}) => {
    const link = schema.marks.link.isInSet(node.marks);
    return Boolean(link && safeHref(link.attrs.href));
  });
  if (!active) return null;
  const activeMark = schema.marks.link.isInSet(active.node.marks);
  const href = activeMark.attrs.href;
  let first = active.index;
  let last = active.index;
  const sharesHref = entry => schema.marks.link.isInSet(entry.node.marks)?.attrs.href === href;
  while (first > 0 && sharesHref(children[first - 1])) first -= 1;
  while (last + 1 < children.length && sharesHref(children[last + 1])) last += 1;
  const from = $from.start() + children[first].offset;
  const to = $from.start() + children[last].offset + children[last].node.nodeSize;
  return {from, to, href, label: state.doc.textBetween(from, to, ' ')};
}

function selectedMarkAttribute(state, markType, attribute, fallback) {
  const {selection} = state;
  if (selection.empty) {
    const mark = markType.isInSet(state.storedMarks || selection.$from.marks());
    return mark ? mark.attrs[attribute] : fallback;
  }
  const values = new Set();
  state.doc.nodesBetween(selection.from, selection.to, node => {
    if (!node.isText) {
      return true;
    }
    const mark = markType.isInSet(node.marks);
    values.add(mark ? mark.attrs[attribute] : fallback);
    return false;
  });
  if (values.size === 0) {
    return fallback;
  }
  return values.size === 1 ? [...values][0] : 'mixed';
}

function textblockPositionAtSelection(state) {
  const {$from} = state.selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    const node = $from.node(depth);
    if (node.isTextblock) {
      return {node, position: $from.before(depth)};
    }
  }
  return null;
}

function selectedTextblocks(state) {
  if (state.selection.empty) {
    const current = textblockPositionAtSelection(state);
    return current ? [current] : [];
  }
  const blocks = new Map();
  state.doc.nodesBetween(state.selection.from, state.selection.to, (node, position) => {
    if (node.isTextblock) {
      blocks.set(position, {node, position});
      return false;
    }
    return true;
  });
  if (blocks.size === 0) {
    const current = textblockPositionAtSelection(state);
    return current ? [current] : [];
  }
  return [...blocks.values()];
}

const TEXTBLOCK_LAYOUT_ATTRIBUTES = Object.freeze([
  'lineSpacing',
  'textAlignment',
  'leftIndent',
  'rightIndent',
  'firstLineIndent',
  'dropCap',
]);

function textblockLayoutAttrs(node, targetType = node?.type) {
  const targetAttrs = targetType?.spec?.attrs || {};
  const sourceAttrs = node?.attrs || {};
  const attrs = {};
  for (const attribute of TEXTBLOCK_LAYOUT_ATTRIBUTES) {
    if (!Object.prototype.hasOwnProperty.call(targetAttrs, attribute)) continue;
    attrs[attribute] = Object.prototype.hasOwnProperty.call(sourceAttrs, attribute)
      ? sourceAttrs[attribute]
      : targetAttrs[attribute].default;
  }
  return attrs;
}

function positionHasAncestor(state, position, names) {
  const resolved = state.doc.resolve(Math.max(0, Math.min(state.doc.content.size, position + 1)));
  for (let depth = resolved.depth; depth > 0; depth -= 1) {
    if (names.includes(resolved.node(depth).type.name)) return true;
  }
  return false;
}

function multiCellSelection(state) {
  const {selection} = state;
  return selection instanceof CellSelection && selection.$anchorCell.pos !== selection.$headCell.pos;
}

function textblockSupportsParagraphLayout(node) {
  return Boolean(
    node
    && (node.type === schema.nodes.paragraph || node.type === schema.nodes.heading)
    && node.attrs.boundary !== true
  );
}

function selectedLayoutTextblocks(state) {
  if (multiCellSelection(state)) return [];
  return selectedTextblocks(state).filter(({node, position}) => (
    textblockSupportsParagraphLayout(node)
    && !positionHasAncestor(state, position, ['list_item', 'task_item'])
  ));
}

function selectedTextblockLayoutValue(state, attribute, fallback = 0) {
  const blocks = selectedLayoutTextblocks(state);
  if (blocks.length === 0) return fallback;
  const values = new Set(blocks.map(({node}) => node.attrs[attribute] ?? fallback));
  return values.size === 1 ? [...values][0] : 'mixed';
}

function selectedLineSpacing(state) {
  const blocks = selectedTextblocks(state);
  if (blocks.length === 0) {
    return 'default';
  }
  const values = new Set(blocks.map(({node}) => normalizedLineSpacing(node.attrs.lineSpacing) || 'default'));
  return values.size === 1 ? [...values][0] : 'mixed';
}

function paragraphLayoutValueLabel(value) {
  return value === 'mixed' ? 'Mixed' : `${Number(value) || 0} pt`;
}

function updateParagraphLayoutState(state) {
  const menu = document.getElementById('editor-paragraph-layout-menu');
  if (!(menu instanceof HTMLElement)) return;
  const blocks = selectedLayoutTextblocks(state);
  const enabled = blocks.length > 0;
  const values = {
    left: selectedTextblockLayoutValue(state, 'leftIndent', 0),
    right: selectedTextblockLayoutValue(state, 'rightIndent', 0),
    firstLine: selectedTextblockLayoutValue(state, 'firstLineIndent', 0),
  };
  const valueTargets = [
    ['[data-live-editor-left-indent-value]', values.left],
    ['[data-live-editor-right-indent-value]', values.right],
    ['[data-live-editor-first-line-indent-value]', values.firstLine],
  ];
  for (const [selector, value] of valueTargets) {
    const target = menu.querySelector(selector);
    if (target instanceof HTMLElement) target.textContent = paragraphLayoutValueLabel(value);
  }
  const lineSpacing = selectedLineSpacing(state);
  setTypographyOptionState(
    menu,
    '[data-live-editor-line-spacing]',
    lineSpacing,
    'liveEditorLineSpacing',
  );
  const alignment = selectedTextblockLayoutValue(state, 'textAlignment', 'left');
  menu.querySelectorAll('[data-live-editor-text-alignment]').forEach(option => {
    if (!(option instanceof HTMLElement)) return;
    const value = option.dataset.liveEditorTextAlignment || '';
    option.setAttribute('aria-checked', value === String(alignment) ? 'true' : 'false');
    option.dataset.mixed = alignment === 'mixed' ? 'true' : 'false';
  });
  const dropCap = selectedTextblockLayoutValue(state, 'dropCap', false);
  const dropCapButton = menu.querySelector('[data-live-editor-drop-cap]');
  if (dropCapButton instanceof HTMLElement) {
    dropCapButton.setAttribute('aria-checked', dropCap === true ? 'true' : 'false');
    dropCapButton.dataset.mixed = dropCap === 'mixed' ? 'true' : 'false';
  }
  const dropCapLabel = menu.querySelector('[data-live-editor-drop-cap-label]');
  if (dropCapLabel instanceof HTMLElement) {
    dropCapLabel.textContent = dropCap === 'mixed' ? 'Mixed' : (dropCap ? 'On' : 'Off');
  }
  menu.querySelectorAll('[data-live-editor-paragraph-layout-control]').forEach(control => {
    if (control instanceof HTMLButtonElement) control.disabled = !enabled;
  });
}

function setTypographyOptionState(root, selector, currentValue, dataAttribute) {
  root.querySelectorAll(selector).forEach(option => {
    if (!(option instanceof HTMLElement)) {
      return;
    }
    const value = option.dataset[dataAttribute] || '';
    option.setAttribute('aria-checked', value === String(currentValue) ? 'true' : 'false');
  });
}

function updateTypographyState(state) {
  const typographyMenu = document.getElementById('editor-typography-menu');
  const highlightMenu = document.getElementById('editor-highlight-palette');
  if (!(typographyMenu instanceof HTMLElement) && !(highlightMenu instanceof HTMLElement)) {
    return;
  }

  if (typographyMenu instanceof HTMLElement) {
    const family = selectedMarkAttribute(state, schema.marks.font_family, 'family', 'system');
    const size = selectedMarkAttribute(state, schema.marks.font_size, 'size', 'default');
    const textColor = selectedMarkAttribute(state, schema.marks.text_color, 'color', 'default');
    const lineSpacing = selectedLineSpacing(state);

    setTypographyOptionState(
      typographyMenu,
      '[data-live-editor-font-family]',
      family,
      'liveEditorFontFamily',
    );
    setTypographyOptionState(
      typographyMenu,
      '[data-live-editor-line-spacing]',
      lineSpacing,
      'liveEditorLineSpacing',
    );
    setTypographyOptionState(
      typographyMenu,
      '[data-live-editor-text-color]',
      textColor,
      'liveEditorTextColor',
    );
    const textColorLabel = typographyMenu.querySelector('[data-live-editor-text-color-label]');
    if (textColorLabel instanceof HTMLElement) {
      textColorLabel.textContent = TEXT_COLOR_LABELS[textColor] || TEXT_COLOR_LABELS.default;
    }

    typographyMenu.querySelectorAll('[data-live-editor-font-size-input]').forEach(input => {
      if (!(input instanceof HTMLInputElement)) {
        return;
      }
      input.dataset.fontSizeState = String(size);
      if (size === 'mixed' || size === 'default') {
        input.value = '';
        input.placeholder = size === 'mixed' ? 'Mixed' : 'Default';
      } else {
        input.value = String(size);
        input.placeholder = '';
      }
    });
    typographyMenu.querySelectorAll('[data-live-editor-font-size-step]').forEach(button => {
      if (!(button instanceof HTMLButtonElement)) {
        return;
      }
      const delta = Number(button.dataset.liveEditorFontSizeStep) || 0;
      const numeric = Number(size);
      button.disabled = Number.isFinite(numeric)
        && ((delta < 0 && numeric <= FONT_SIZE_MIN) || (delta > 0 && numeric >= FONT_SIZE_MAX));
    });
  }

  if (highlightMenu instanceof HTMLElement) {
    const highlightColor = selectedMarkAttribute(state, schema.marks.highlight, 'color', 'none');
    setTypographyOptionState(
      highlightMenu,
      '[data-live-editor-highlight-color]',
      highlightColor,
      'liveEditorHighlightColor',
    );
  }
}

function ancestorDepth(state, names) {
  const {$from} = state.selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    if (names.includes($from.node(depth).type.name)) {
      return depth;
    }
  }
  return -1;
}

function selectedBulletListMarker(state) {
  const contexts = selectedListContexts(state);
  if (contexts.length === 0 || contexts.some(context => context.node.type !== schema.nodes.bullet_list)) return null;
  const markers = new Set(contexts.map(context => normalizedBulletMarker(context.node.attrs.marker)));
  return markers.size === 1 ? [...markers][0] : 'mixed';
}

function selectedOrderedListStyle(state) {
  const contexts = selectedListContexts(state);
  if (contexts.length === 0 || contexts.some(context => context.node.type !== schema.nodes.ordered_list)) {
    return null;
  }
  const styles = new Set(contexts.map(context => context.node.attrs.style || 'auto'));
  return styles.size === 1 ? [...styles][0] : 'mixed';
}

function blockStyleAtResolved($pos) {
  if (listContextAtResolved($pos)) {
    return 'list';
  }
  for (let depth = $pos.depth; depth > 0; depth -= 1) {
    const node = $pos.node(depth);
    if (node.type === schema.nodes.heading) {
      return `h${node.attrs.level}`;
    }
    if (node.type === schema.nodes.blockquote) {
      return 'quote';
    }
    if (node.type === schema.nodes.code_block) {
      return 'code';
    }
    if (node.type === schema.nodes.paragraph) {
      return 'paragraph';
    }
  }
  return 'paragraph';
}

function selectedBlockStyle(state) {
  if (state.selection.empty) {
    return blockStyleAtResolved(state.selection.$from);
  }
  const styles = new Set(selectedTextblocks(state).map(({position}) => {
    const probe = Math.min(position + 1, state.doc.content.size);
    return blockStyleAtResolved(state.doc.resolve(probe));
  }));
  if (styles.size === 0) {
    return blockStyleAtResolved(state.selection.$from);
  }
  return styles.size === 1 ? [...styles][0] : 'mixed';
}

function updateListContextState(view, listType) {
  const context = document.getElementById(LIST_CONTEXT_ID);
  if (!(context instanceof HTMLElement)) {
    return;
  }
  const activeElement = document.activeElement;
  const relevantFocus = view.hasFocus()
    || (activeElement instanceof Node && context.contains(activeElement));
  const visible = listType !== null && relevantFocus;
  context.hidden = !visible;
  context.setAttribute('aria-hidden', visible ? 'false' : 'true');
  context.dataset.listType = listType || '';
}

function updateListStyleState(state, listType) {
  const orderedStyle = selectedOrderedListStyle(state);
  const bulletMarker = selectedBulletListMarker(state);
  const orderedLabels = {
    auto: 'Style: Auto', decimal: 'Style: 1, 2, 3',
    'lower-roman': 'Style: i, ii, iii', 'lower-alpha': 'Style: a, b, c', mixed: 'Style: Mixed',
  };
  const bulletLabels = {disc: 'Marker: Bullet', dash: 'Marker: Dash', mixed: 'Marker: Mixed'};
  document.querySelectorAll('[data-live-editor-list-style]').forEach(button => {
    if (!(button instanceof HTMLButtonElement)) return;
    const enabled = listType === 'ordered_list' || listType === 'bullet_list';
    button.disabled = !enabled;
    button.dataset.listStyle = listType === 'ordered_list' ? orderedStyle || '' : bulletMarker || '';
    const label = button.querySelector('[data-live-editor-list-style-label]');
    if (label instanceof HTMLElement) {
      label.textContent = listType === 'bullet_list'
        ? bulletLabels[bulletMarker] || bulletLabels.disc
        : orderedLabels[orderedStyle] || orderedLabels.auto;
    }
    const accessible = listType === 'bullet_list' ? 'Bullet-list marker style' : 'Ordered-list numbering style';
    button.title = accessible;
    button.setAttribute('aria-label', accessible);
  });
  document.querySelectorAll('[data-live-editor-list-style-option]').forEach(option => {
    if (!(option instanceof HTMLElement)) return;
    const action = option.dataset.liveEditorListStyleOption || '';
    const isBullet = action.startsWith('bullet_style_');
    const relevant = isBullet ? listType === 'bullet_list' : listType === 'ordered_list';
    option.hidden = !relevant;
    if (!relevant) {
      option.setAttribute('aria-checked', 'false');
      return;
    }
    const normalized = isBullet
      ? action.replace('bullet_style_', '').replaceAll('_', '-')
      : action.replace('list_style_', '').replaceAll('_', '-');
    option.setAttribute('aria-checked', normalized === (isBullet ? bulletMarker : orderedStyle) ? 'true' : 'false');
  });
  document.querySelectorAll('[data-live-editor-list-order-only]').forEach(element => {
    if (element instanceof HTMLElement) element.hidden = listType !== 'ordered_list';
  });
  document.querySelectorAll('[data-live-editor-list-numbering-command]').forEach(button => {
    if (!(button instanceof HTMLButtonElement)) return;
    button.hidden = listType !== 'ordered_list';
    if (listType !== 'ordered_list') return;
    const action = button.dataset.liveEditorListNumberingCommand;
    button.disabled = action === 'list_continue' ? !continueOrderedNumbering(state) : !restartOrderedNumbering(state);
  });
}

function currentTableContext(view) {
  const {selection} = view.state;
  let $cell = null;
  if (selection instanceof CellSelection) {
    $cell = selection.$headCell;
  } else {
    $cell = cellAround(selection.$from);
  }
  if (!$cell) {
    return null;
  }
  const table = findTable($cell);
  if (!table) {
    return null;
  }
  const map = TableMap.get(table.node);
  const relativeCellPos = $cell.pos - table.start;
  let rect = null;
  try {
    rect = map.findCell(relativeCellPos);
  } catch {
    return null;
  }
  return {$cell, table, map, rect};
}

function tableHasHeaderRow(context) {
  const firstRow = context?.table.node.firstChild;
  if (!firstRow || firstRow.childCount === 0) {
    return false;
  }
  for (let index = 0; index < firstRow.childCount; index += 1) {
    if (firstRow.child(index).type !== schema.nodes.table_header) {
      return false;
    }
  }
  return true;
}

function tableDomContext(view, context = currentTableContext(view)) {
  if (!context) {
    return null;
  }
  let cell = view.nodeDOM(context.$cell.pos);
  if (!(cell instanceof HTMLElement) || !cell.matches('td, th')) {
    const domAtPos = view.domAtPos(Math.min(context.$cell.pos + 1, view.state.doc.content.size));
    const candidate = domAtPos.node instanceof Element ? domAtPos.node : domAtPos.node.parentElement;
    cell = candidate?.closest('td, th') || null;
  }
  if (!(cell instanceof HTMLElement)) {
    return null;
  }
  const row = cell.closest('tr');
  const tableElement = cell.closest('table');
  const wrapper = cell.closest('.tableWrapper') || tableElement?.parentElement;
  if (!(row instanceof HTMLElement) || !(tableElement instanceof HTMLElement) || !(wrapper instanceof HTMLElement)) {
    return null;
  }
  return {...context, cell, row, tableElement, wrapper};
}

function updateToolbarState(view) {
  const state = view.state;
  const usable = state.selection.from >= 0;
  setButtonState('bold', markActive(state, schema.marks.strong), !usable);
  setButtonState('italic', markActive(state, schema.marks.em), !usable);
  setButtonState('underline', markActive(state, schema.marks.underline), !usable);
  setButtonState('strikethrough', markActive(state, schema.marks.strike), !usable);
  setButtonState('superscript', markActive(state, schema.marks.superscript), !usable);
  setButtonState('subscript', markActive(state, schema.marks.subscript), !usable);
  setButtonState('highlight', markActive(state, schema.marks.highlight), !usable);
  setButtonState('link', markActive(state, schema.marks.link), !usable);
  const listType = selectedListType(state);
  setButtonState('bullet', listType === 'bullet_list', !usable);
  setButtonState('numbered', listType === 'ordered_list', !usable);
  setButtonState('checklist', listType === 'task_list', !usable);
  setButtonState('outdent', false, !liftSelectedListItems(state));
  setButtonState('indent', false, !sinkSelectedListItems(state));
  updateListContextState(view, listType);
  updateListStyleState(state, listType);
  setButtonState('quote', selectedBlockStyle(state) === 'quote', !usable);
  setButtonState('code', selectedBlockStyle(state) === 'code', !usable);
  setButtonState('horizontal_rule', false, !usable);
  const tableContext = currentTableContext(view);
  setButtonState('table', tableContext !== null, !usable);
  setButtonState('undo', undoDepth(state) > 0, undoDepth(state) === 0);
  setButtonState('redo', redoDepth(state) > 0, redoDepth(state) === 0);
  setBlockStyleState(selectedBlockStyle(state), listType, !usable);
  updateTypographyState(state);
  updateParagraphLayoutState(state);
}

function documentIsEmpty(doc) {
  return Boolean(
    doc.childCount === 1
    && doc.firstChild?.type === schema.nodes.paragraph
    && doc.firstChild.content.size === 0
  );
}

function placeholderPlugin() {
  return new Plugin({
    key: placeholderKey,
    props: {
      decorations(state) {
        if (!documentIsEmpty(state.doc)) {
          return DecorationSet.empty;
        }
        const paragraph = state.doc.firstChild;
        if (!paragraph) {
          return DecorationSet.empty;
        }
        return DecorationSet.create(state.doc, [
          Decoration.node(0, paragraph.nodeSize, {
            class: 'prosemirror-empty-paragraph',
            'data-placeholder': 'Start writing',
          }),
        ]);
      },
    },
  });
}

function findHighlightPlugin() {
  return new Plugin({
    key: findHighlightKey,
    state: {
      init() {
        return DecorationSet.empty;
      },
      apply(transaction, decorations) {
        const update = transaction.getMeta(findHighlightKey);
        if (update !== undefined) {
          if (!update || !update.match) return DecorationSet.empty;
          const match = update.match;
          const attributes = {
            class: 'prosemirror-find-match prosemirror-find-match-current',
            'data-rustnote-find-match': 'current',
          };
          const decoration = match.decorationKind === 'node'
            ? Decoration.node(match.decorationFrom, match.decorationTo, attributes, {findMatch: true})
            : Decoration.inline(match.from, match.to, attributes, {findMatch: true});
          return DecorationSet.create(transaction.doc, [decoration]);
        }
        return transaction.docChanged
          ? DecorationSet.empty
          : decorations.map(transaction.mapping, transaction.doc);
      },
    },
    props: {
      decorations(state) {
        return findHighlightKey.getState(state) || DecorationSet.empty;
      },
    },
  });
}

function isAtomicBoundaryNode(node) {
  return node?.type === schema.nodes.table || node?.type === schema.nodes.horizontal_rule || node?.type === schema.nodes.attachment;
}

function atomicBoundaryParagraphAttrs(attrs = {}) {
  return {
    ...attrs,
    lineSpacing: null,
    textAlignment: 'left',
    leftIndent: 0,
    rightIndent: 0,
    firstLineIndent: 0,
    dropCap: false,
    boundary: true,
  };
}

function atomicBoundaryParagraph() {
  return schema.nodes.paragraph.create(atomicBoundaryParagraphAttrs());
}

function isEmptyParagraph(node) {
  return node?.type === schema.nodes.paragraph && node.content.size === 0;
}

function isAtomicBoundaryParagraph(node) {
  return isEmptyParagraph(node) && node.attrs.boundary === true;
}

function isListNode(node) {
  return node?.type === schema.nodes.bullet_list
    || node?.type === schema.nodes.ordered_list
    || node?.type === schema.nodes.task_list;
}

function isStaleListBoundaryParagraph(node, previous, next) {
  return isAtomicBoundaryParagraph(node) && (isListNode(previous) || isListNode(next));
}

function ordinaryParagraphFrom(node) {
  return schema.nodes.paragraph.create(
    {lineSpacing: normalizedLineSpacing(node?.attrs?.lineSpacing), boundary: false},
    node?.content || null,
    node?.marks,
  );
}

function normalizeAtomicBoundaryDocument(doc) {
  const original = [];
  doc.forEach(node => original.push(node));
  const content = [];
  let changed = false;
  for (let index = 0; index < original.length; index += 1) {
    const node = original[index];
    const previous = index > 0 ? original[index - 1] : null;
    const next = index + 1 < original.length ? original[index + 1] : null;
    if (node.type === schema.nodes.paragraph && node.attrs.boundary === true && node.content.size > 0) {
      content.push(ordinaryParagraphFrom(node));
      changed = true;
      continue;
    }
    if (isEmptyParagraph(node) && isAtomicBoundaryNode(previous) && isAtomicBoundaryNode(next)) {
      changed = true;
      continue;
    }
    if (isStaleListBoundaryParagraph(node, previous, next)) {
      content.push(ordinaryParagraphFrom(node));
      changed = true;
      continue;
    }
    const validLeadingBoundary = index === 0 && isAtomicBoundaryNode(next);
    const validTrailingBoundary = index === original.length - 1 && isAtomicBoundaryNode(previous);
    if (isAtomicBoundaryParagraph(node) && !validLeadingBoundary && !validTrailingBoundary) {
      changed = true;
      continue;
    }
    if (isEmptyParagraph(node) && !node.attrs.boundary && (validLeadingBoundary || validTrailingBoundary)) {
      content.push(atomicBoundaryParagraph());
      changed = true;
      continue;
    }
    content.push(node);
  }
  if (content.length === 0) {
    content.push(schema.nodes.paragraph.create());
    changed = true;
  }
  if (isAtomicBoundaryNode(content[0])) {
    content.unshift(atomicBoundaryParagraph());
    changed = true;
  }
  if (isAtomicBoundaryNode(content.at(-1))) {
    content.push(atomicBoundaryParagraph());
    changed = true;
  }
  return changed ? doc.type.create(doc.attrs, content, doc.marks) : doc;
}

function atomicBoundaryDecorations(state) {
  const decorations = [];
  state.doc.forEach((node, offset, index) => {
    if (node.type !== schema.nodes.paragraph || node.attrs.boundary !== true || node.content.size !== 0) {
      return;
    }
    const previous = index > 0 ? state.doc.child(index - 1) : null;
    const next = index + 1 < state.doc.childCount ? state.doc.child(index + 1) : null;
    const active = state.selection.$from.parent === node;
    decorations.push(Decoration.node(offset, offset + node.nodeSize, {
      class: active
        ? 'prosemirror-atomic-boundary-paragraph prosemirror-atomic-boundary-active'
        : 'prosemirror-atomic-boundary-paragraph',
      'data-atomic-boundary-position': isAtomicBoundaryNode(previous) ? 'after' : 'before',
      'data-atomic-boundary-active': active ? 'true' : 'false',
    }));
  });
  return DecorationSet.create(state.doc, decorations);
}

function nestedEdgeBoundaryPositions(doc) {
  const positions = new Set();
  const visitEdge = (node, position, useLastChild, depth) => {
    let current = node;
    let currentPosition = position;
    let currentDepth = depth;
    while (current) {
      if (
        currentDepth > 1
        && current.type === schema.nodes.paragraph
        && current.attrs.boundary === true
      ) {
        positions.add(currentPosition);
      }
      if (current.childCount === 0) {
        break;
      }
      const childIndex = useLastChild ? current.childCount - 1 : 0;
      let childOffset = 0;
      for (let index = 0; index < childIndex; index += 1) {
        childOffset += current.child(index).nodeSize;
      }
      currentPosition += 1 + childOffset;
      current = current.child(childIndex);
      currentDepth += 1;
    }
  };
  if (doc.childCount > 0) {
    visitEdge(doc.firstChild, 0, false, 1);
    let lastPosition = 0;
    for (let index = 0; index < doc.childCount - 1; index += 1) {
      lastPosition += doc.child(index).nodeSize;
    }
    visitEdge(doc.lastChild, lastPosition, true, 1);
  }
  return [...positions].sort((left, right) => right - left);
}

function appendAtomicBoundaryNormalization(state, addToHistory) {
  let transaction = state.tr;
  if (!addToHistory) {
    transaction = transaction.setMeta('addToHistory', false);
  }
  let changed = false;
  for (const position of nestedEdgeBoundaryPositions(state.doc)) {
    const mapped = transaction.mapping.map(position);
    const node = transaction.doc.nodeAt(mapped);
    if (node?.type === schema.nodes.paragraph && node.attrs.boundary === true) {
      transaction = transaction.setNodeMarkup(mapped, undefined, {
        ...node.attrs,
        boundary: false,
      }, node.marks);
      changed = true;
    }
  }
  const entries = [];
  state.doc.forEach((node, offset, index) => entries.push({node, offset, index}));
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const {node, offset} = entries[index];
    const previous = index > 0 ? entries[index - 1].node : null;
    const next = index + 1 < entries.length ? entries[index + 1].node : null;
    if (node.type === schema.nodes.paragraph && node.attrs.boundary === true && node.content.size > 0) {
      const mapped = transaction.mapping.map(offset);
      const current = transaction.doc.nodeAt(mapped);
      if (current?.type === schema.nodes.paragraph) {
        transaction = transaction.setNodeMarkup(mapped, undefined, {
          ...current.attrs,
          boundary: false,
        }, current.marks);
        changed = true;
      }
      continue;
    }
    if (isAtomicBoundaryParagraph(node) && isAtomicBoundaryNode(previous) && isAtomicBoundaryNode(next)) {
      const from = transaction.mapping.map(offset);
      const current = transaction.doc.nodeAt(from);
      if (isEmptyParagraph(current) && transaction.doc.childCount > 1) {
        transaction = transaction.delete(from, from + current.nodeSize);
        changed = true;
      }
      continue;
    }
    if (isStaleListBoundaryParagraph(node, previous, next)) {
      const mapped = transaction.mapping.map(offset);
      const current = transaction.doc.nodeAt(mapped);
      if (isAtomicBoundaryParagraph(current)) {
        transaction = transaction.setNodeMarkup(mapped, undefined, {
          ...current.attrs,
          boundary: false,
        }, current.marks);
        changed = true;
      }
      continue;
    }
    const validLeadingBoundary = index === 0 && isAtomicBoundaryNode(next);
    const validTrailingBoundary = index === entries.length - 1 && isAtomicBoundaryNode(previous);
    if (isAtomicBoundaryParagraph(node) && !validLeadingBoundary && !validTrailingBoundary) {
      const from = transaction.mapping.map(offset);
      const current = transaction.doc.nodeAt(from);
      if (isAtomicBoundaryParagraph(current) && transaction.doc.childCount > 1) {
        transaction = transaction.delete(from, from + current.nodeSize);
        changed = true;
      } else if (isAtomicBoundaryParagraph(current)) {
        transaction = transaction.setNodeMarkup(from, undefined, {
          ...current.attrs,
          boundary: false,
        }, current.marks);
        changed = true;
      }
    }
  }

  const first = transaction.doc.firstChild;
  if (isAtomicBoundaryNode(first)) {
    transaction = transaction.insert(0, atomicBoundaryParagraph());
    changed = true;
  } else if (
    isEmptyParagraph(first)
    && first?.attrs.boundary !== true
    && isAtomicBoundaryNode(transaction.doc.childCount > 1 ? transaction.doc.child(1) : null)
  ) {
    transaction = transaction.setNodeMarkup(0, undefined, atomicBoundaryParagraphAttrs(first.attrs), first.marks);
    changed = true;
  }

  const lastIndex = transaction.doc.childCount - 1;
  const last = lastIndex >= 0 ? transaction.doc.child(lastIndex) : null;
  const beforeLast = lastIndex > 0 ? transaction.doc.child(lastIndex - 1) : null;
  if (isAtomicBoundaryNode(last)) {
    transaction = transaction.insert(transaction.doc.content.size, atomicBoundaryParagraph());
    changed = true;
  } else if (isEmptyParagraph(last) && last?.attrs.boundary !== true && isAtomicBoundaryNode(beforeLast)) {
    let lastPosition = 0;
    for (let index = 0; index < lastIndex; index += 1) {
      lastPosition += transaction.doc.child(index).nodeSize;
    }
    transaction = transaction.setNodeMarkup(
      lastPosition,
      undefined,
      atomicBoundaryParagraphAttrs(last.attrs),
      last.marks,
    );
    changed = true;
  }
  return changed ? transaction : null;
}

function tableBoundaryPlugin() {
  return new Plugin({
    key: tableBoundaryKey,
    props: {
      decorations: atomicBoundaryDecorations,
    },
    appendTransaction(transactions, _oldState, state) {
      const addToHistory = transactions.some(
        transaction => transaction.docChanged && transaction.getMeta('addToHistory') !== false,
      );
      return appendAtomicBoundaryNormalization(state, addToHistory);
    },
  });
}

function preservedSelectionPlugin() {
  return new Plugin({
    key: preservedSelectionKey,
    state: {
      init() {
        return DecorationSet.empty;
      },
      apply(transaction, oldSet) {
        const mapped = oldSet.map(transaction.mapping, transaction.doc);
        const meta = transaction.getMeta(preservedSelectionKey);
        if (meta === null || meta === undefined) {
          return mapped;
        }
        if (meta === false || meta.from === meta.to) {
          return DecorationSet.empty;
        }
        const from = Math.max(1, Math.min(meta.from, transaction.doc.content.size));
        const to = Math.max(from, Math.min(meta.to, transaction.doc.content.size));
        return DecorationSet.create(transaction.doc, [
          Decoration.inline(from, to, {class: 'prosemirror-preserved-selection'}),
        ]);
      },
    },
    props: {
      decorations(state) {
        return preservedSelectionKey.getState(state);
      },
      handleDOMEvents: {
        blur(view) {
          const {from, to} = view.state.selection;
          if (from !== to) {
            view.dispatch(view.state.tr.setMeta(preservedSelectionKey, {from, to}).setMeta('addToHistory', false));
          }
          return false;
        },
        focus(view) {
          if (preservedSelectionKey.getState(view.state)?.find().length) {
            view.dispatch(view.state.tr.setMeta(preservedSelectionKey, false).setMeta('addToHistory', false));
          }
          return false;
        },
      },
    },
  });
}

function headingInputRule() {
  return textblockTypeInputRule(/^(#{1,6})\s$/, schema.nodes.heading, match => ({level: match[1].length}));
}

function orderedListInputRule() {
  return wrappingInputRule(
    /^(\d+)\.\s$/,
    schema.nodes.ordered_list,
    match => ({order: Number(match[1]) || 1}),
    (match, node) => node.childCount + node.attrs.order === Number(match[1]),
  );
}

function taskListInputRule() {
  return new InputRule(/^\s*[-+*]\s\[( |x|X)\]\s$/, (state, match, start, end) => {
    const {$from} = state.selection;
    if (!$from.parent.isTextblock || $from.depth < 1) {
      return null;
    }
    const blockStart = $from.before();
    const paragraph = schema.nodes.paragraph.create();
    const item = schema.nodes.task_item.create({checked: match[1].toLowerCase() === 'x'}, paragraph);
    const list = schema.nodes.task_list.create(null, item);
    const transaction = state.tr.replaceWith(blockStart, blockStart + $from.parent.nodeSize, list);
    const selectionPosition = Math.min(blockStart + 3, transaction.doc.content.size);
    return transaction.setSelection(TextSelection.near(transaction.doc.resolve(selectionPosition)));
  });
}


const SCIENTIFIC_PASTE_CONFIDENCE = Object.freeze({
  STRONG: 'strong',
  AMBIGUOUS: 'ambiguous',
  PLAIN: 'plain',
});

const CURRENCY_CONTEXT_PATTERN = /\b(?:amount|budget|cost|discount|fee|fees|paid|pay|price|priced|sale|subtotal|total|usd|inr|rupees?|dollars?|euros?|pounds?|yen)\s*(?:is|was|at|of|:)?\s*$/i;
const SCIENTIFIC_STRUCTURAL_LATEX_COMMANDS = Object.freeze([
  'begin', 'end', 'dfrac', 'tfrac', 'frac', 'left', 'right', 'lim',
  'mathbf', 'mathrm', 'text', 'overset', 'underset',
]);

function scientificLatexCommandNames(value) {
  const names = [];
  const pattern = /\\([A-Za-z]+)/g;
  const source = String(value || '');
  let match;
  while ((match = pattern.exec(source)) !== null) names.push(match[1]);
  return names;
}

function scientificExplicitLatexCommands() {
  const commands = new Set(SCIENTIFIC_STRUCTURAL_LATEX_COMMANDS);
  for (const command of SCIENTIFIC_NOTATION_BY_LATEX_COMMAND.keys()) commands.add(command);
  for (const group of SCIENTIFIC_TEMPLATE_GROUPS) {
    for (const [, source] of group.items) {
      if (!source) continue;
      for (const command of scientificLatexCommandNames(source)) commands.add(command);
    }
  }
  return commands;
}
const SCIENTIFIC_EXPLICIT_LATEX_COMMANDS = scientificExplicitLatexCommands();

function hasExplicitScientificLatexCommand(value) {
  for (const command of scientificLatexCommandNames(value)) {
    if (SCIENTIFIC_EXPLICIT_LATEX_COMMANDS.has(command)) return true;
  }
  return false;
}

function hasScientificPasteCandidate(value) {
  return containsScientificCharacter(value, SCIENTIFIC_PASTE_CANDIDATE_PATTERN);
}

function hasAmbiguousScientificNotation(value) {
  for (const character of String(value || '')) {
    if (
      character !== '$'
      && character !== '\\'
      && SCIENTIFIC_PASTE_CANDIDATE_SET.has(character)
    ) return true;
  }
  return false;
}

function looksLikeInterruptedCurrencySource(source) {
  return /^\s*\d+(?:[.,]\d{1,2})?\s+(?:and|or|to|through|-)\s*$/i.test(String(source || ''));
}

function looksLikeMonetaryAmount(source) {
  const value = String(source || '').trim();
  return /^(?:\d+|\d{1,3}(?:,\d{3})+(?:\.\d{1,2})?|\d+\.\d{2})$/.test(value);
}

function currencyContextBefore(text, opening) {
  return CURRENCY_CONTEXT_PATTERN.test(String(text || '').slice(Math.max(0, opening - 48), opening));
}

function classifyDollarDelimitedSource(text, opening, closing) {
  const value = String(text || '');
  const source = value.slice(opening + 1, closing);
  if (looksLikeInterruptedCurrencySource(source)) return SCIENTIFIC_PASTE_CONFIDENCE.AMBIGUOUS;
  if (/\d/.test(value[opening + 1] || '') && /\d/.test(value[closing + 1] || '')) {
    return SCIENTIFIC_PASTE_CONFIDENCE.AMBIGUOUS;
  }
  if (looksLikeMonetaryAmount(source) && currencyContextBefore(value, opening)) {
    return SCIENTIFIC_PASTE_CONFIDENCE.AMBIGUOUS;
  }
  return SCIENTIFIC_PASTE_CONFIDENCE.STRONG;
}

function classifyScientificPasteText(value) {
  const source = String(value || '').trim();
  if (source === '') return {confidence: SCIENTIFIC_PASTE_CONFIDENCE.PLAIN, kind: null, source: ''};
  const delimited = /^(?:\$\$[\s\S]*\$\$|\\\[[\s\S]*\\\]|\$[^\n$]+\$|\\\([\s\S]*\\\))$/.test(source);
  if (delimited) {
    const parsed = parseScientificComposerSource(source);
    return {confidence: SCIENTIFIC_PASTE_CONFIDENCE.STRONG, kind: 'equation', ...parsed};
  }
  if (hasExplicitScientificLatexCommand(source)) {
    const chemistry = /^\\(?:ce|pu)\s*\{/.test(source);
    return {
      confidence: SCIENTIFIC_PASTE_CONFIDENCE.STRONG,
      kind: chemistry ? 'chemistry' : 'equation',
      source,
      displayMode: chemistry || /\\begin\s*\{/.test(source) || source.includes('\n'),
    };
  }
  if (looksLikeChemicalExpression(source)) {
    return {
      confidence: SCIENTIFIC_PASTE_CONFIDENCE.STRONG,
      kind: 'chemistry',
      source: normalizeChemistryComposerInput(source),
      displayMode: false,
    };
  }
  if (looksLikeUnicodeScientificExpression(source)) {
    return {
      confidence: SCIENTIFIC_PASTE_CONFIDENCE.STRONG,
      kind: 'equation',
      source: unicodeScientificToLatex(source),
      displayMode: false,
    };
  }
  if (hasAmbiguousScientificNotation(source)) {
    return {confidence: SCIENTIFIC_PASTE_CONFIDENCE.AMBIGUOUS, kind: null, source};
  }
  return {confidence: SCIENTIFIC_PASTE_CONFIDENCE.PLAIN, kind: null, source};
}

function trimAutoLinkCandidate(value) {
  let candidate = String(value || '');
  while (/[.,;:!?]$/.test(candidate)) candidate = candidate.slice(0, -1);
  while (candidate.endsWith(')') && (candidate.match(/\(/g) || []).length < (candidate.match(/\)/g) || []).length) {
    candidate = candidate.slice(0, -1);
  }
  while (candidate.endsWith(']') && (candidate.match(/\[/g) || []).length < (candidate.match(/\]/g) || []).length) {
    candidate = candidate.slice(0, -1);
  }
  return candidate;
}

function autoLinkMatch(text, requireEnd = true) {
  const pattern = requireEnd
    ? /(?:^|\s)((?:https?:\/\/|mailto:)[^\s<>]+)$/i
    : /(?:^|\s)((?:https?:\/\/|mailto:)[^\s<>]+)\s$/i;
  const match = String(text || '').match(pattern);
  if (!match) return null;
  const candidate = trimAutoLinkCandidate(match[1]);
  const href = normalizeSafeHref(candidate);
  if (!href) return null;
  const offset = match.index + match[0].lastIndexOf(match[1]);
  return {href, offset, length: candidate.length};
}

function autoLinkInputRule() {
  return new InputRule(/(?:^|\s)((?:https?:\/\/|mailto:)[^\s<>]+)\s$/i, (state, match, start) => {
    if (
      scientificNodeContext(state)
      || selectionIsInsideNodeType(state, schema.nodes.code_block)
      || selectionUsesMark(state, schema.marks.code)
    ) return null;
    const result = autoLinkMatch(match[0], false);
    if (!result) return null;
    const from = start + result.offset;
    const to = from + result.length;
    if (state.doc.rangeHasMark(from, to, schema.marks.link) || state.doc.rangeHasMark(from, to, schema.marks.note_link) || state.doc.rangeHasMark(from, to, schema.marks.media_link)) return null;
    return state.tr.addMark(from, to, schema.marks.link.create({href: result.href, title: null}));
  });
}

function inlineMathInputRule() {
  return new InputRule(/\$([^$\n]+)\$$/, (state, match, start, end) => {
    const source = match[1];
    if (
      source.trim() === '' ||
      isLikelyCurrencyInlineInput(state, start, source) ||
      selectionUsesMark(state, schema.marks.code) ||
      state.doc.textBetween(Math.max(0, start - 1), start) === '\\'
    ) {
      return null;
    }
    const node = scientificNodeText(schema.nodes.math_inline, source);
    const transaction = state.tr.replaceWith(start, end, node);
    return transaction.setSelection(TextSelection.create(transaction.doc, start + node.nodeSize));
  });
}

function displayMathInputRule() {
  return new InputRule(/^\$\$\s$/, (state, _match, _start, _end) => {
    const {$from} = state.selection;
    if (!$from.parent.isTextblock || selectionUsesMark(state, schema.marks.code)) return null;
    const blockStart = $from.before();
    const node = scientificNodeText(schema.nodes.math_display, '');
    const transaction = state.tr.replaceWith(blockStart, blockStart + $from.parent.nodeSize, node);
    return transaction.setSelection(TextSelection.create(transaction.doc, blockStart + 1));
  });
}

function editorInputRules() {
  return inputRules({
    rules: [
      ...smartQuotes,
      ellipsis,
      emDash,
      lineStartEmDashInputRule(),
      headingInputRule(),
      wrappingInputRule(/^\s*>\s$/, schema.nodes.blockquote),
      wrappingInputRule(
        /^\s*[+*]\s$/,
        schema.nodes.bullet_list,
        () => bulletListAttrs('disc'),
        (_match, node) => normalizedBulletMarker(node.attrs.marker) === 'disc',
      ),
      wrappingInputRule(
        /^\s*—\s$/,
        schema.nodes.bullet_list,
        () => bulletListAttrs('dash'),
        (_match, node) => normalizedBulletMarker(node.attrs.marker) === 'dash',
      ),
      orderedListInputRule(),
      taskListInputRule(),
      autoLinkInputRule(),
      inlineMathInputRule(),
      displayMathInputRule(),
      textblockTypeInputRule(/^```$/, schema.nodes.code_block),
    ],
  });
}

function toggleCurrentTask(state, dispatch) {
  const depth = ancestorDepth(state, ['task_item']);
  if (depth < 0) {
    return false;
  }
  if (!dispatch) {
    return true;
  }
  const position = state.selection.$from.before(depth);
  const item = state.doc.nodeAt(position);
  if (item?.type !== schema.nodes.task_item) {
    return false;
  }
  dispatch(state.tr.setNodeMarkup(position, undefined, {
    ...item.attrs,
    checked: !item.attrs.checked,
  }));
  return true;
}

function insertHardBreak(state, dispatch) {
  const {$from} = state.selection;
  if (!$from.parent.isTextblock || $from.parent.type === schema.nodes.code_block) {
    return false;
  }
  if (dispatch) {
    dispatch(
      state.tr
        .replaceSelectionWith(schema.nodes.hard_break.create())
        .scrollIntoView(),
    );
  }
  return true;
}

function tableKeymap() {
  return keymap({
    Backspace: chainCommands(handleTableBoundaryBackspace, discreteCommand(deleteSelectedTable)),
    Delete: discreteCommand(deleteSelectedTable),
    Tab: goToNextCell(1),
    'Shift-Tab': goToNextCell(-1),
  });
}

function stableColumnResizing(options) {
  const plugin = columnResizing(options);
  const decorations = plugin.spec.props?.decorations;
  if (typeof decorations !== 'function') {
    return plugin;
  }
  const stableDecorations = state => {
    const decorationSet = decorations(state);
    if (!decorationSet) {
      return null;
    }
    const nodeDecorations = decorationSet.find().filter(decoration => decoration.from < decoration.to);
    return nodeDecorations.length > 0
      ? DecorationSet.create(state.doc, nodeDecorations)
      : null;
  };
  plugin.spec.props.decorations = stableDecorations;
  plugin.props.decorations = stableDecorations;
  return plugin;
}

function activeScriptMark(state) {
  if (!state.selection.empty) return null;
  const marks = state.storedMarks || state.selection.$from.marks();
  return schema.marks.superscript.isInSet(marks)
    || schema.marks.subscript.isInSet(marks)
    || null;
}

function nodeHasMark(node, markType) {
  return Boolean(node?.isText && markType.isInSet(node.marks));
}

function exitScriptMark(direction = 0, expectedType = null) {
  return (state, dispatch) => {
    const {selection} = state;
    if (!(selection instanceof TextSelection) || !selection.empty) return false;
    const active = activeScriptMark(state);
    if (!active || (expectedType && active.type !== expectedType)) return false;

    if (direction < 0 && nodeHasMark(selection.$from.nodeBefore, active.type)) return false;
    if (direction > 0 && nodeHasMark(selection.$from.nodeAfter, active.type)) return false;
    if (!dispatch) return true;

    let marks = state.storedMarks || selection.$from.marks();
    marks = schema.marks.superscript.removeFromSet(marks);
    marks = schema.marks.subscript.removeFromSet(marks);
    dispatch(state.tr.setStoredMarks(marks).setMeta('addToHistory', false));
    return true;
  };
}

function discreteCommand(command) {
  return (state, dispatch, view) => {
    if (!dispatch) {
      return command(state, undefined, view);
    }
    const controller = activeEditorController;
    if (controller?.view === view) {
      return controller.runDiscreteUserAction(() => command(state, dispatch, view));
    }
    return command(state, transaction => dispatch(closeHistory(transaction)), view);
  };
}

const LEADING_INDENT_WHITESPACE = /^[\t \u00a0\u1680\u2000-\u200a\u202f\u205f\u3000]+/u;
const NON_ASCII_INDENT_WHITESPACE = /[\t\u00a0\u1680\u2000-\u200a\u202f\u205f\u3000]/u;
function stripLeadingIndentWhitespace(text) {
  const source = String(text || '');
  const match = source.match(LEADING_INDENT_WHITESPACE);
  if (!match) return source;
  const prefix = match[0];
  if (!NON_ASCII_INDENT_WHITESPACE.test(prefix)) return source;
  return source.slice(prefix.length);
}

function normalizePastedIndentText(text) {
  return String(text || '').split('\n').map(stripLeadingIndentWhitespace).join('\n');
}

function normalizeTextblockLeadingIndent(node) {
  if (!node?.isTextblock || node.type === schema.nodes.code_block || node.childCount === 0) return node;
  const children = [];
  let atStart = true;
  node.forEach(child => {
    if (atStart && child.isText) {
      const normalized = stripLeadingIndentWhitespace(child.text || '');
      if (normalized !== '') {
        children.push(normalized === child.text ? child : schema.text(normalized, child.marks));
        atStart = false;
      }
      return;
    }
    children.push(child);
    atStart = false;
  });
  if (children.length === node.childCount && children.every((child, index) => child === node.child(index))) return node;
  return node.copy(Fragment.fromArray(children));
}

function normalizePastedIndentNode(node) {
  if (node.isTextblock) return normalizeTextblockLeadingIndent(node);
  if (node.isLeaf) return node;
  const children = [];
  let changed = false;
  node.forEach(child => {
    const normalized = normalizePastedIndentNode(child);
    children.push(normalized);
    changed = changed || normalized !== child;
  });
  return changed ? node.copy(Fragment.fromArray(children)) : node;
}

function normalizePastedIndentSlice(slice) {
  const children = [];
  let changed = false;
  slice.content.forEach(child => {
    const normalized = normalizePastedIndentNode(child);
    children.push(normalized);
    changed = changed || normalized !== child;
  });
  return changed ? new Slice(Fragment.fromArray(children), slice.openStart, slice.openEnd) : slice;
}

function pasteNormalizationPlugin() {
  return new Plugin({
    props: {
      transformPastedText(text, _plain, view) {
        return view.state.selection.$from.parent.type === schema.nodes.code_block
          ? text
          : normalizePastedIndentText(text);
      },
      transformPasted(slice, view) {
        return view.state.selection.$from.parent.type === schema.nodes.code_block
          ? slice
          : normalizePastedIndentSlice(slice);
      },
    },
  });
}

function selectionInsideAutoIndentExcludedContext(state) {
  return ancestorDepth(state, ['blockquote', 'list_item', 'task_item', 'table_cell', 'table_header']) >= 0;
}

const paragraphLayoutStateKey = new PluginKey('rustnote-paragraph-layout');

function paragraphLayoutStatePlugin() {
  return new Plugin({
    key: paragraphLayoutStateKey,
    state: {
      init() {
        return {suppressedPosition: null};
      },
      apply(transaction, previous, _oldState, newState) {
        let suppressedPosition = Number.isInteger(previous?.suppressedPosition)
          ? previous.suppressedPosition
          : null;
        if (suppressedPosition !== null && transaction.docChanged) {
          suppressedPosition = transaction.mapping.map(suppressedPosition, -1);
        }
        const meta = transaction.getMeta(AUTO_INDENT_SUPPRESSION_META);
        if (meta?.clear === true) suppressedPosition = null;
        if (Number.isInteger(meta?.position)) suppressedPosition = meta.position;
        if (suppressedPosition !== null) {
          const current = textblockPositionAtSelection(newState);
          if (!current || current.position !== suppressedPosition) suppressedPosition = null;
        }
        return {suppressedPosition};
      },
    },
  });
}

function splitTextblockWithParagraphDefaults(
  state,
  dispatch,
  {autoFirstLineIndent = null, defaultLineSpacing = null} = {},
) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) || !selection.empty || selectionInsideAutoIndentExcludedContext(state)) return false;
  const current = textblockPositionAtSelection(state);
  if (!current
      || (current.node.type !== schema.nodes.paragraph && current.node.type !== schema.nodes.heading)
      || current.node.attrs.boundary === true) return false;

  const sourceIsParagraph = current.node.type === schema.nodes.paragraph;
  const autoIndentEnabled = autoFirstLineIndent !== null
    && autoFirstLineIndent !== undefined
    && Number.isFinite(Number(autoFirstLineIndent));
  const normalizedDefaultSpacing = normalizedLineSpacing(defaultLineSpacing);
  if (!autoIndentEnabled && normalizedDefaultSpacing === null) return false;

  const suppressed = paragraphLayoutStateKey.getState(state)?.suppressedPosition === current.position;
  let transaction = null;
  const applied = splitBlock(state, next => { transaction = next; });
  if (!applied || transaction === null) return false;
  const {$from} = transaction.selection;
  const target = $from.parent;
  if (target.type !== schema.nodes.paragraph || target.attrs.boundary === true) {
    dispatch?.(transaction.scrollIntoView());
    return true;
  }

  const position = $from.before($from.depth);
  const attrs = {...target.attrs, dropCap: false};
  if (autoIndentEnabled) {
    if (sourceIsParagraph) Object.assign(attrs, textblockLayoutAttrs(current.node, target.type));
    const requested = suppressed
      ? 0
      : (sourceIsParagraph && Number(current.node.attrs.firstLineIndent) > 0
        ? Number(current.node.attrs.firstLineIndent)
        : Number(autoFirstLineIndent));
    attrs.firstLineIndent = normalizedTextblockIndent(requested, PARAGRAPH_FIRST_LINE_INDENT_MAX);
    attrs.dropCap = false;
  }

  const currentSpacing = sourceIsParagraph ? normalizedLineSpacing(current.node.attrs.lineSpacing) : null;
  if (currentSpacing !== null) {
    attrs.lineSpacing = currentSpacing;
  } else if (normalizedDefaultSpacing !== null) {
    attrs.lineSpacing = normalizedDefaultSpacing;
  }

  transaction = transaction.setNodeMarkup(position, undefined, attrs, target.marks);
  if (suppressed && autoIndentEnabled) transaction = transaction.setMeta(AUTO_INDENT_SUPPRESSION_META, {clear: true});
  dispatch?.(transaction.scrollIntoView());
  return true;
}

function splitTextblockWithFirstLineIndent(state, dispatch, amount = DEFAULT_AUTO_FIRST_LINE_INDENT) {
  return splitTextblockWithParagraphDefaults(state, dispatch, {autoFirstLineIndent: amount});
}

function applyNewParagraphDefaults(state, dispatch) {
  const controller = activeEditorController;
  if (!controller) return false;
  return splitTextblockWithParagraphDefaults(state, dispatch, {
    autoFirstLineIndent: controller.autoIndentEnabled ? controller.autoFirstLineIndent : null,
    defaultLineSpacing: controller.defaultLineSpacing,
  });
}

function clearFirstLineIndentOnBackspace(state, dispatch) {
  const {selection} = state;
  if (!(selection instanceof TextSelection)
      || !selection.empty
      || selection.$from.parentOffset !== 0) {
    return false;
  }
  for (let depth = selection.$from.depth; depth > 0; depth -= 1) {
    const ancestor = selection.$from.node(depth);
    if (ancestor.type === schema.nodes.list_item || ancestor.type === schema.nodes.task_item) return false;
  }
  const current = textblockPositionAtSelection(state);
  if (!current || !textblockSupportsParagraphLayout(current.node) || Number(current.node.attrs.firstLineIndent) <= 0) return false;
  if (!dispatch) return true;
  dispatch(state.tr.setNodeMarkup(current.position, undefined, {
    ...current.node.attrs,
    firstLineIndent: 0,
  }, current.node.marks)
    .setMeta(AUTO_INDENT_SUPPRESSION_META, {position: current.position})
    .scrollIntoView());
  return true;
}

function paragraphLayoutKeymap() {
  return keymap({
    Enter: applyNewParagraphDefaults,
    Backspace: clearFirstLineIndentOnBackspace,
  });
}

function formattingKeymap() {
  return keymap({
    'Mod-b': discreteCommand(toggleMark(schema.marks.strong)),
    'Mod-i': discreteCommand(toggleMark(schema.marks.em)),
    'Mod-u': discreteCommand(toggleMark(schema.marks.underline)),
    'Mod-Shift-v': requestPlainTextPaste,
    'Mod-Shift-x': discreteCommand(toggleMark(schema.marks.strike)),
    'Mod-Shift-h': discreteCommand(toggleMark(schema.marks.highlight)),
    ArrowLeft: exitScriptMark(-1),
    ArrowRight: exitScriptMark(1),
    ArrowUp: exitScriptMark(0, schema.marks.subscript),
    ArrowDown: exitScriptMark(0, schema.marks.superscript),
  });
}

function plugins() {
  return [
    history({newGroupDelay: 1000}),
    plainTextPastePlugin(),
    preservedSelectionPlugin(),
    findHighlightPlugin(),
    attachmentImportPlugin(),
    mediaLinkPastePlugin(),
    linkPastePlugin(),
    pasteNormalizationPlugin(),
    scientificSelectionPlugin(),
    editorClipboardProvenancePlugin(),
    placeholderPlugin(),
    tableBoundaryPlugin(),
    editorInputRules(),
    scientificKeymap(),
    tableKeymap(),
    listKeymap(),
    emDashSequenceKeymap(),
    atomicBoundaryKeymap(),
    editorBoundaryKeymap(),
    paragraphLayoutStatePlugin(),
    paragraphLayoutKeymap(),
    formattingKeymap(),
    keymap({
      'Mod-z': undo,
      'Shift-Mod-z': redo,
      'Mod-y': redo,
      'Shift-Enter': chainCommands(exitCode, insertHardBreak),
      'Mod-Enter': exitCode,
      'Mod-Shift-Enter': discreteCommand(toggleCurrentTask),
    }),
    keymap(baseKeymap),
    stableColumnResizing({
      handleWidth: 6,
      cellMinWidth: TABLE_MIN_COLUMN_WIDTH,
      defaultCellMinWidth: TABLE_DEFAULT_COLUMN_WIDTH,
      lastColumnResizable: true,
    }),
    dropCursor(),
    gapCursor(),
    tableEditing({allowTableNodeSelection: true}),
  ];
}


function scientificNodeContext(state) {
  const {selection} = state;
  if (selection instanceof NodeSelection && isScientificNode(selection.node)) {
    return {node: selection.node, position: selection.from, depth: selection.$from.depth};
  }
  const {$from} = selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    const node = $from.node(depth);
    if (isScientificNode(node)) {
      return {node, position: $from.before(depth), depth};
    }
  }
  return null;
}

function activeScientificInsertionTarget(state) {
  if (!scientificNodeContext(state)) return null;
  return state.selection instanceof NodeSelection ? 'active_append' : 'active_caret';
}

function scientificComposerInsertionMode(target, returnState = null) {
  if (target === 'active_append') return 'append';
  if (target === 'active_caret') return 'caret';
  if (target !== 'composer_insert' || !returnState) return null;
  const guidedCaret = returnState.viewMode === 'guided'
    && returnState.fieldInsertion
    && Array.isArray(returnState.fieldInsertion.path)
    && Number.isFinite(returnState.fieldInsertion.selectionStart)
    && Number.isFinite(returnState.fieldInsertion.selectionEnd);
  const sourceCaret = returnState.viewMode === 'source'
    && Number.isFinite(returnState.selectionStart)
    && Number.isFinite(returnState.selectionEnd);
  return guidedCaret || sourceCaret ? 'caret' : 'append';
}

function scientificSelectionDecorations(state) {
  if (state.selection instanceof NodeSelection) return DecorationSet.empty;
  const context = scientificNodeContext(state);
  if (!context) return DecorationSet.empty;
  return DecorationSet.create(state.doc, [
    Decoration.node(
      context.position,
      context.position + context.node.nodeSize,
      {class: 'prosemirror-scientific-active'},
      {scientificActive: true},
    ),
  ]);
}


function isEscapedAt(text, index) {
  let slashes = 0;
  for (let cursor = index - 1; cursor >= 0 && text[cursor] === '\\'; cursor -= 1) slashes += 1;
  return slashes % 2 === 1;
}

function findScientificDelimiter(text, delimiter, start, singleLine = false) {
  for (let index = start; index <= text.length - delimiter.length; index += 1) {
    if (singleLine && text[index] === '\n') return -1;
    if (text.startsWith(delimiter, index) && !isEscapedAt(text, index)) return index;
  }
  return -1;
}

function isLikelyCurrencyDelimiterPair(text, opening, closing) {
  return classifyDollarDelimitedSource(text, opening, closing) !== SCIENTIFIC_PASTE_CONFIDENCE.STRONG;
}

function isLikelyCurrencyInlineInput(state, start, source) {
  if (looksLikeInterruptedCurrencySource(source)) return true;
  const before = state.doc.textBetween(Math.max(0, start - 48), start, ' ', ' ');
  return looksLikeMonetaryAmount(source) && CURRENCY_CONTEXT_PATTERN.test(before);
}

function selectionIsInsideNodeType(state, nodeType) {
  const {$from} = state.selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    if ($from.node(depth).type === nodeType) return true;
  }
  return false;
}

function selectionUsesMark(state, markType) {
  const marks = state.storedMarks || state.selection.$from.marks();
  return marks.some(mark => mark.type === markType);
}

function appendScientificText(nodes, text) {
  if (text === '') return;
  const previous = nodes.at(-1);
  if (previous?.isText) {
    nodes[nodes.length - 1] = schema.text(`${previous.text || ''}${text}`, previous.marks);
  } else {
    nodes.push(schema.text(text));
  }
}

function parseInlineScientificMarkdown(text) {
  const source = String(text || '');
  const nodes = [];
  let buffer = '';
  let found = false;
  let cursor = 0;
  const flush = () => {
    appendScientificText(nodes, buffer);
    buffer = '';
  };
  while (cursor < source.length) {
    if (source[cursor] === '`') {
      let ticks = 1;
      while (source[cursor + ticks] === '`') ticks += 1;
      const delimiter = '`'.repeat(ticks);
      const end = source.indexOf(delimiter, cursor + ticks);
      if (end >= 0) {
        buffer += source.slice(cursor, end + ticks);
        cursor = end + ticks;
        continue;
      }
    }
    if (source.startsWith('\\$', cursor)) {
      buffer += '$';
      cursor += 2;
      continue;
    }
    if (source.startsWith('\\(', cursor)) {
      const end = findScientificDelimiter(source, '\\)', cursor + 2, true);
      if (end >= 0) {
        const latex = source.slice(cursor + 2, end);
        if (latex.trim() !== '') {
          flush();
          nodes.push(scientificNodeText(schema.nodes.math_inline, latex));
          found = true;
          cursor = end + 2;
          continue;
        }
      }
    }
    if (source[cursor] === '$' && source[cursor + 1] !== '$' && !isEscapedAt(source, cursor)) {
      const end = findScientificDelimiter(source, '$', cursor + 1, true);
      if (end >= 0) {
        const latex = source.slice(cursor + 1, end);
        if (
          latex.trim() !== '' &&
          !looksLikeInterruptedCurrencySource(latex) &&
          !isLikelyCurrencyDelimiterPair(source, cursor, end)
        ) {
          flush();
          nodes.push(scientificNodeText(schema.nodes.math_inline, latex));
          found = true;
          cursor = end + 1;
          continue;
        }
      }
    }
    buffer += source[cursor];
    cursor += 1;
  }
  flush();
  return found ? nodes : null;
}

function scientificParagraphFromLines(lines) {
  const content = [];
  let found = false;
  lines.forEach((line, index) => {
    const parsed = parseInlineScientificMarkdown(line);
    if (parsed) {
      content.push(...parsed);
      found = true;
    } else {
      appendScientificText(content, line);
    }
    if (index + 1 < lines.length) content.push(schema.nodes.hard_break.create());
  });
  return {node: schema.nodes.paragraph.create(null, content), found};
}

function parseScientificMarkdownBlocks(text) {
  const normalized = String(text || '').replace(/\r\n?/g, '\n');
  const lines = normalized.split('\n');
  const blocks = [];
  let paragraphLines = [];
  let found = false;
  const flushParagraph = () => {
    if (paragraphLines.length === 0) return;
    const paragraph = scientificParagraphFromLines(paragraphLines);
    blocks.push(paragraph.node);
    found = found || paragraph.found;
    paragraphLines = [];
  };
  for (let index = 0; index < lines.length;) {
    const line = lines[index];
    const trimmed = line.trim();
    if (trimmed.startsWith('```')) {
      flushParagraph();
      const language = trimmed.slice(3).trim() || null;
      const codeLines = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith('```')) {
        codeLines.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) index += 1;
      const code = codeLines.join('\n');
      blocks.push(schema.nodes.code_block.create(
        {language, lineSpacing: null},
        code === '' ? null : schema.text(code),
      ));
      continue;
    }
    const sameLineDollar = trimmed.match(/^\$\$([\s\S]*?)\$\$$/);
    const sameLineBracket = trimmed.match(/^\\\[([\s\S]*?)\\\]$/);
    if (sameLineDollar || sameLineBracket) {
      flushParagraph();
      const latex = (sameLineDollar?.[1] ?? sameLineBracket?.[1] ?? '').trim();
      blocks.push(scientificNodeText(schema.nodes.math_display, latex));
      found = true;
      index += 1;
      continue;
    }
    if (trimmed === '$$' || trimmed === '\\[') {
      const closing = trimmed === '$$' ? '$$' : '\\]';
      let end = index + 1;
      while (end < lines.length && lines[end].trim() !== closing) end += 1;
      if (end < lines.length) {
        flushParagraph();
        blocks.push(scientificNodeText(schema.nodes.math_display, lines.slice(index + 1, end).join('\n')));
        found = true;
        index = end + 1;
        continue;
      }
    }
    if (trimmed === '') {
      flushParagraph();
    } else {
      paragraphLines.push(line);
    }
    index += 1;
  }
  flushParagraph();
  return found ? blocks : null;
}

function hasInlineScientificDollarPair(source) {
  let cursor = 0;
  while (cursor < source.length) {
    const start = source.indexOf('$', cursor);
    if (start < 0) return false;
    if (isEscapedAt(source, start) || source[start - 1] === '$' || source[start + 1] === '$') {
      cursor = start + 1;
      continue;
    }
    const lineEnd = source.indexOf('\n', start + 1);
    let endCursor = start + 1;
    while (true) {
      const end = source.indexOf('$', endCursor);
      if (end < 0 || (lineEnd >= 0 && end >= lineEnd)) break;
      if (!isEscapedAt(source, end) && source[end - 1] !== '$' && source[end + 1] !== '$') {
        return true;
      }
      endCursor = end + 1;
    }
    if (lineEnd < 0) return false;
    cursor = lineEnd + 1;
  }
  return false;
}

function hasScientificMarkdownPasteCandidate(value) {
  const source = String(value || '');

  let inlineStart = source.indexOf('\\(');
  while (inlineStart >= 0) {
    const inlineEnd = source.indexOf('\\)', inlineStart + 2);
    if (inlineEnd >= 0) {
      const lineEnd = source.indexOf('\n', inlineStart + 2);
      if (lineEnd < 0 || inlineEnd < lineEnd) return true;
    }
    inlineStart = source.indexOf('\\(', inlineStart + 2);
  }

  const bracketStart = source.indexOf('\\[');
  if (bracketStart >= 0 && source.indexOf('\\]', bracketStart + 2) >= 0) return true;

  const displayDollarStart = source.indexOf('$$');
  if (displayDollarStart >= 0 && source.indexOf('$$', displayDollarStart + 2) >= 0) return true;

  return hasInlineScientificDollarPair(source);
}

function scientificMarkdownPasteSlice(text, state) {
  const source = String(text || '');
  if (!hasScientificMarkdownPasteCandidate(source)) return null;
  if (!source.includes('\n') && !source.trim().startsWith('$$') && !source.trim().startsWith('\\[')) {
    const inline = parseInlineScientificMarkdown(source);
    if (inline && state.selection.$from.parent.inlineContent) {
      return new Slice(Fragment.fromArray(inline), 0, 0);
    }
  }
  const blocks = parseScientificMarkdownBlocks(source);
  return blocks ? new Slice(Fragment.fromArray(blocks), 0, 0) : null;
}

function mediaLinkPastePlugin() {
  return new Plugin({
    props: {
      handlePaste(view, event) {
        if (!event) return false;
        const html = event.clipboardData?.getData('text/html') || '';
        if (html.includes('data-rustnote-media-link')) return false;
        const attrs = parseMediaDeepLinkUri((event.clipboardData?.getData('text/plain') || '').trim());
        if (!attrs) return false;
        const {state} = view;
        if (
          scientificNodeContext(state)
          || selectionIsInsideNodeType(state, schema.nodes.code_block)
          || selectionUsesMark(state, schema.marks.code)
          || !state.selection.$from.parent.inlineContent
        ) return false;
        const mark = schema.marks.media_link.create(attrs);
        const label = `Media · ${mediaTimeLabelStatic(attrs.startMs / 1000)}`;
        let transaction = state.tr;
        if (!state.selection.empty) {
          if (!state.selection.$from.sameParent(state.selection.$to)) return false;
          transaction = transaction
            .addMark(state.selection.from, state.selection.to, mark)
            .setSelection(TextSelection.create(transaction.doc, state.selection.to));
        } else {
          transaction = transaction.replaceSelectionWith(schema.text(label, [mark]));
        }
        event.preventDefault();
        view.dispatch(transaction.scrollIntoView());
        return true;
      },
    },
  });
}

function linkPastePlugin() {
  return new Plugin({
    key: linkPasteKey,
    props: {
      handlePaste(view, event) {
        if (!event) return false;
        const text = event.clipboardData?.getData('text/plain') || '';
        const href = normalizeSafeHref(text.trim());
        const {state} = view;
        if (
          !href
          || scientificNodeContext(state)
          || selectionIsInsideNodeType(state, schema.nodes.code_block)
          || selectionUsesMark(state, schema.marks.code)
          || !state.selection.$from.parent.inlineContent
        ) {
          return false;
        }
        const mark = schema.marks.link.create({href, title: null});
        let transaction = state.tr;
        if (!state.selection.empty) {
          if (!state.selection.$from.sameParent(state.selection.$to)) return false;
          transaction = transaction
            .addMark(state.selection.from, state.selection.to, mark)
            .setSelection(TextSelection.create(transaction.doc, state.selection.to));
        } else {
          transaction = transaction.replaceSelectionWith(schema.text(href, [mark]));
        }
        event.preventDefault();
        view.dispatch(transaction.scrollIntoView());
        return true;
      },
    },
  });
}

function fragmentContainsScientificNode(fragment) {
  let found = false;
  fragment.forEach(node => {
    if (found) return;
    if (isScientificNode(node) || (node.content.size > 0 && fragmentContainsScientificNode(node.content))) found = true;
  });
  return found;
}

function scientificClipboardInlineText(node) {
  let output = '';
  node.forEach(child => {
    if (child.isText) output += child.text || '';
    else if (child.type === schema.nodes.hard_break) output += '\n';
    else if (child.type === schema.nodes.math_inline) output += scientificNodeMarkdown(child);
    else if (child.isInline) output += child.textContent;
  });
  return output;
}

function scientificClipboardBlockText(node) {
  if (node.type === schema.nodes.math_display) return scientificNodeMarkdown(node);
  if (node.isTextblock) return scientificClipboardInlineText(node);
  if (node.type === schema.nodes.table) {
    const {rows} = tableMatrix(node, cell => (
      scientificClipboardBlockText(cell).replace(/\n+/g, ' / ').trim()
    ));
    return rows.map(cells => cells.join('\t')).join('\n');
  }
  if (['bullet_list', 'ordered_list', 'task_list'].includes(node.type.name)) {
    const items = [];
    node.forEach(item => items.push(scientificClipboardBlockText(item).trim()));
    return items.join('\n');
  }
  const blocks = [];
  node.forEach(child => {
    const value = scientificClipboardBlockText(child);
    if (value !== '') blocks.push(value);
  });
  return blocks.join(node.type.name === 'list_item' ? '\n' : '\n\n');
}

function scientificClipboardText(slice) {
  const blocks = [];
  slice.content.forEach(node => {
    const value = scientificClipboardBlockText(node);
    if (value !== '') blocks.push(value);
  });
  return blocks.join('\n\n').replace(/\n{3,}/g, '\n\n').trimEnd();
}

function scientificSelectionPlugin() {
  return new Plugin({
    key: scientificSelectionKey,
    props: {
      decorations: scientificSelectionDecorations,
      clipboardTextSerializer(slice) {
        if (!fragmentContainsScientificNode(slice.content)) {
          return slice.content.textBetween(0, slice.content.size, '\n\n');
        }
        return scientificClipboardText(slice);
      },
      handlePaste(view, event) {
        if (!event) return false;
        const text = event.clipboardData?.getData('text/plain') || '';
        const structured = structuredScientificClipboardPayload(event);
        if (text.trim() === '' && !structured) return false;
        if (
          selectionIsInsideNodeType(view.state, schema.nodes.code_block) ||
          selectionUsesMark(view.state, schema.marks.code)
        ) return false;
        const scientificContext = scientificNodeContext(view.state);
        if (scientificContext && !(view.state.selection instanceof NodeSelection)) {
          if (!structured) return false;
          event.preventDefault();
          view.dispatch(view.state.tr.insertText(structured.source).scrollIntoView());
          return true;
        }
        if (structured) {
          const displayMode = structured.displayMode || !view.state.selection.$from.parent.inlineContent;
          const node = scientificNodeText(
            displayMode ? schema.nodes.math_display : schema.nodes.math_inline,
            structured.source,
          );
          event.preventDefault();
          view.dispatch(view.state.tr.replaceSelection(new Slice(Fragment.from(node), 0, 0)).scrollIntoView());
          return true;
        }
        if (!hasScientificPasteCandidate(text)) return false;
        const slice = scientificMarkdownPasteSlice(text, view.state);
        if (slice) {
          event.preventDefault();
          view.dispatch(view.state.tr.replaceSelection(slice).scrollIntoView());
          return true;
        }
        const controller = activeEditorController;
        const classification = classifyScientificPasteText(text);
        if (controller && classification.confidence === SCIENTIFIC_PASTE_CONFIDENCE.STRONG) {
          event.preventDefault();
          return controller.openScientificComposerFromPaste(
            classification.kind,
            text,
            classification.source,
            classification.displayMode,
          );
        }
        return false;
      },
    },
  });
}

function displayMathOnEnter(state, dispatch) {
  const {$from} = state.selection;
  if (
    !$from.parent.isTextblock
    || $from.parent.type === schema.nodes.code_block
    || selectionUsesMark(state, schema.marks.code)
    || $from.parent.textContent !== '$$'
    || $from.parentOffset !== 2
  ) {
    return false;
  }
  if (!dispatch) return true;
  const start = $from.before();
  const node = scientificNodeText(schema.nodes.math_display, '');
  let transaction = state.tr.replaceWith(start, start + $from.parent.nodeSize, node);
  transaction = transaction.setSelection(TextSelection.create(transaction.doc, start + 1));
  dispatch(transaction.scrollIntoView());
  return true;
}

function selectedScientificContext(state) {
  const {selection} = state;
  if (!(selection instanceof NodeSelection) || !isScientificNode(selection.node)) return null;
  return {node: selection.node, position: selection.from};
}

function activateSelectedScientificNode(state, dispatch, direction = 1) {
  const context = selectedScientificContext(state);
  if (!context) return false;
  if (dispatch) {
    const offset = direction < 0 ? context.node.content.size : 0;
    dispatch(
      state.tr
        .setSelection(TextSelection.create(state.doc, context.position + 1 + offset))
        .scrollIntoView(),
    );
  }
  return true;
}

function scientificBoundarySelection(doc, position, direction = 1) {
  const $position = doc.resolve(clampDocumentPosition(doc, position));
  return GapCursor.valid($position)
    ? new GapCursor($position)
    : TextSelection.near($position, direction);
}

function mappedPositionAfterSteps(transaction, mapStart, position, assoc = 1) {
  const mapping = new Mapping();
  for (const stepMap of transaction.mapping.maps.slice(mapStart)) mapping.appendMap(stepMap);
  return mapping.map(position, assoc);
}

function paragraphAttrsFromTextblock(textblock) {
  return {
    ...textblockLayoutAttrs(textblock, schema.nodes.paragraph),
    boundary: false,
  };
}

function scientificConversionResult(transaction = null, error = '') {
  return {transaction, error};
}

function inlineScientificToDisplayTransaction(state, context) {
  if (!context || context.node.type !== schema.nodes.math_inline) {
    return scientificConversionResult(null, 'Select an inline equation before converting it.');
  }
  const $position = state.doc.resolve(context.position);
  const textblock = $position.parent;
  if (!textblock.isTextblock || !textblock.inlineContent || $position.depth === 0) {
    return scientificConversionResult(null, 'This inline equation is not inside a convertible text block.');
  }

  const child = textblock.childAfter($position.parentOffset);
  if (!child.node || child.offset !== $position.parentOffset || child.node !== context.node) {
    return scientificConversionResult(null, 'The selected equation changed before conversion could begin.');
  }

  const beforeContent = textblock.content.cut(0, $position.parentOffset);
  const afterOffset = $position.parentOffset + context.node.nodeSize;
  const afterContent = textblock.content.cut(afterOffset, textblock.content.size);
  const textblockDepth = $position.depth;
  const containerDepth = textblockDepth - 1;
  const container = $position.node(containerDepth);
  const index = $position.index(containerDepth);
  const textblockPosition = $position.before(textblockDepth);
  const listItemContainer = container.type === schema.nodes.list_item || container.type === schema.nodes.task_item;

  if (listItemContainer && index === 0 && beforeContent.size === 0) {
    return scientificConversionResult(
      null,
      'Convert is unavailable here because a list item must begin with meaningful paragraph text. Move the equation after some text or outside the list first.',
    );
  }

  const replacementNodes = [];
  if (beforeContent.size > 0) {
    replacementNodes.push(textblock.type.create(textblock.attrs, beforeContent, textblock.marks));
  }

  const displayNode = scientificNodeText(schema.nodes.math_display, context.node.textContent);
  replacementNodes.push(displayNode);

  if (afterContent.size > 0) {
    const trailingType = textblock.type === schema.nodes.heading ? schema.nodes.paragraph : textblock.type;
    const trailingAttrs = trailingType === schema.nodes.paragraph && textblock.type === schema.nodes.heading
      ? paragraphAttrsFromTextblock(textblock)
      : textblock.attrs;
    replacementNodes.push(trailingType.create(trailingAttrs, afterContent, textblock.marks));
  }

  const replacement = Fragment.fromArray(replacementNodes);
  if (!container.canReplace(index, index + 1, replacement)) {
    return scientificConversionResult(
      null,
      'This equation cannot be converted without changing the surrounding document structure.',
    );
  }

  const displayOffset = replacementNodes
    .slice(0, replacementNodes.indexOf(displayNode))
    .reduce((size, node) => size + node.nodeSize, 0);
  let transaction = state.tr;
  const mapStart = transaction.mapping.maps.length;
  transaction = transaction.replaceWith(
    textblockPosition,
    textblockPosition + textblock.nodeSize,
    replacement,
  );
  const replacementStart = mappedPositionAfterSteps(transaction, mapStart, textblockPosition, -1);
  transaction = transaction.setSelection(NodeSelection.create(transaction.doc, replacementStart + displayOffset));
  return scientificConversionResult(transaction);
}

function displayScientificToInlineTransaction(state, context) {
  if (!context || context.node.type !== schema.nodes.math_display) {
    return scientificConversionResult(null, 'Select a display equation before converting it.');
  }
  const $position = state.doc.resolve(context.position);
  const container = $position.parent;
  const index = $position.index();
  const child = container.childAfter($position.parentOffset);
  if (!child.node || child.offset !== $position.parentOffset || child.node !== context.node) {
    return scientificConversionResult(null, 'The selected equation changed before conversion could begin.');
  }

  const inlineNode = scientificNodeText(schema.nodes.math_inline, context.node.textContent);
  const paragraph = schema.nodes.paragraph.create(null, inlineNode);
  const replacement = Fragment.from(paragraph);
  if (!container.canReplace(index, index + 1, replacement)) {
    return scientificConversionResult(
      null,
      'This display equation cannot become a paragraph in its current container.',
    );
  }

  let transaction = state.tr;
  const mapStart = transaction.mapping.maps.length;
  transaction = transaction.replaceWith(
    context.position,
    context.position + context.node.nodeSize,
    paragraph,
  );
  const paragraphPosition = mappedPositionAfterSteps(transaction, mapStart, context.position, -1);
  const caret = paragraphPosition + 1 + inlineNode.nodeSize;
  transaction = transaction.setSelection(TextSelection.create(transaction.doc, caret));
  return scientificConversionResult(transaction);
}

function adjacentScientificContext(state, direction) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) && !(selection instanceof GapCursor)) return null;
  if (!selection.empty) return null;
  const {$from} = selection;
  const node = direction < 0 ? $from.nodeBefore : $from.nodeAfter;
  if (isScientificNode(node)) {
    return {
      node,
      position: direction < 0 ? selection.from - node.nodeSize : selection.from,
    };
  }

  if (!(selection instanceof TextSelection)) return null;
  const atTextblockEdge = direction < 0
    ? $from.parentOffset === 0
    : $from.parentOffset === $from.parent.content.size;
  if (!atTextblockEdge) return null;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    const parentDepth = depth - 1;
    const parent = $from.node(parentDepth);
    const currentIndex = $from.index(parentDepth);
    const siblingIndex = direction < 0 ? currentIndex - 1 : currentIndex + 1;
    if (siblingIndex < 0 || siblingIndex >= parent.childCount) continue;
    const sibling = parent.child(siblingIndex);
    if (!isScientificNode(sibling)) return null;
    return {
      node: sibling,
      position: direction < 0 ? $from.before(depth) - sibling.nodeSize : $from.after(depth),
    };
  }
  return null;
}

function selectAdjacentScientificNode(direction) {
  return (state, dispatch) => {
    const context = adjacentScientificContext(state, direction);
    if (!context) return false;
    if (dispatch) {
      dispatch(
        state.tr
          .setSelection(NodeSelection.create(state.doc, context.position))
          .scrollIntoView(),
      );
    }
    return true;
  };
}

function exitScientificNode(direction, displayOnly = false) {
  return (state, dispatch) => {
    const context = scientificNodeContext(state);
    if (!context || state.selection instanceof NodeSelection) return false;
    if (displayOnly && context.node.type !== schema.nodes.math_display) return false;
    const offset = state.selection.$from.parentOffset;
    const atEdge = direction < 0 ? offset === 0 : offset === context.node.content.size;
    if (!atEdge) return false;
    if (dispatch) {
      const target = direction < 0 ? context.position : context.position + context.node.nodeSize;
      dispatch(
        state.tr
          .setSelection(scientificBoundarySelection(state.doc, target, direction))
          .scrollIntoView(),
      );
    }
    return true;
  };
}

function movePastSelectedScientific(direction, displayOnly = false) {
  return (state, dispatch) => {
    const context = selectedScientificContext(state);
    if (!context) return false;
    if (displayOnly && context.node.type !== schema.nodes.math_display) return false;
    if (dispatch) {
      const target = direction < 0 ? context.position : context.position + context.node.nodeSize;
      dispatch(
        state.tr
          .setSelection(scientificBoundarySelection(state.doc, target, direction))
          .scrollIntoView(),
      );
    }
    return true;
  };
}

function handleScientificHorizontalArrow(direction) {
  return chainCommands(
    exitScientificNode(direction),
    movePastSelectedScientific(direction),
    selectAdjacentScientificNode(direction),
  );
}

function selectScientificOnBackspace(state, dispatch) {
  if (state.selection instanceof NodeSelection) return false;
  const context = scientificNodeContext(state);
  if (context && state.selection.$from.parentOffset === 0) {
    if (dispatch) {
      dispatch(state.tr.setSelection(NodeSelection.create(state.doc, context.position)).scrollIntoView());
    }
    return true;
  }
  const previous = adjacentScientificContext(state, -1);
  if (!previous) return false;
  if (dispatch) {
    dispatch(state.tr.setSelection(NodeSelection.create(state.doc, previous.position)).scrollIntoView());
  }
  return true;
}

function selectScientificOnDelete(state, dispatch) {
  if (state.selection instanceof NodeSelection) return false;
  const context = scientificNodeContext(state);
  if (context && state.selection.$from.parentOffset === context.node.content.size) {
    if (dispatch) {
      dispatch(state.tr.setSelection(NodeSelection.create(state.doc, context.position)).scrollIntoView());
    }
    return true;
  }
  const next = adjacentScientificContext(state, 1);
  if (!next) return false;
  if (dispatch) {
    dispatch(state.tr.setSelection(NodeSelection.create(state.doc, next.position)).scrollIntoView());
  }
  return true;
}

function deleteSelectedScientific(state, dispatch) {
  if (!selectedScientificContext(state)) return false;
  if (dispatch) dispatch(state.tr.deleteSelection().scrollIntoView());
  return true;
}

function escapeScientificNode(state, dispatch) {
  const context = scientificNodeContext(state);
  if (!context) return false;
  if (dispatch) {
    const target = context.position + context.node.nodeSize;
    dispatch(
      state.tr
        .setSelection(scientificBoundarySelection(state.doc, target, 1))
        .scrollIntoView(),
    );
  }
  return true;
}

function openSelectedScientificTools(state, _dispatch, view) {
  const context = selectedScientificContext(state);
  if (!context || !view) return false;
  const nodeDOM = view.nodeDOM(context.position);
  const anchor = nodeDOM instanceof HTMLElement ? nodeDOM : nodeDOM?.parentElement;
  if (!(anchor instanceof HTMLElement)) return false;
  return activeEditorController?.openScientificTools(anchor) || false;
}

function scientificKeymap() {
  return keymap({
    Enter: chainCommands(
      displayMathOnEnter,
      (state, dispatch) => activateSelectedScientificNode(state, dispatch, 1),
      (state, dispatch) => {
        const context = scientificNodeContext(state);
        if (context?.node.type !== schema.nodes.math_display || state.selection instanceof NodeSelection) return false;
        if (dispatch) dispatch(state.tr.insertText('\n').scrollIntoView());
        return true;
      },
    ),
    Escape: escapeScientificNode,
    ArrowLeft: handleScientificHorizontalArrow(-1),
    ArrowRight: handleScientificHorizontalArrow(1),
    ArrowUp: chainCommands(exitScientificNode(-1, true), movePastSelectedScientific(-1, true)),
    ArrowDown: chainCommands(exitScientificNode(1, true), movePastSelectedScientific(1, true)),
    Backspace: chainCommands(
      selectScientificOnBackspace,
      discreteCommand(deleteSelectedScientific),
    ),
    Delete: chainCommands(
      selectScientificOnDelete,
      discreteCommand(deleteSelectedScientific),
    ),
    'Shift-F10': openSelectedScientificTools,
  });
}

const scientificRenderViews = new Map();
const scientificRenderPriorityQueue = new Set();
const scientificRenderFallbackQueue = new Set();
const scientificRenderInitialQueue = new Set();
let scientificRenderObserver = null;
let scientificRenderObserverResolved = false;
let scientificRenderFrame = null;
let scientificRenderInitialFrame = null;
let scientificRenderFallbackHandle = null;

function scientificRenderObserverInstance() {
  if (scientificRenderObserverResolved) return scientificRenderObserver;
  scientificRenderObserverResolved = true;
  try {
    const Observer = window.IntersectionObserver;
    if (typeof Observer !== 'function') return null;
    scientificRenderObserver = new Observer(entries => {
      for (const entry of entries) {
        const view = scientificRenderViews.get(entry.target);
        if (view) view.setVisible(entry.isIntersecting);
      }
    }, {rootMargin: `${SCIENTIFIC_RENDER_ROOT_MARGIN}px 0px`});
  } catch (error) {
    console.warn('Rustnote scientific viewport rendering is using the queued fallback', error);
    scientificRenderObserver = null;
  }
  return scientificRenderObserver;
}

function scientificViewCanRender(view) {
  return Boolean(view && !view.destroyed && view.needsRender && view.dom?.isConnected);
}

function scientificViewNearViewport(view) {
  if (!view?.dom?.isConnected) return false;
  const rect = view.dom.getBoundingClientRect();
  return rect.bottom >= -SCIENTIFIC_RENDER_ROOT_MARGIN
    && rect.top <= window.innerHeight + SCIENTIFIC_RENDER_ROOT_MARGIN;
}

function renderQueuedScientificView(view) {
  scientificRenderPriorityQueue.delete(view);
  scientificRenderFallbackQueue.delete(view);
  if (scientificViewCanRender(view)) view.render();
}

function flushScientificPriorityQueue() {
  scientificRenderFrame = null;
  let rendered = 0;
  for (const view of [...scientificRenderPriorityQueue]) {
    renderQueuedScientificView(view);
    rendered += 1;
    if (rendered >= SCIENTIFIC_RENDER_VISIBLE_BATCH) break;
  }
  if (scientificRenderPriorityQueue.size > 0) {
    scientificRenderFrame = window.requestAnimationFrame(flushScientificPriorityQueue);
  }
}

function flushScientificFallbackQueue(deadline = null) {
  scientificRenderFallbackHandle = null;
  let rendered = 0;
  for (const view of [...scientificRenderFallbackQueue]) {
    if (deadline && rendered > 0 && deadline.timeRemaining() < 2 && !deadline.didTimeout) break;
    renderQueuedScientificView(view);
    rendered += 1;
    if (rendered >= SCIENTIFIC_RENDER_FALLBACK_BATCH) break;
  }
  if (scientificRenderFallbackQueue.size > 0) scheduleScientificFallbackFlush();
}

function scheduleScientificFallbackFlush() {
  if (scientificRenderFallbackHandle !== null) return;
  if (typeof window.requestIdleCallback === 'function') {
    scientificRenderFallbackHandle = window.requestIdleCallback(flushScientificFallbackQueue, {timeout: 500});
  } else {
    scientificRenderFallbackHandle = window.setTimeout(() => flushScientificFallbackQueue(null), 32);
  }
}

function scheduleScientificRender(view, priority = false) {
  if (!scientificViewCanRender(view)) return;
  if (priority) {
    scientificRenderFallbackQueue.delete(view);
    scientificRenderPriorityQueue.add(view);
    if (scientificRenderFrame === null) {
      scientificRenderFrame = window.requestAnimationFrame(flushScientificPriorityQueue);
    }
    return;
  }
  if (!scientificRenderPriorityQueue.has(view)) scientificRenderFallbackQueue.add(view);
  scheduleScientificFallbackFlush();
}

function flushScientificInitialQueue() {
  scientificRenderInitialFrame = null;
  for (const view of [...scientificRenderInitialQueue]) {
    scientificRenderInitialQueue.delete(view);
    if (!scientificViewCanRender(view)) continue;
    if (view.viewportObserved) {
      if (scientificViewNearViewport(view)) scheduleScientificRender(view, true);
      continue;
    }
    scheduleScientificRender(view, false);
  }
}

function scheduleScientificInitialCheck(view) {
  scientificRenderInitialQueue.add(view);
  if (scientificRenderInitialFrame === null) {
    scientificRenderInitialFrame = window.requestAnimationFrame(flushScientificInitialQueue);
  }
}

function registerScientificRenderView(view) {
  scientificRenderViews.set(view.dom, view);
  view.viewportObserved = false;
  const observer = scientificRenderObserverInstance();
  if (observer) {
    try {
      observer.observe(view.dom);
      view.viewportObserved = true;
    } catch (error) {
      console.warn('Rustnote could not observe a scientific node; using queued rendering', error);
    }
  }
  scheduleScientificInitialCheck(view);
}

function unregisterScientificRenderView(view) {
  scientificRenderInitialQueue.delete(view);
  scientificRenderPriorityQueue.delete(view);
  scientificRenderFallbackQueue.delete(view);
  if (view.viewportObserved && scientificRenderObserverResolved && scientificRenderObserver) {
    try {
      scientificRenderObserver.unobserve(view.dom);
    } catch {
      // The view is being destroyed; observer cleanup failure is non-fatal.
    }
  }
  view.viewportObserved = false;
  scientificRenderViews.delete(view.dom);
}

if (typeof window.addEventListener === 'function') {
  window.addEventListener('beforeprint', () => {
    for (const view of scientificRenderViews.values()) {
      if (!view.destroyed && view.needsRender) view.render();
    }
  });
}

class ScientificMathView {
  constructor(node, view, getPos) {
    this.node = node;
    this.view = view;
    this.getPos = getPos;
    this.displayMode = node.type === schema.nodes.math_display;
    this.destroyed = false;
    this.lastRenderedKey = null;
    this.dom = document.createElement(this.displayMode ? 'div' : 'span');
    this.dom.className = this.displayMode
      ? 'prosemirror-scientific-node prosemirror-scientific-display'
      : 'prosemirror-scientific-node prosemirror-scientific-inline';
    this.dom.dataset.scientificType = this.displayMode ? 'display' : 'inline';
    this.dom.setAttribute('role', 'group');
    this.dom.setAttribute('aria-label', this.displayMode ? 'Display equation' : 'Inline equation');
    this.renderDOM = document.createElement(this.displayMode ? 'div' : 'span');
    this.renderDOM.className = 'prosemirror-scientific-render';
    this.renderDOM.contentEditable = 'false';
    this.renderDOM.setAttribute('aria-hidden', 'true');
    this.assistiveDOM = document.createElement('span');
    this.assistiveDOM.className = 'prosemirror-scientific-assistive sr-only';
    this.assistiveDOM.contentEditable = 'false';
    this.contentDOM = document.createElement(this.displayMode ? 'div' : 'span');
    this.contentDOM.className = 'prosemirror-scientific-source';
    this.contentDOM.setAttribute('aria-label', this.displayMode ? 'Display equation LaTeX source' : 'Inline equation LaTeX source');
    this.contentDOM.setAttribute('spellcheck', 'false');
    this.contentDOM.setAttribute('aria-hidden', 'true');
    this.toolsButton = document.createElement('button');
    this.toolsButton.type = 'button';
    this.toolsButton.className = 'prosemirror-scientific-tools-button';
    this.toolsButton.contentEditable = 'false';
    this.toolsButton.tabIndex = -1;
    this.toolsButton.textContent = '⋯';
    this.toolsButton.setAttribute('aria-label', 'Open scientific tools for this equation');
    this.dom.append(this.renderDOM, this.assistiveDOM, this.contentDOM, this.toolsButton);
    this.onRenderPointerDown = event => {
      event.preventDefault();
      event.stopPropagation();
      const position = this.getPos();
      if (!Number.isInteger(position)) return;
      this.view.dispatch(
        this.view.state.tr
          .setSelection(NodeSelection.create(this.view.state.doc, position))
          .scrollIntoView(),
      );
      this.view.focus();
      scheduleScientificRender(this, true);
    };
    this.onRenderDoubleClick = event => {
      event.preventDefault();
      event.stopPropagation();
      const position = this.getPos();
      if (!Number.isInteger(position)) return;
      this.view.dispatch(
        this.view.state.tr
          .setSelection(TextSelection.create(this.view.state.doc, position + 1))
          .scrollIntoView(),
      );
      this.view.focus();
      scheduleScientificRender(this, true);
    };
    this.renderDOM.addEventListener('mousedown', this.onRenderPointerDown);
    this.renderDOM.addEventListener('dblclick', this.onRenderDoubleClick);
    this.onToolsPointerDown = event => {
      event.preventDefault();
      event.stopPropagation();
    };
    this.onToolsClick = event => {
      event.preventDefault();
      event.stopPropagation();
      const position = this.getPos();
      if (!Number.isInteger(position)) return;
      const context = scientificNodeContext(this.view.state);
      if (!context || context.position !== position) {
        this.view.dispatch(
          this.view.state.tr
            .setSelection(NodeSelection.create(this.view.state.doc, position))
            .scrollIntoView(),
        );
      }
      this.view.focus();
      scheduleScientificRender(this, true);
      activeEditorController?.openScientificTools(this.dom);
    };
    this.toolsButton.addEventListener('pointerdown', this.onToolsPointerDown);
    this.toolsButton.addEventListener('mousedown', this.onToolsPointerDown);
    this.toolsButton.addEventListener('click', this.onToolsClick);
    this.visible = false;
    this.needsRender = true;
    this.renderDOM.dataset.mathPending = 'true';
    this.renderDOM.textContent = this.displayMode ? 'Display equation' : 'Equation';
    this.updateAssistiveContent();
    registerScientificRenderView(this);
  }

  setSourceEditingAccessible(active) {
    this.contentDOM.setAttribute('aria-hidden', active ? 'false' : 'true');
    this.assistiveDOM.setAttribute('aria-hidden', active ? 'true' : 'false');
  }

  updateAssistiveContent() {
    this.assistiveDOM.replaceChildren();
    if (scientificSourceTooLarge(this.node.textContent)) {
      this.assistiveDOM.textContent = 'Equation is too large to render; its source remains available for editing.';
      return;
    }
    const mathml = this.renderDOM.querySelector('.katex-mathml');
    if (mathml) {
      const clone = mathml.cloneNode(true);
      if (clone instanceof Element) {
        clone.removeAttribute('aria-hidden');
        clone.querySelectorAll('[aria-hidden]').forEach(element => element.removeAttribute('aria-hidden'));
      }
      this.assistiveDOM.appendChild(clone);
      return;
    }
    const source = this.node.textContent.trim();
    this.assistiveDOM.textContent = source === ''
      ? (this.displayMode ? 'Empty display equation' : 'Empty inline equation')
      : `Equation source: ${source}`;
  }

  setVisible(visible) {
    this.visible = visible;
    if (visible && this.needsRender) scheduleScientificRender(this, true);
  }

  render() {
    if (this.destroyed) return;
    const renderKey = `${this.displayMode ? 'display' : 'inline'}\u0000${this.node.textContent}`;
    if (!this.needsRender && renderKey === this.lastRenderedKey) return;
    renderScientificSource(this.renderDOM, this.node.textContent, this.displayMode);
    this.renderDOM.dataset.mathPending = 'false';
    this.lastRenderedKey = renderKey;
    this.needsRender = false;
    this.updateAssistiveContent();
  }

  update(node, decorations) {
    if (node.type !== this.node.type) return false;
    const changed = node.textContent !== this.node.textContent;
    this.node = node;
    if (changed) {
      this.needsRender = true;
      this.renderDOM.dataset.mathPending = 'true';
    }
    const active = decorations.some(decoration => decoration.spec?.scientificActive === true);
    this.dom.classList.toggle('prosemirror-scientific-active', active);
    this.setSourceEditingAccessible(active);
    if (changed || active) {
      if (active || this.visible) scheduleScientificRender(this, true);
      else if (!this.viewportObserved) scheduleScientificRender(this, false);
    }
    return true;
  }

  selectNode() {
    this.dom.classList.add('ProseMirror-selectednode');
    this.toolsButton.tabIndex = 0;
    this.setSourceEditingAccessible(false);
    scheduleScientificRender(this, true);
  }

  deselectNode() {
    this.dom.classList.remove('ProseMirror-selectednode');
    this.toolsButton.tabIndex = -1;
  }

  stopEvent(event) {
    return this.renderDOM.contains(event.target) || this.assistiveDOM.contains(event.target) || this.toolsButton.contains(event.target);
  }

  ignoreMutation(mutation) {
    return this.renderDOM.contains(mutation.target)
      || this.assistiveDOM.contains(mutation.target)
      || this.toolsButton.contains(mutation.target);
  }

  destroy() {
    this.destroyed = true;
    unregisterScientificRenderView(this);
    this.renderDOM.removeEventListener('mousedown', this.onRenderPointerDown);
    this.renderDOM.removeEventListener('dblclick', this.onRenderDoubleClick);
    this.toolsButton.removeEventListener('pointerdown', this.onToolsPointerDown);
    this.toolsButton.removeEventListener('mousedown', this.onToolsPointerDown);
    this.toolsButton.removeEventListener('click', this.onToolsClick);
  }
}

function editorViewIsEditable(view) {
  const editable = view?.props?.editable;
  return typeof editable !== 'function' || editable(view.state) !== false;
}

function syncTaskCheckboxInteractionState(checkbox, editable) {
  const checked = checkbox.getAttribute('aria-checked') === 'true';
  checkbox.tabIndex = editable ? 0 : -1;
  checkbox.setAttribute('aria-disabled', editable ? 'false' : 'true');
  checkbox.title = editable
    ? (checked ? 'Mark task as incomplete' : 'Mark task as complete')
    : 'Checklist item is read-only';
}

class TaskItemView {
  constructor(node, view, getPos) {
    this.node = node;
    this.view = view;
    this.getPos = getPos;
    this.dom = document.createElement('li');
    this.dom.dataset.taskItem = 'true';
    this.checkbox = document.createElement('span');
    this.checkbox.className = 'prosemirror-task-checkbox';
    this.checkbox.contentEditable = 'false';
    this.checkbox.setAttribute('role', 'checkbox');
    this.checkbox.setAttribute('aria-label', 'Toggle checklist item');
    this.contentDOM = document.createElement('div');
    this.contentDOM.className = 'prosemirror-task-content';
    this.dom.append(this.checkbox, this.contentDOM);
    this.onPointerDown = event => {
      event.preventDefault();
      event.stopPropagation();
    };
    this.toggleTask = () => {
      if (!editorViewIsEditable(this.view)) {
        return;
      }
      const position = this.getPos();
      if (typeof position !== 'number') {
        return;
      }
      const current = this.view.state.doc.nodeAt(position);
      if (current?.type !== schema.nodes.task_item) {
        return;
      }
      const scrollContainer = this.view.dom.closest('.markdown-preview');
      const scrollTop = scrollContainer instanceof HTMLElement ? scrollContainer.scrollTop : null;
      const scrollLeft = scrollContainer instanceof HTMLElement ? scrollContainer.scrollLeft : null;
      const restoreScroll = () => {
        if (scrollContainer instanceof HTMLElement && scrollTop !== null && scrollLeft !== null) {
          scrollContainer.scrollTop = scrollTop;
          scrollContainer.scrollLeft = scrollLeft;
        }
      };
      const transaction = this.view.state.tr.setNodeMarkup(position, undefined, {
        ...current.attrs,
        checked: !current.attrs.checked,
      });
      const controller = activeEditorController;
      if (controller?.view === this.view) {
        controller.runDiscreteUserAction(() => {
          this.view.dispatch(transaction);
          return true;
        });
      } else {
        this.view.dispatch(closeHistory(transaction));
      }
      restoreScroll();
      window.requestAnimationFrame(restoreScroll);
    };
    this.onClick = event => {
      event.preventDefault();
      event.stopPropagation();
      this.toggleTask();
    };
    this.onKeyDown = event => {
      if (event.key !== ' ' && event.key !== 'Enter') {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      this.toggleTask();
    };
    this.checkbox.addEventListener('pointerdown', this.onPointerDown);
    this.checkbox.addEventListener('mousedown', this.onPointerDown);
    this.checkbox.addEventListener('click', this.onClick);
    this.checkbox.addEventListener('keydown', this.onKeyDown);
    this.update(node);
  }

  update(node) {
    if (node.type !== schema.nodes.task_item) {
      return false;
    }
    this.node = node;
    const checked = Boolean(node.attrs.checked);
    this.dom.className = checked
      ? 'prosemirror-list-item prosemirror-list-item-checked'
      : 'prosemirror-list-item prosemirror-list-item-unchecked';
    this.dom.dataset.checked = checked ? 'true' : 'false';
    this.checkbox.setAttribute('aria-checked', checked ? 'true' : 'false');
    syncTaskCheckboxInteractionState(this.checkbox, editorViewIsEditable(this.view));
    return true;
  }

  stopEvent(event) {
    return event.target === this.checkbox;
  }

  ignoreMutation(mutation) {
    return mutation.type === 'attributes'
      && (mutation.target === this.dom || mutation.target === this.checkbox);
  }

  destroy() {
    this.checkbox.removeEventListener('pointerdown', this.onPointerDown);
    this.checkbox.removeEventListener('mousedown', this.onPointerDown);
    this.checkbox.removeEventListener('click', this.onClick);
    this.checkbox.removeEventListener('keydown', this.onKeyDown);
  }
}

function restartOrderedNumbering(state, dispatch) {
  const contexts = selectedListContexts(state)
    .filter(context => context.node.type === schema.nodes.ordered_list);
  if (contexts.length === 0) {
    return false;
  }
  if (!dispatch) {
    return true;
  }
  let transaction = state.tr;
  for (const context of contexts) {
    const node = transaction.doc.nodeAt(context.pos);
    if (node?.type === schema.nodes.ordered_list) {
      transaction = transaction.setNodeMarkup(
        context.pos,
        undefined,
        orderedListAttrs(1, node.attrs.style, true),
        node.marks,
      );
    }
  }
  dispatch(transaction.scrollIntoView());
  return true;
}

function continueOrderedNumbering(state, dispatch) {
  const context = commonSelectedListContext(state);
  if (!context || context.node.type !== schema.nodes.ordered_list) {
    return false;
  }
  const previous = adjacentOrderedContext(context, -1);
  if (!previous) {
    return false;
  }
  if (!dispatch) {
    return true;
  }
  const transaction = state.tr.setNodeMarkup(
    context.pos,
    undefined,
    orderedListAttrs(
      Number(previous.attrs.order) + previous.childCount,
      context.node.attrs.style || previous.attrs.style,
      false,
    ),
    context.node.marks,
  );
  dispatch(transaction.scrollIntoView());
  return true;
}

function clampDocumentPosition(doc, position) {
  const numeric = Number(position);
  if (!Number.isFinite(numeric)) {
    return 0;
  }
  return Math.max(0, Math.min(Math.trunc(numeric), doc.content.size));
}


function clamp(value, minimum, maximum) {
  return Math.max(minimum, Math.min(value, maximum));
}

function replaceTableWithParagraph(state, dispatch, tablePos, tableNode) {
  if (!dispatch || tableNode?.type !== schema.nodes.table) {
    return Boolean(tableNode?.type === schema.nodes.table);
  }
  const afterPosition = tablePos + tableNode.nodeSize;
  const following = state.doc.nodeAt(afterPosition);
  let transaction = state.tr.delete(tablePos, afterPosition);
  if (following?.type === schema.nodes.paragraph && following.attrs.boundary === true) {
    const mappedFollowing = transaction.mapping.map(afterPosition);
    const current = transaction.doc.nodeAt(mappedFollowing);
    if (current?.type === schema.nodes.paragraph) {
      transaction = transaction.setNodeMarkup(
        mappedFollowing,
        undefined,
        {...current.attrs, boundary: false},
        current.marks,
      );
    }
  }
  if (transaction.doc.childCount === 0) {
    transaction = transaction.insert(0, schema.nodes.paragraph.create());
  }
  const resolved = transaction.doc.resolve(Math.min(tablePos, transaction.doc.content.size));
  transaction = transaction.setSelection(TextSelection.near(resolved, 1));
  dispatch(transaction.scrollIntoView());
  return true;
}

function deleteSelectedTable(state, dispatch) {
  const {selection} = state;
  if (!(selection instanceof NodeSelection) || selection.node.type !== schema.nodes.table) {
    return false;
  }
  return replaceTableWithParagraph(state, dispatch, selection.from, selection.node);
}

function precedingTablePosition(selection) {
  if (
    !(selection instanceof TextSelection)
    || !selection.empty
    || !selection.$from.parent.isTextblock
    || selection.$from.parentOffset !== 0
  ) {
    return null;
  }
  const paragraphPosition = selection.$from.before(selection.$from.depth);
  const boundary = selection.$from.doc.resolve(paragraphPosition);
  const table = boundary.nodeBefore;
  return table?.type === schema.nodes.table
    ? paragraphPosition - table.nodeSize
    : null;
}

function handleTableBoundaryBackspace(state, dispatch) {
  const tablePosition = precedingTablePosition(state.selection);
  if (tablePosition === null) {
    return false;
  }
  const table = state.doc.nodeAt(tablePosition);
  if (!table || !NodeSelection.isSelectable(table)) {
    return false;
  }
  if (dispatch) {
    dispatch(
      state.tr
        .setSelection(NodeSelection.create(state.doc, tablePosition))
        .scrollIntoView(),
    );
  }
  return true;
}


function guidedField(labelText, value, handlers, options = {}) {
  const label = document.createElement('label');
  label.className = `scientific-guided-field${options.wide ? ' wide' : ''}`;
  const caption = document.createElement('span');
  caption.textContent = labelText;
  const input = options.multiline ? document.createElement('textarea') : document.createElement('input');
  if (!options.multiline) input.type = 'text';
  if (options.multiline) input.rows = options.rows || 2;
  input.value = String(value || '');
  input.placeholder = options.placeholder || '';
  input.autocapitalize = 'off';
  input.autocomplete = 'off';
  input.spellcheck = false;
  input.addEventListener('input', () => handlers.input(input.value));
  handlers.registerInput(input, options.path || null);
  label.append(caption, input);
  return {label, input};
}

function guidedStepper(labelText, value, minimum, maximum, onChange) {
  const row = document.createElement('div');
  row.className = 'scientific-guided-stepper';
  const label = document.createElement('span');
  label.textContent = labelText;
  const controls = document.createElement('div');
  controls.className = 'matrix-stepper-controls';
  const decrease = document.createElement('button');
  decrease.type = 'button';
  decrease.textContent = '−';
  const output = document.createElement('span');
  output.className = 'matrix-stepper-value';
  output.textContent = String(value);
  const increase = document.createElement('button');
  increase.type = 'button';
  increase.textContent = '+';
  for (const button of [decrease, increase]) button.addEventListener('pointerdown', event => event.preventDefault());
  decrease.addEventListener('click', event => {
    event.preventDefault();
    onChange(Math.max(minimum, Number(value) - 1));
  });
  increase.addEventListener('click', event => {
    event.preventDefault();
    onChange(Math.min(maximum, Number(value) + 1));
  });
  controls.append(decrease, output, increase);
  row.append(label, controls);
  return row;
}

function renderGuidedScientificForm(kind, data, handlers) {
  const form = document.createElement('form');
  form.className = `scientific-guided-form scientific-guided-${kind}`;
  form.setAttribute('aria-label', `${GUIDED_STRUCTURE_LABELS[kind] || 'Scientific'} guided form`);
  form.addEventListener('submit', event => event.preventDefault());
  if (guidedScientificRowLimitExceeded(kind, data)) {
    const guidance = document.createElement('p');
    guidance.className = 'matrix-grid-guidance';
    guidance.textContent = `This structure has more than ${SCIENTIFIC_GUIDED_ROW_LIMIT} rows. Continue in Advanced source to edit it without truncation.`;
    form.appendChild(guidance);
    return form;
  }
  const field = (label, key, options = {}) => guidedField(label, data[key], {
    registerInput: handlers.registerInput,
    input: value => {
      data[key] = value;
      handlers.changed(false);
    },
  }, {...options, path: [key]});
  if (kind === 'expression') {
    form.appendChild(field('Expression', 'expression', {wide: true, multiline: true, rows: 3, placeholder: 'E = mc^2'}).label);
    return form;
  }
  if (kind === 'chemistry') {
    const kindLabel = document.createElement('span');
    kindLabel.className = 'scientific-guided-section-label';
    kindLabel.textContent = 'Notation';
    const command = document.createElement('div');
    command.className = 'scientific-chemistry-kind';
    for (const [value, label] of [['ce', 'Formula or reaction'], ['pu', 'Physical unit']]) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = label;
      button.classList.toggle('active', data.command === value);
      button.setAttribute('aria-pressed', data.command === value ? 'true' : 'false');
      button.addEventListener('pointerdown', event => event.preventDefault());
      button.addEventListener('click', event => {
        event.preventDefault();
        data.command = value;
        handlers.changed(true);
      });
      command.appendChild(button);
    }
    form.append(
      kindLabel,
      command,
      field(data.command === 'pu' ? 'Unit' : 'Formula or reaction', 'value', {
        wide: true,
        multiline: true,
        rows: 3,
        placeholder: data.command === 'pu' ? 'kg m s^{-2}' : '2H2 + O2 -> 2H2O',
      }).label,
    );
    return form;
  }
  if (kind === 'scripts') {
    form.append(
      field('Base', 'base', {wide: true, placeholder: 'X'}).label,
      field('Left superscript', 'leftSuperscript', {placeholder: 'a'}).label,
      field('Right superscript', 'superscript', {placeholder: 'c'}).label,
      field('Left subscript', 'leftSubscript', {placeholder: 'b'}).label,
      field('Right subscript', 'subscript', {placeholder: 'd'}).label,
    );
    return form;
  }
  if (kind === 'fraction') {
    form.append(
      field('Numerator', 'numerator', {wide: true, placeholder: 'a'}).label,
      field('Denominator', 'denominator', {wide: true, placeholder: 'b'}).label,
    );
    return form;
  }
  if (kind === 'root') {
    form.append(
      field('Value under root', 'radicand', {wide: true, placeholder: 'x'}).label,
      field('Index (optional)', 'index', {placeholder: '3'}).label,
    );
    return form;
  }
  if (kind === 'integral') {
    const operatorLabel = document.createElement('span');
    operatorLabel.className = 'scientific-guided-section-label scientific-guided-wide';
    operatorLabel.textContent = 'Integral type';
    const operator = document.createElement('div');
    operator.className = 'scientific-integral-kind scientific-guided-wide';
    for (const [value, label] of SCIENTIFIC_INTEGRAL_OPERATORS) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = label;
      button.classList.toggle('active', data.operator === value);
      button.setAttribute('aria-pressed', data.operator === value ? 'true' : 'false');
      button.addEventListener('pointerdown', event => event.preventDefault());
      button.addEventListener('click', event => {
        event.preventDefault();
        data.operator = value;
        handlers.changed(true);
      });
      operator.appendChild(button);
    }
    form.append(
      operatorLabel,
      operator,
      field('Lower limit', 'lower', {placeholder: '0'}).label,
      field('Upper limit', 'upper', {placeholder: '1'}).label,
      field('Integrand', 'integrand', {wide: true, placeholder: 'x^2'}).label,
      field('Variable', 'variable', {placeholder: 'x'}).label,
    );
    return form;
  }
  if (kind === 'summation' || kind === 'product') {
    form.append(
      field('Lower limit', 'lower', {placeholder: 'i=1'}).label,
      field('Upper limit', 'upper', {placeholder: 'n'}).label,
      field(kind === 'product' ? 'Factors' : 'Terms', 'expression', {wide: true, placeholder: 'x_i'}).label,
    );
    return form;
  }
  if (kind === 'matrix') {
    const matrix = ensureMatrixDimensions(data, data.rows, data.columns);
    Object.assign(data, matrix);
    const dimensions = document.createElement('div');
    dimensions.className = 'scientific-matrix-dimensions';
    dimensions.append(
      guidedStepper('Rows', data.rows, 1, MATRIX_DIMENSION_MAX, value => {
        Object.assign(data, ensureMatrixDimensions(data, value, data.columns));
        handlers.changed(true);
      }),
      guidedStepper('Columns', data.columns, 1, MATRIX_DIMENSION_MAX, value => {
        Object.assign(data, ensureMatrixDimensions(data, data.rows, value));
        handlers.changed(true);
      }),
    );
    const bracketLabel = document.createElement('span');
    bracketLabel.className = 'scientific-guided-section-label';
    bracketLabel.textContent = 'Brackets';
    const brackets = document.createElement('div');
    brackets.className = 'scientific-matrix-brackets';
    for (const [environment, label] of MATRIX_ENVIRONMENTS) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = label;
      button.classList.toggle('active', data.environment === environment);
      button.setAttribute('aria-pressed', data.environment === environment ? 'true' : 'false');
      button.addEventListener('pointerdown', event => event.preventDefault());
      button.addEventListener('click', event => {
        event.preventDefault();
        data.environment = environment;
        handlers.changed(true);
      });
      brackets.appendChild(button);
    }
    const grid = document.createElement('div');
    grid.className = 'scientific-matrix-grid';
    grid.style.setProperty('--matrix-columns', String(data.columns));
    data.cells.forEach((row, rowIndex) => row.forEach((cell, columnIndex) => {
      const input = document.createElement('input');
      input.type = 'text';
      input.value = cell;
      input.placeholder = `${rowIndex + 1},${columnIndex + 1}`;
      input.setAttribute('aria-label', `Matrix row ${rowIndex + 1}, column ${columnIndex + 1}`);
      input.autocapitalize = 'off';
      input.autocomplete = 'off';
      input.spellcheck = false;
      handlers.registerInput(input, ['cells', rowIndex, columnIndex]);
      input.addEventListener('input', () => {
        data.cells[rowIndex][columnIndex] = input.value;
        handlers.changed(false);
      });
      input.addEventListener('paste', event => {
        const text = event.clipboardData?.getData('text/plain') || '';
        if (!/[\t\n]/.test(text)) return;
        event.preventDefault();
        const pasted = text.replace(/\r/g, '').split('\n').filter((line, index, all) => line !== '' || index < all.length - 1).map(line => line.split('\t'));
        const rows = Math.min(MATRIX_DIMENSION_MAX, Math.max(data.rows, rowIndex + pasted.length));
        const columns = Math.min(MATRIX_DIMENSION_MAX, Math.max(data.columns, columnIndex + Math.max(1, ...pasted.map(values => values.length))));
        Object.assign(data, ensureMatrixDimensions(data, rows, columns));
        pasted.forEach((values, rowOffset) => values.forEach((value, columnOffset) => {
          if (rowIndex + rowOffset < data.rows && columnIndex + columnOffset < data.columns) {
            data.cells[rowIndex + rowOffset][columnIndex + columnOffset] = value;
          }
        }));
        handlers.changed(true);
      });
      grid.appendChild(input);
    }));
    const guidance = document.createElement('p');
    guidance.className = 'matrix-grid-guidance';
    guidance.textContent = 'Tab moves between cells. You can paste rows and columns from a spreadsheet.';
    form.append(dimensions, bracketLabel, brackets, grid, guidance);
    return form;
  }
  if (kind === 'cases') {
    const header = document.createElement('div');
    header.className = 'scientific-cases-header';
    header.append(Object.assign(document.createElement('span'), {textContent: 'Expression'}), Object.assign(document.createElement('span'), {textContent: 'Condition'}));
    form.appendChild(header);
    data.rows.forEach((row, index) => {
      const item = document.createElement('div');
      item.className = 'scientific-cases-row';
      const expression = guidedField('', row.expression, {
        registerInput: handlers.registerInput,
        input: value => { row.expression = value; handlers.changed(false); },
      }, {placeholder: 'x^2', path: ['rows', index, 'expression']}).input;
      const condition = guidedField('', row.condition, {
        registerInput: handlers.registerInput,
        input: value => { row.condition = value; handlers.changed(false); },
      }, {placeholder: 'x \\ge 0', path: ['rows', index, 'condition']}).input;
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.textContent = '−';
      remove.setAttribute('aria-label', `Remove case ${index + 1}`);
      remove.disabled = data.rows.length <= 1;
      remove.addEventListener('pointerdown', event => event.preventDefault());
      remove.addEventListener('click', event => {
        event.preventDefault();
        if (data.rows.length > 1) data.rows.splice(index, 1);
        handlers.changed(true);
      });
      item.append(expression, condition, remove);
      form.appendChild(item);
    });
    const add = document.createElement('button');
    add.type = 'button';
    add.className = 'scientific-guided-add-row';
    const atRowLimit = data.rows.length >= SCIENTIFIC_GUIDED_ROW_LIMIT;
    add.disabled = atRowLimit;
    add.textContent = atRowLimit ? `Maximum ${SCIENTIFIC_GUIDED_ROW_LIMIT} cases` : '+ Add case';
    add.title = atRowLimit ? 'Use Advanced source for larger case structures.' : '';
    add.addEventListener('click', event => {
      event.preventDefault();
      if (data.rows.length >= SCIENTIFIC_GUIDED_ROW_LIMIT) return;
      data.rows.push({expression: '', condition: ''});
      handlers.changed(true);
    });
    form.appendChild(add);
    return form;
  }
  if (kind === 'aligned') {
    const header = document.createElement('div');
    header.className = 'scientific-aligned-header';
    for (const label of ['Left side', 'Relation', 'Right side']) {
      header.appendChild(Object.assign(document.createElement('span'), {textContent: label}));
    }
    form.appendChild(header);
    data.rows.forEach((row, index) => {
      if (typeof row === 'string') data.rows[index] = {left: row, relation: '=', right: ''};
      const current = data.rows[index];
      const item = document.createElement('div');
      item.className = 'scientific-aligned-row';
      const left = guidedField('', current.left, {
        registerInput: handlers.registerInput,
        input: value => { current.left = value; handlers.changed(false); },
      }, {placeholder: 'a', path: ['rows', index, 'left']}).input;
      const relation = document.createElement('select');
      relation.setAttribute('aria-label', `Relation for line ${index + 1}`);
      for (const [value, label] of SCIENTIFIC_ALIGNED_RELATIONS) {
        const option = document.createElement('option');
        option.value = value;
        option.textContent = label;
        relation.appendChild(option);
      }
      relation.value = normalizedAlignedRelation(current.relation);
      relation.addEventListener('change', () => {
        current.relation = relation.value;
        handlers.changed(false);
      });
      const right = guidedField('', current.right, {
        registerInput: handlers.registerInput,
        input: value => { current.right = value; handlers.changed(false); },
      }, {placeholder: 'b', path: ['rows', index, 'right']}).input;
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.textContent = '−';
      remove.setAttribute('aria-label', `Remove line ${index + 1}`);
      remove.disabled = data.rows.length <= 1;
      remove.addEventListener('pointerdown', event => event.preventDefault());
      remove.addEventListener('click', event => {
        event.preventDefault();
        if (data.rows.length > 1) data.rows.splice(index, 1);
        handlers.changed(true);
      });
      item.append(left, relation, right, remove);
      form.appendChild(item);
    });
    const add = document.createElement('button');
    add.type = 'button';
    add.className = 'scientific-guided-add-row';
    const atRowLimit = data.rows.length >= SCIENTIFIC_GUIDED_ROW_LIMIT;
    add.disabled = atRowLimit;
    add.textContent = atRowLimit ? `Maximum ${SCIENTIFIC_GUIDED_ROW_LIMIT} lines` : '+ Add line';
    add.title = atRowLimit ? 'Use Advanced source for larger aligned structures.' : '';
    add.addEventListener('click', event => {
      event.preventDefault();
      if (data.rows.length >= SCIENTIFIC_GUIDED_ROW_LIMIT) return;
      data.rows.push({left: '', relation: '=', right: ''});
      handlers.changed(true);
    });
    form.appendChild(add);
    return form;
  }
  return form;
}
export {GUIDED_STRUCTURE_LABELS, ScientificMathView, TaskItemView, activeScientificInsertionTarget, adjacentOrderedContext, ancestorDepth, atomicBoundaryParagraph, autoLinkMatch, blockStyleLabel, clamp, clampDocumentPosition, cloneGuidedData, commonSelectedListContext, continueOrderedNumbering, currentTableContext, defaultGuidedScientificData, deleteSelectedTable, displayScientificToInlineTransaction, documentIsEmpty, editorEnvelope, guidedScientificEligibility, guidedScientificSource, guidedScientificUnavailableMessage, inlineScientificToDisplayTransaction, insertGuidedValueAtPath, insertIntoTextarea, normalizeAtomicBoundaryDocument, orderedListAttrs, plugins, renderGuidedScientificForm, renderScientificSource, replaceTableWithParagraph, resolveGuidedStructure, restartOrderedNumbering, scientificBoundarySelection, scientificComposerInsertionMode, scientificNodeContext, scientificNodeMarkdown, scientificNodeText, scientificTemplateCaretOffset, scientificToolGroupsPanel, selectedBlockStyle, selectedLinkContext, selectedListContexts, selectedListType, selectedLayoutTextblocks, selectedMarkAttribute, selectedTextblockLayoutValue, selectedTextblocks, selectionIsInsideNodeType, selectionUsesMark, splitTextblockWithParagraphDefaults, splitTextblockWithFirstLineIndent, clearFirstLineIndentOnBackspace, paragraphLayoutStatePlugin, normalizePastedIndentSlice, normalizePastedIndentText, textblockLayoutAttrs, syncTaskCheckboxInteractionState, tableDomContext, tableHasHeaderRow, updateToolbarState};

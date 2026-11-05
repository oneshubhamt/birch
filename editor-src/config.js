import {PluginKey} from "prosemirror-state";
import {nodeNamesForValidator, specDomain, specLimit} from "./document/spec.js";




const RUNTIME_MARKER = __RUSTNOTE_RUNTIME_MARKER__;
const ROOT_ID = 'note-prosemirror-editor';
const EDITOR_BRIDGE_PROTOCOL_VERSION = 1;
const EVENT_BRIDGE_ID = 'note-editor-event-bridge';
const MAX_TRANSIENT_IMAGE_IMPORT_BYTES = 64 * 1024 * 1024;
const EDITOR_SYNC_DELAY_MS = 900;
const EDITOR_SYNC_MAX_DELAY_MS = 2200;

const preservedSelectionKey = new PluginKey('rustnote-preserved-selection');
const placeholderKey = new PluginKey('rustnote-placeholder');
const tableBoundaryKey = new PluginKey('rustnote-table-boundary');
const scientificSelectionKey = new PluginKey('rustnote-scientific-selection');
const linkPasteKey = new PluginKey('rustnote-link-paste');
const findHighlightKey = new PluginKey('rustnote-find-highlight');


const FLOATING_TOOLBAR_ID = 'rustnote-floating-selection-toolbar';
const LINK_CONTEXT_MENU_ID = 'rustnote-link-context-menu';
const ATTACHMENT_CONTEXT_MENU_ID = 'rustnote-attachment-context-menu';
const CODE_CONTEXT_MENU_ID = 'rustnote-code-context-menu';
const INSERT_MENU_ID = 'rustnote-insert-menu';
const INSERT_TRIGGER_ID = 'editor-insert-trigger';
const LIST_CONTEXT_ID = 'editor-list-context-tools';
const TABLE_TOOLBAR_ID = 'rustnote-table-toolbar';
const TABLE_LEVEL_CONTROL_ID = 'rustnote-table-level-control';
const TABLE_ROW_CONTROL_ID = 'rustnote-table-row-control';
const TABLE_COLUMN_CONTROL_ID = 'rustnote-table-column-control';
const TABLE_ACTION_MENU_ID = 'rustnote-table-action-menu';
const TABLE_GRID_SIZE = 8;
const MATRIX_DIMENSION_MAX = 12;
const TABLE_MIN_COLUMN_WIDTH = 80;
const TABLE_DEFAULT_COLUMN_WIDTH = 120;
const VIEWPORT_MARGIN = 10;
const SURFACE_GAP = 8;
const FONT_SIZE_MIN = specLimit('font_size_min');
const FONT_SIZE_MAX = specLimit('font_size_max');
const FONT_FAMILIES = specDomain('font_family');
const TEXT_COLORS = specDomain('text_color_ui');
const TEXT_COLOR_LABELS = Object.freeze({default: 'Default', grey: 'Grey', red: 'Red', orange: 'Orange', green: 'Green', blue: 'Blue', purple: 'Purple', mixed: 'Mixed'});
const HIGHLIGHT_COLORS = specDomain('highlight_color');
const LINE_SPACINGS = specDomain('line_spacing');
const TEXT_ALIGNMENTS = specDomain('text_alignment');
const PARAGRAPH_SIDE_INDENT_MAX = specLimit('paragraph_side_indent_max');
const PARAGRAPH_FIRST_LINE_INDENT_MAX = specLimit('paragraph_first_line_indent_max');
const PARAGRAPH_INDENT_STEP = 18;
const DEFAULT_AUTO_FIRST_LINE_INDENT = 18;
const AUTO_INDENT_SUPPRESSION_META = 'rustnote-auto-indent-suppression';
const ORDERED_LIST_STYLES = specDomain('ordered_list_style');
const BULLET_LIST_MARKERS = specDomain('bullet_list_marker');
const DEFAULT_FONT_SIZE_POINTS = Object.freeze({paragraph: 11, h1: 23, h2: 17, h3: 14, quote: 11, code: 11, list: 11, mixed: 11});
const SCIENTIFIC_NODE_TYPES = nodeNamesForValidator('scientific');
const SCIENTIFIC_TEMPLATE_GROUPS = Object.freeze([
  {
    id: 'basic',
    label: 'Basic',
    items: [
      ['x²', null, 'guided:superscript'], ['xᵢ', null, 'guided:subscript'], ['ᵃXᵦ', null, 'guided:scripts'],
      ['Fraction', null, 'guided:fraction'], ['Root', null, 'guided:root'], ['Scalable brackets', '\\left( \\right)'],
    ],
  },
  {
    id: 'calculus',
    label: 'Calculus',
    items: [
      ['Integral', null, 'guided:integral'], ['Double integral', '\\iint'], ['Triple integral', '\\iiint'],
      ['Contour integral', '\\oint'], ['Partial', '\\partial'], ['Derivative', '\\frac{d}{dx}'],
      ['Limit', '\\lim_{x \\to }'], ['Summation', null, 'guided:summation'], ['Product', null, 'guided:product'],
      ['Nabla', '\\nabla'],
    ],
  },
  {
    id: 'structures',
    label: 'Structures',
    items: [
      ['Matrix…', null, 'guided:matrix'], ['Determinant', null, 'guided:determinant'],
      ['Cases', null, 'guided:cases'],
      ['Aligned equations', null, 'guided:aligned'],
      ['Vector', '\\vec{}'],
    ],
  },
  {
    id: 'chemistry',
    label: 'Chemistry',
    items: [
      ['Formula', '\\ce{}'], ['Reaction', '\\ce{A -> B}'], ['Reversible', '\\ce{A <-> B}'],
      ['Equilibrium', '\\ce{A <=> B}'], ['Charge', '\\ce{Fe^3+}'],
      ['Isotope', '\\ce{^{238}_{92}U}'], ['State symbol', '\\ce{H2O(l)}'],
      ['Arrow conditions', '\\ce{A ->[heat][catalyst] B}'], ['Physical unit', '\\pu{}'],
    ],
  },
]);
const SCIENTIFIC_SYMBOL_GROUP_DEFINITIONS = Object.freeze([
  ['greek', 'Greek'],
  ['relations', 'Relations, logic and sets'],
  ['arrows', 'Arrows'],
]);

// Canonical scientific notation registry. Palette presentation, Unicode
// normalization, smart-paste recognition and MathML operator conversion are
// all derived from this table so supported notation cannot silently drift
// between independent regexes/maps.
const SCIENTIFIC_NOTATION_REGISTRY = Object.freeze([
  // Greek. Unicode Greek letters are contextual rather than intrinsically
  // scientific: "α = β" is scientific, while an isolated Greek character
  // may still be ordinary prose.
  {id: 'alpha', latex: '\\alpha', unicode: ['α'], palette: ['greek', 'α'], paste: 'contextual'},
  {id: 'beta', latex: '\\beta', unicode: ['β'], palette: ['greek', 'β'], paste: 'contextual'},
  {id: 'gamma', latex: '\\gamma', unicode: ['γ'], palette: ['greek', 'γ'], paste: 'contextual'},
  {id: 'delta', latex: '\\delta', unicode: ['δ'], palette: ['greek', 'δ'], paste: 'contextual'},
  {id: 'epsilon', latex: '\\epsilon', unicode: ['ε'], palette: ['greek', 'ε'], paste: 'contextual'},
  {id: 'zeta', latex: '\\zeta', unicode: ['ζ'], palette: ['greek', 'ζ'], paste: 'contextual'},
  {id: 'eta', latex: '\\eta', unicode: ['η'], palette: ['greek', 'η'], paste: 'contextual'},
  {id: 'theta', latex: '\\theta', unicode: ['θ'], palette: ['greek', 'θ'], paste: 'contextual'},
  {id: 'iota', latex: '\\iota', unicode: ['ι'], palette: ['greek', 'ι'], paste: 'contextual'},
  {id: 'kappa', latex: '\\kappa', unicode: ['κ'], palette: ['greek', 'κ'], paste: 'contextual'},
  {id: 'lambda', latex: '\\lambda', unicode: ['λ'], palette: ['greek', 'λ'], paste: 'contextual'},
  {id: 'mu', latex: '\\mu', unicode: ['μ'], palette: ['greek', 'μ'], paste: 'contextual'},
  {id: 'nu', latex: '\\nu', unicode: ['ν'], palette: ['greek', 'ν'], paste: 'contextual'},
  {id: 'xi', latex: '\\xi', unicode: ['ξ'], palette: ['greek', 'ξ'], paste: 'contextual'},
  {id: 'pi', latex: '\\pi', unicode: ['π'], palette: ['greek', 'π'], paste: 'contextual'},
  {id: 'rho', latex: '\\rho', unicode: ['ρ'], palette: ['greek', 'ρ'], paste: 'contextual'},
  {id: 'sigma', latex: '\\sigma', unicode: ['σ'], palette: ['greek', 'σ'], paste: 'contextual'},
  {id: 'tau', latex: '\\tau', unicode: ['τ'], palette: ['greek', 'τ'], paste: 'contextual'},
  {id: 'upsilon', latex: '\\upsilon', unicode: ['υ'], palette: ['greek', 'υ'], paste: 'contextual'},
  {id: 'phi', latex: '\\phi', unicode: ['φ'], palette: ['greek', 'φ'], paste: 'contextual'},
  {id: 'chi', latex: '\\chi', unicode: ['χ'], palette: ['greek', 'χ'], paste: 'contextual'},
  {id: 'psi', latex: '\\psi', unicode: ['ψ'], palette: ['greek', 'ψ'], paste: 'contextual'},
  {id: 'omega', latex: '\\omega', unicode: ['ω'], palette: ['greek', 'ω'], paste: 'contextual'},
  {id: 'Gamma', latex: '\\Gamma', unicode: ['Γ'], palette: ['greek', 'Γ'], paste: 'contextual'},
  {id: 'Delta', latex: '\\Delta', unicode: ['Δ'], palette: ['greek', 'Δ'], paste: 'contextual'},
  {id: 'Theta', latex: '\\Theta', unicode: ['Θ'], palette: ['greek', 'Θ'], paste: 'contextual'},
  {id: 'Lambda', latex: '\\Lambda', unicode: ['Λ'], palette: ['greek', 'Λ'], paste: 'contextual'},
  {id: 'Xi', latex: '\\Xi', unicode: ['Ξ'], palette: ['greek', 'Ξ'], paste: 'contextual'},
  {id: 'Pi', latex: '\\Pi', unicode: ['Π'], palette: ['greek', 'Π'], paste: 'contextual'},
  {id: 'Sigma', latex: '\\Sigma', unicode: ['Σ'], palette: ['greek', 'Σ'], paste: 'contextual'},
  {id: 'Phi', latex: '\\Phi', unicode: ['Φ'], palette: ['greek', 'Φ'], paste: 'contextual'},
  {id: 'Psi', latex: '\\Psi', unicode: ['Ψ'], palette: ['greek', 'Ψ'], paste: 'contextual'},
  {id: 'Omega', latex: '\\Omega', unicode: ['Ω'], palette: ['greek', 'Ω'], paste: 'contextual'},

  // Relations, logic and sets.
  {id: 'equals', latex: '=', unicode: ['='], palette: ['relations', '='], paste: 'contextual', aligned: '=', alignedOrder: 0},
  {id: 'not-equal', latex: '\\ne', latexAliases: ['\\neq'], unicode: ['≠'], palette: ['relations', '≠'], paste: 'strong', aligned: '≠', alignedOrder: 3},
  {id: 'less-equal', latex: '\\le', latexAliases: ['\\leq'], unicode: ['≤'], palette: ['relations', '≤'], paste: 'strong', aligned: '≤', alignedOrder: 1},
  {id: 'greater-equal', latex: '\\ge', latexAliases: ['\\geq'], unicode: ['≥'], palette: ['relations', '≥'], paste: 'strong', aligned: '≥', alignedOrder: 2},
  {id: 'approx', latex: '\\approx', unicode: ['≈'], palette: ['relations', '≈'], paste: 'strong', aligned: '≈', alignedOrder: 4},
  {id: 'similar', latex: '\\sim', unicode: ['∼'], paste: 'strong', aligned: '∼', alignedOrder: 5},
  {id: 'equivalent', latex: '\\equiv', unicode: ['≡'], palette: ['relations', '≡'], paste: 'strong', aligned: '≡', alignedOrder: 6},
  {id: 'plus-minus', latex: '\\pm', unicode: ['±'], palette: ['relations', '±'], paste: 'strong'},
  {id: 'times', latex: '\\times', unicode: ['×'], palette: ['relations', '×'], paste: 'strong'},
  {id: 'divide', latex: '\\div', unicode: ['÷'], paste: 'strong'},
  {id: 'cdot', latex: '\\cdot', unicode: ['·', '⋅'], palette: ['relations', '·'], paste: 'strong'},
  {id: 'infinity', latex: '\\infty', unicode: ['∞'], palette: ['relations', '∞'], paste: 'strong'},
  {id: 'proportional', latex: '\\propto', unicode: ['∝'], palette: ['relations', '∝'], paste: 'strong', aligned: '∝', alignedOrder: 7},
  {id: 'member', latex: '\\in', unicode: ['∈'], palette: ['relations', '∈'], paste: 'strong'},
  {id: 'not-member', latex: '\\notin', unicode: ['∉'], palette: ['relations', '∉'], paste: 'strong'},
  {id: 'subset', latex: '\\subset', unicode: ['⊂'], palette: ['relations', '⊂'], paste: 'strong'},
  {id: 'subset-equal', latex: '\\subseteq', unicode: ['⊆'], palette: ['relations', '⊆'], paste: 'strong'},
  {id: 'union', latex: '\\cup', unicode: ['∪'], palette: ['relations', '∪'], paste: 'strong'},
  {id: 'intersection', latex: '\\cap', unicode: ['∩'], palette: ['relations', '∩'], paste: 'strong'},
  {id: 'forall', latex: '\\forall', unicode: ['∀'], palette: ['relations', '∀'], paste: 'strong'},
  {id: 'exists', latex: '\\exists', unicode: ['∃'], palette: ['relations', '∃'], paste: 'strong'},
  {id: 'empty-set', latex: '\\varnothing', unicode: ['∅'], palette: ['relations', '∅'], paste: 'strong'},
  {id: 'perpendicular', latex: '\\perp', unicode: ['⊥'], palette: ['relations', '⊥'], paste: 'strong'},
  {id: 'parallel', latex: '\\parallel', unicode: ['∥'], palette: ['relations', '∥'], paste: 'strong'},
  {id: 'tensor-product', latex: '\\otimes', unicode: ['⊗'], paste: 'strong'},
  {id: 'direct-sum', latex: '\\oplus', unicode: ['⊕'], paste: 'strong'},

  // Arrows.
  {id: 'to', latex: '\\to', latexAliases: ['\\rightarrow'], unicode: ['→'], palette: ['arrows', '→'], paste: 'strong'},
  {id: 'left-arrow', latex: '\\leftarrow', unicode: ['←'], palette: ['arrows', '←'], paste: 'strong'},
  {id: 'left-right-arrow', latex: '\\leftrightarrow', unicode: ['↔'], palette: ['arrows', '↔'], paste: 'strong'},
  {id: 'right-double-arrow', latex: '\\Rightarrow', unicode: ['⇒'], palette: ['arrows', '⇒'], paste: 'strong'},
  {id: 'left-double-arrow', latex: '\\Leftarrow', unicode: ['⇐'], palette: ['arrows', '⇐'], paste: 'strong'},
  {id: 'left-right-double-arrow', latex: '\\Leftrightarrow', unicode: ['⇔'], palette: ['arrows', '⇔'], paste: 'strong'},
  {id: 'maps-to', latex: '\\mapsto', unicode: ['↦'], palette: ['arrows', '↦'], paste: 'strong'},
  {id: 'north-east-arrow', latex: '\\nearrow', unicode: ['↗'], palette: ['arrows', '↗'], paste: 'strong'},
  {id: 'south-east-arrow', latex: '\\searrow', unicode: ['↘'], palette: ['arrows', '↘'], paste: 'strong'},
  {id: 'right-labelled-arrow', latex: '\\xrightarrow', paletteSource: '\\xrightarrow{}', palette: ['arrows', '→ label'], paste: 'explicit'},
  {id: 'left-labelled-arrow', latex: '\\xleftarrow', paletteSource: '\\xleftarrow{}', palette: ['arrows', '← label'], paste: 'explicit'},

  // Common scientific Unicode that is supported outside the visible symbol
  // palette but participates in paste/import normalization.
  {id: 'square-root', latex: '\\sqrt{}', latexCommand: '\\sqrt', unicode: ['√'], paste: 'strong'},
  {id: 'summation', latex: '\\sum', unicode: ['∑'], paste: 'strong'},
  {id: 'product', latex: '\\prod', unicode: ['∏'], paste: 'strong'},
  {id: 'integral', latex: '\\int', unicode: ['∫'], paste: 'strong'},
  {id: 'partial', latex: '\\partial', unicode: ['∂'], paste: 'strong'},
  {id: 'nabla', latex: '\\nabla', unicode: ['∇'], paste: 'strong'},
]);

const SCIENTIFIC_NOTATION_BY_UNICODE = new Map();
const SCIENTIFIC_NOTATION_BY_LATEX_COMMAND = new Map();
for (const entry of SCIENTIFIC_NOTATION_REGISTRY) {
  for (const alias of entry.unicode || []) SCIENTIFIC_NOTATION_BY_UNICODE.set(alias, entry);
  for (const latex of [entry.latexCommand || entry.latex, ...(entry.latexAliases || [])]) {
    const match = /^\\([A-Za-z]+)$/.exec(latex);
    if (match) SCIENTIFIC_NOTATION_BY_LATEX_COMMAND.set(match[1], entry);
  }
}

const SCIENTIFIC_SYMBOL_GROUPS = Object.freeze(SCIENTIFIC_SYMBOL_GROUP_DEFINITIONS.map(([id, label]) => ({
  id,
  label,
  items: SCIENTIFIC_NOTATION_REGISTRY
    .filter(entry => entry.palette && entry.palette[0] === id)
    .map(entry => [entry.palette[1], entry.paletteSource || entry.latex]),
})));
const SCIENTIFIC_RENDER_OPTIONS = Object.freeze({
  throwOnError: false,
  trust: false,
  strict: 'warn',
  output: 'htmlAndMathml',
});
const SCIENTIFIC_CLIPBOARD_MIME = 'application/x-rustnote-scientific+json';
const SCIENTIFIC_CLIPBOARD_VERSION = 1;
const SCIENTIFIC_PREVIEW_DELAY_MS = 72;
const SCIENTIFIC_RENDER_ROOT_MARGIN = 320;
const SCIENTIFIC_RENDER_VISIBLE_BATCH = 10;
const SCIENTIFIC_RENDER_FALLBACK_BATCH = 4;
const SCIENTIFIC_RENDER_SOURCE_LIMIT = 16 * 1024;
const SCIENTIFIC_RENDER_EXPANDED_SOURCE_LIMIT = 128 * 1024;
const SCIENTIFIC_GUIDED_ROW_LIMIT = 64;
const SCIENTIFIC_TOOLS_TITLE_ID = `${INSERT_MENU_ID}-scientific-tools-title`;
const SCIENTIFIC_TOOLS_DESCRIPTION_ID = `${INSERT_MENU_ID}-scientific-tools-description`;
const SCIENTIFIC_COMPOSER_TITLE_ID = `${INSERT_MENU_ID}-scientific-composer-title`;
const SCIENTIFIC_COMPOSER_DESCRIPTION_ID = `${INSERT_MENU_ID}-scientific-composer-description`;

const CONTENT_INSERT_COMMANDS = Object.freeze([
  {
    id: 'equation',
    label: 'Equation…',
    description: 'Paste or type an inline or display equation',
    keywords: ['math', 'latex', 'equation', 'scientific', 'formula'],
    action: 'equation_composer',
    icon: '\u2211',
  },
  {
    id: 'chemical-equation',
    label: 'Chemical equation…',
    description: 'Paste or type a chemical formula or reaction',
    keywords: ['chemistry', 'reaction', 'mhchem', 'chemical'],
    action: 'chemical_composer',
    icon: '\u2697',
  },
  {
    id: 'matrix',
    label: 'Matrix…',
    description: 'Choose any row-by-column matrix size',
    keywords: ['matrix', 'rows', 'columns', 'scientific', 'math'],
    action: 'matrix',
    icon: '[ ]',
  },
  {
    id: 'table',
    label: 'Table…',
    description: 'Choose rows, columns and an optional header',
    keywords: ['grid', 'rows', 'columns', 'cells'],
    action: 'table',
    icon: '\u25a6',
  },
  {
    id: 'note-link',
    label: 'Note link…',
    description: 'Link to another Rustnote note',
    keywords: ['note', 'link', 'backlink', 'wiki', 'reference'],
    action: 'note_link',
    icon: '\u2197',
  },
  {
    id: 'image-attachment',
    label: 'Image…',
    description: 'Insert one or more images from this device',
    keywords: ['image', 'photo', 'picture', 'attachment'],
    action: 'attachment_image',
    icon: 'IMG',
  },
  {
    id: 'pdf-attachment',
    label: 'PDF…',
    description: 'Insert one or more PDF documents',
    keywords: ['pdf', 'document', 'attachment'],
    action: 'attachment_pdf',
    icon: 'PDF',
  },
  {
    id: 'audio-attachment',
    label: 'Audio…',
    description: 'Insert one or more audio files',
    keywords: ['audio', 'music', 'recording', 'attachment'],
    action: 'attachment_audio',
    icon: 'AUD',
  },
  {
    id: 'video-attachment',
    label: 'Video…',
    description: 'Insert one or more videos',
    keywords: ['video', 'movie', 'attachment'],
    action: 'attachment_video',
    icon: 'VID',
  },
  {
    id: 'file-attachment',
    label: 'File…',
    description: 'Insert any file as an embedded attachment card',
    keywords: ['file', 'document', 'attachment'],
    action: 'attachment_file',
    icon: 'FILE',
  },
]);

const INSERT_COMMANDS = Object.freeze([
  {
    id: 'code-block',
    label: 'Code block',
    description: 'Create or remove a code block',
    keywords: ['code', 'preformatted', 'programming'],
    action: 'code',
    icon: '</>',
  },
  {
    id: 'link',
    label: 'Link…',
    description: 'Add or edit a web link',
    keywords: ['link', 'url', 'web', 'hyperlink'],
    action: 'link',
    icon: '\u2197',
  },
  ...CONTENT_INSERT_COMMANDS,
]);

const SLASH_COMMANDS = Object.freeze([
  {
    id: 'text', label: 'Text', description: 'Convert the current block to normal text',
    keywords: ['paragraph', 'normal', 'body'], action: 'paragraph', icon: 'T',
  },
  {
    id: 'heading-1', label: 'Heading 1', description: 'Large section heading',
    keywords: ['h1', 'title', 'section'], action: 'heading1', icon: 'H1',
  },
  {
    id: 'heading-2', label: 'Heading 2', description: 'Medium section heading',
    keywords: ['h2', 'subheading', 'section'], action: 'heading2', icon: 'H2',
  },
  {
    id: 'heading-3', label: 'Heading 3', description: 'Small section heading',
    keywords: ['h3', 'subheading'], action: 'heading3', icon: 'H3',
  },
  {
    id: 'bullet-list', label: 'Bullet list', description: 'Create or convert a bulleted list',
    keywords: ['unordered', 'list', 'bullets'], action: 'bullet', icon: '\u2022',
  },
  {
    id: 'numbered-list', label: 'Numbered list', description: 'Create or convert an ordered list',
    keywords: ['ordered', 'list', 'numbers'], action: 'numbered', icon: '1.',
  },
  {
    id: 'checklist', label: 'Checklist', description: 'Create a list of tasks',
    keywords: ['task', 'todo', 'checkbox'], action: 'checklist', icon: '\u2610',
  },
  {
    id: 'quote', label: 'Quote', description: 'Create or remove a quotation block',
    keywords: ['blockquote', 'citation'], action: 'quote', icon: '\u275d',
  },
  {
    id: 'code-block', label: 'Code block', description: 'Create or remove a code block',
    keywords: ['code', 'preformatted', 'programming'], action: 'code', icon: '</>',
  },
  ...CONTENT_INSERT_COMMANDS,
  {
    id: 'horizontal-rule', label: 'Horizontal rule', description: 'Insert a quiet document divider',
    keywords: ['divider', 'separator', 'line', 'hr'], action: 'horizontal_rule', icon: '\u2014',
  },
]);
export {ATTACHMENT_CONTEXT_MENU_ID, AUTO_INDENT_SUPPRESSION_META, BULLET_LIST_MARKERS, CODE_CONTEXT_MENU_ID, DEFAULT_FONT_SIZE_POINTS, EDITOR_BRIDGE_PROTOCOL_VERSION, EDITOR_SYNC_DELAY_MS, EDITOR_SYNC_MAX_DELAY_MS, EVENT_BRIDGE_ID, FLOATING_TOOLBAR_ID, FONT_FAMILIES, FONT_SIZE_MAX, FONT_SIZE_MIN, HIGHLIGHT_COLORS, INSERT_COMMANDS, INSERT_MENU_ID, INSERT_TRIGGER_ID, LINE_SPACINGS, LINK_CONTEXT_MENU_ID, LIST_CONTEXT_ID, MATRIX_DIMENSION_MAX, MAX_TRANSIENT_IMAGE_IMPORT_BYTES, ORDERED_LIST_STYLES, PARAGRAPH_FIRST_LINE_INDENT_MAX, PARAGRAPH_INDENT_STEP, PARAGRAPH_SIDE_INDENT_MAX, DEFAULT_AUTO_FIRST_LINE_INDENT, ROOT_ID, RUNTIME_MARKER, SCIENTIFIC_CLIPBOARD_MIME, SCIENTIFIC_CLIPBOARD_VERSION, SCIENTIFIC_COMPOSER_DESCRIPTION_ID, SCIENTIFIC_COMPOSER_TITLE_ID, SCIENTIFIC_GUIDED_ROW_LIMIT, SCIENTIFIC_NODE_TYPES, SCIENTIFIC_NOTATION_BY_LATEX_COMMAND, SCIENTIFIC_NOTATION_BY_UNICODE, SCIENTIFIC_NOTATION_REGISTRY, SCIENTIFIC_PREVIEW_DELAY_MS, SCIENTIFIC_RENDER_EXPANDED_SOURCE_LIMIT, SCIENTIFIC_RENDER_FALLBACK_BATCH, SCIENTIFIC_RENDER_OPTIONS, SCIENTIFIC_RENDER_ROOT_MARGIN, SCIENTIFIC_RENDER_SOURCE_LIMIT, SCIENTIFIC_RENDER_VISIBLE_BATCH, SCIENTIFIC_SYMBOL_GROUPS, SCIENTIFIC_TEMPLATE_GROUPS, SCIENTIFIC_TOOLS_DESCRIPTION_ID, SCIENTIFIC_TOOLS_TITLE_ID, SLASH_COMMANDS, SURFACE_GAP, TABLE_ACTION_MENU_ID, TABLE_COLUMN_CONTROL_ID, TABLE_DEFAULT_COLUMN_WIDTH, TABLE_GRID_SIZE, TABLE_LEVEL_CONTROL_ID, TABLE_MIN_COLUMN_WIDTH, TABLE_ROW_CONTROL_ID, TABLE_TOOLBAR_ID, TEXT_COLORS, TEXT_COLOR_LABELS, TEXT_ALIGNMENTS, VIEWPORT_MARGIN, findHighlightKey, linkPasteKey, placeholderKey, preservedSelectionKey, scientificSelectionKey, tableBoundaryKey};

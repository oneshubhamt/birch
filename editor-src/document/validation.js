import {DOCUMENT_SPEC, ENVELOPE_SPEC, contextAllowed, specDomain, specLimit, utf8ByteLength, utf8StringCompare, validateAttributeObject, validateSpecAttrs} from "./spec.js";

function normalizeSafeHref(value) {
  if (typeof value !== 'string') return null;
  const source = value.trim();
  if (
    source === ''
    || source !== value
    || [...source].some(character => /[\p{White_Space}\p{Cc}]/u.test(character))
  ) {
    return null;
  }
  let parsed;
  try {
    parsed = new URL(source);
  } catch {
    return null;
  }
  const protocol = parsed.protocol.toLowerCase();
  if (protocol === 'https:' || protocol === 'http:') {
    return parsed.hostname && parsed.hostname !== '.' && parsed.hostname !== '..' ? source : null;
  }
  if (protocol === 'mailto:') {
    const address = source.slice(7).split(/[?#]/, 1)[0];
    const separator = address.lastIndexOf('@');
    return separator > 0 && separator < address.length - 1 ? source : null;
  }
  return null;
}

function safeHref(value) {
  return normalizeSafeHref(value) !== null;
}

function validNoteId(value) {
  return typeof value === 'string'
    && value.length > 0
    && utf8ByteLength(value) <= specLimit('note_id_max_bytes')
    && !/[\u0000-\u001f\u007f-\u009f]/u.test(value);
}

function validAttachmentId(value) {
  return typeof value === 'string'
    && value.length > 0
    && utf8ByteLength(value) <= specLimit('attachment_id_max_bytes')
    && !/[\u0000-\u001f\u007f-\u009f]/u.test(value);
}

function canonicalObject(value, path) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${path} must be a JSON object`);
  }
  return value;
}

function canonicalRequiredString(object, key, path) {
  const value = object[key];
  if (typeof value !== 'string') throw new Error(`${path}.${key} must be a string`);
  return value;
}

function canonicalRequiredAttrs(object, path) {
  if (!Object.prototype.hasOwnProperty.call(object, 'attrs')) {
    throw new Error(`${path}.attrs must be a JSON object`);
  }
  return canonicalObject(object.attrs, `${path}.attrs`);
}

function canonicalRequiredContent(object, path) {
  if (!Array.isArray(object.content)) throw new Error(`${path}.content must be an array`);
  return object.content;
}

function canonicalOptionalContent(object, path) {
  if (!Object.prototype.hasOwnProperty.call(object, 'content')) return [];
  if (!Array.isArray(object.content)) throw new Error(`${path}.content must be an array`);
  return object.content;
}

function requireMinimumContent(node, rule, path) {
  const content = canonicalRequiredContent(node, path);
  const minimum = Number.isSafeInteger(rule.content_min) ? rule.content_min : 0;
  if (content.length < minimum) throw new Error(`${path} does not contain enough child nodes`);
  return content;
}

function validateChildren(content, rule, path) {
  const childContext = rule.child_context;
  if (typeof childContext !== 'string' || childContext === '') throw new Error(`${path}: document specification is missing child_context`);
  const childTypes = Array.isArray(rule.child_types) ? rule.child_types : null;
  content.forEach((child, index) => {
    const childPath = `${path}.content[${index}]`;
    if (childTypes) {
      const childObject = canonicalObject(child, childPath);
      const childType = canonicalRequiredString(childObject, 'type', childPath);
      if (!childTypes.includes(childType)) throw new Error(`${childPath} contains an invalid child node`);
    }
    validateCanonicalNode(child, childContext, childPath);
  });
}

function validateCanonicalTableGeometry(rows, path) {
  const rowCount = rows.length;
  const occupied = Array.from({length: rowCount}, () => []);
  const columnWidthGrid = Array.from({length: rowCount}, () => []);
  let tableWidth = 0;

  rows.forEach((rowValue, rowIndex) => {
    const rowPath = `${path}.content[${rowIndex}]`;
    const row = canonicalObject(rowValue, rowPath);
    const cells = canonicalRequiredContent(row, rowPath);
    let column = 0;

    cells.forEach((cellValue, cellIndex) => {
      while (occupied[rowIndex][column] !== undefined) column += 1;
      const cellPath = `${rowPath}.content[${cellIndex}]`;
      const cell = canonicalObject(cellValue, cellPath);
      const attrs = canonicalRequiredAttrs(cell, cellPath);
      const {colspan, rowspan, colwidth} = attrs;
      if (rowIndex + rowspan > rowCount) {
        throw new Error(`${cellPath}.attrs.rowspan extends beyond the final table row`);
      }

      for (let rowOffset = 0; rowOffset < rowspan; rowOffset += 1) {
        for (let columnOffset = 0; columnOffset < colspan; columnOffset += 1) {
          const targetRow = rowIndex + rowOffset;
          const targetColumn = column + columnOffset;
          if (occupied[targetRow][targetColumn] !== undefined) {
            throw new Error(`${cellPath} collides with another spanning table cell at column ${targetColumn + 1}`);
          }
        }
      }

      for (let rowOffset = 0; rowOffset < rowspan; rowOffset += 1) {
        for (let columnOffset = 0; columnOffset < colspan; columnOffset += 1) {
          const targetRow = rowIndex + rowOffset;
          const targetColumn = column + columnOffset;
          occupied[targetRow][targetColumn] = cellPath;
          columnWidthGrid[targetRow][targetColumn] = Array.isArray(colwidth) ? colwidth[columnOffset] : null;
        }
      }
      column += colspan;
    });
    tableWidth = Math.max(tableWidth, occupied[rowIndex].length);
  });

  if (tableWidth === 0) throw new Error(`${path}: table must occupy at least one logical column`);
  occupied.forEach((row, rowIndex) => {
    for (let column = 0; column < tableWidth; column += 1) {
      if (row[column] === undefined) {
        throw new Error(`${path}.content[${rowIndex}] is missing logical table cell ${column + 1}`);
      }
    }
  });

  for (let column = 0; column < tableWidth; column += 1) {
    let expectedWidth = null;
    let sawUnspecifiedWidth = false;
    for (let row = 0; row < rowCount; row += 1) {
      const width = columnWidthGrid[row][column];
      if (width === null || width === undefined) {
        sawUnspecifiedWidth = true;
      } else if (expectedWidth === null) {
        expectedWidth = width;
      } else if (expectedWidth !== width) {
        throw new Error(`${path}: logical column ${column + 1} has conflicting column widths`);
      }
    }
    if (expectedWidth !== null && sawUnspecifiedWidth) {
      throw new Error(`${path}: logical column ${column + 1} has incomplete column-width metadata`);
    }
  }
}

function mediaAnnotationSortKey(annotation, rule, path) {
  const parts = Array.isArray(rule.media_annotation_sort_key) ? rule.media_annotation_sort_key : [];
  if (parts.length === 0) throw new Error(`${path}: document specification is missing media annotation sort order`);
  return parts.map(part => {
    if (part === 'startMs') return {kind: 'number', value: annotation.startMs};
    if (part === 'endMs_or_startMs') return {kind: 'number', value: annotation.endMs ?? annotation.startMs};
    if (part === 'id_utf8') return {kind: 'utf8', value: annotation.id};
    throw new Error(`${path}: document specification uses unsupported media annotation sort key ${part}`);
  });
}

function compareMediaAnnotationSortKeys(left, right) {
  const length = Math.min(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const a = left[index];
    const b = right[index];
    if (a.kind !== b.kind) throw new Error('Rustnote document specification mixes incompatible media annotation sort fields');
    const order = a.kind === 'utf8'
      ? utf8StringCompare(a.value, b.value)
      : (a.value === b.value ? 0 : (a.value < b.value ? -1 : 1));
    if (order !== 0) return order;
  }
  return left.length === right.length ? 0 : (left.length < right.length ? -1 : 1);
}

const MARK_VALIDATORS = Object.freeze({
  simple() {},
  font_family() {},
  font_size() {},
  text_color() {},
  highlight(mark, _rule, path) {
    if (Object.prototype.hasOwnProperty.call(mark, 'attrs')
        && !Object.prototype.hasOwnProperty.call(mark.attrs, 'color')) {
      throw new Error(`${path}.attrs.color must be a string`);
    }
  },
  link(mark, _rule, path) {
    const href = canonicalRequiredAttrs(mark, path).href;
    if (typeof href !== 'string') throw new Error(`${path}.attrs.href must be a string`);
    if (!safeHref(href)) throw new Error(`${path}.attrs.href uses an unsupported or incomplete URL`);
  },
  note_link(mark, _rule, path) {
    const noteId = canonicalRequiredAttrs(mark, path).noteId;
    if (!validNoteId(noteId)) throw new Error(`${path}.attrs.noteId is invalid`);
  },
  media_link(mark, _rule, path) {
    const attrs = canonicalRequiredAttrs(mark, path);
    if (!validNoteId(attrs.noteId)) throw new Error(`${path}.attrs.noteId is invalid`);
    if (attrs.endMs !== null && attrs.endMs !== undefined && attrs.endMs <= attrs.startMs) {
      throw new Error(`${path}.attrs.endMs must be greater than startMs`);
    }
  },
});

function validateTextblockLayoutInvariants(node, path, boundaryAware) {
  const attrs = node.attrs && typeof node.attrs === 'object' && !Array.isArray(node.attrs)
    ? node.attrs
    : {};
  const firstLineIndent = attrs.firstLineIndent ?? 0;
  const dropCap = attrs.dropCap ?? false;
  if (dropCap && firstLineIndent !== 0) {
    throw new Error(`${path}.attrs.dropCap cannot be combined with firstLineIndent`);
  }
  if (!boundaryAware || attrs.boundary !== true) return;
  if ((attrs.lineSpacing ?? null) !== null
      || (attrs.textAlignment ?? 'left') !== 'left'
      || (attrs.leftIndent ?? 0) !== 0
      || (attrs.rightIndent ?? 0) !== 0
      || firstLineIndent !== 0
      || dropCap) {
    throw new Error(`${path}: atomic boundary paragraphs must use default paragraph layout`);
  }
}

const NODE_VALIDATORS = Object.freeze({
  doc(node, rule, path) {
    const content = requireMinimumContent(node, rule, path);
    validateChildren(content, rule, path);
  },
  textblock(node, rule, path) {
    validateChildren(canonicalOptionalContent(node, path), rule, path);
    validateTextblockLayoutInvariants(node, path, true);
  },
  heading(node, rule, path) {
    validateChildren(canonicalOptionalContent(node, path), rule, path);
    validateTextblockLayoutInvariants(node, path, false);
  },
  blockquote(node, rule, path) {
    const content = requireMinimumContent(node, rule, path);
    validateChildren(content, rule, path);
  },
  code_block(node, rule, path) {
    const content = canonicalOptionalContent(node, path);
    validateChildren(content, rule, path);
    const allowedMarks = rule.allowed_marks || [];
    content.forEach((child, index) => {
      const childPath = `${path}.content[${index}]`;
      const marks = canonicalObject(child, childPath).marks;
      if (marks === undefined) return;
      if (!Array.isArray(marks)) throw new Error(`${childPath}.marks must be an array`);
      marks.forEach((mark, markIndex) => {
        const markPath = `${childPath}.marks[${markIndex}]`;
        const markType = canonicalRequiredString(canonicalObject(mark, markPath), 'type', markPath);
        if (!allowedMarks.includes(markType)) throw new Error(`${markPath}: code blocks support only typography marks`);
      });
    });
  },
  list(node, rule, path, nodeType) {
    const content = requireMinimumContent(node, rule, path);
    content.forEach((child, index) => {
      const childPath = `${path}.content[${index}]`;
      const childObject = canonicalObject(child, childPath);
      if (canonicalRequiredString(childObject, 'type', childPath) !== rule.list_item_type) {
        throw new Error(`${childPath}: ${nodeType} contains an invalid list item`);
      }
      validateCanonicalNode(child, rule.list_item_context, childPath);
    });
  },
  list_item(node, rule, path) {
    const content = requireMinimumContent(node, rule, path);
    const first = canonicalObject(content[0], `${path}.content[0]`);
    if (canonicalRequiredString(first, 'type', `${path}.content[0]`) !== rule.first_child_type) {
      throw new Error(`${path}: list items must begin with ${String(rule.first_child_type)}`);
    }
    validateChildren(content, rule, path);
  },
  text(node, rule, path, _nodeType, context) {
    if (typeof node.text !== 'string') throw new Error(`${path}.text must be a string`);
    if (node.text.length === 0) throw new Error(`${path}.text must not be empty`);
    if (Object.prototype.hasOwnProperty.call(node, 'content')) throw new Error(`${path}: text nodes cannot contain child nodes`);
    if (!Object.prototype.hasOwnProperty.call(node, 'marks')) return;
    if (!Array.isArray(node.marks)) throw new Error(`${path}.marks must be an array`);
    if ((rule.markless_contexts || []).includes(context) && node.marks.length > 0) {
      throw new Error(`${path}: this text context cannot contain marks`);
    }
    const markTypes = [];
    node.marks.forEach((mark, index) => {
      validateCanonicalMark(mark, `${path}.marks[${index}]`);
      markTypes.push(canonicalRequiredString(canonicalObject(mark, `${path}.marks[${index}]`), 'type', `${path}.marks[${index}]`));
    });
    for (const group of rule.mutually_exclusive_marks || []) {
      if (group.filter(mark => markTypes.includes(mark)).length > 1) throw new Error(`${path}: mutually exclusive marks overlap`);
    }
    const maxOne = rule.max_one_of_marks || [];
    if (markTypes.filter(mark => maxOne.includes(mark)).length > 1) throw new Error(`${path}: semantic link marks overlap`);
  },
  hard_break(node, _rule, path) {
    if (Object.prototype.hasOwnProperty.call(node, 'content') || Object.prototype.hasOwnProperty.call(node, 'text')) {
      throw new Error(`${path}: hard breaks cannot contain content`);
    }
  },
  horizontal_rule(node, _rule, path) {
    if (['content', 'text', 'marks', 'attrs'].some(key => Object.prototype.hasOwnProperty.call(node, key))) {
      throw new Error(`${path}: horizontal rules cannot contain attributes or content`);
    }
  },
  scientific(node, rule, path) {
    if (['attrs', 'text', 'marks'].some(key => Object.prototype.hasOwnProperty.call(node, key))) {
      throw new Error(`${path}: scientific nodes may contain only plain source-text children`);
    }
    validateChildren(canonicalOptionalContent(node, path), rule, path);
  },
  pending_attachment(node, rule, path) {
    if (['content', 'text', 'marks'].some(key => Object.prototype.hasOwnProperty.call(node, key))) {
      throw new Error(`${path}: pending attachment nodes cannot contain text, marks, or child content`);
    }
    const attrs = canonicalRequiredAttrs(node, path);
    if (attrs.items !== null) {
      attrs.items.forEach((item, index) => {
        validateAttributeObject(item, rule.item_attrs || {}, 'strict', `${path}.attrs.items[${index}]`);
      });
    }
  },
  attachment(node, rule, path) {
    if (['content', 'text', 'marks'].some(key => Object.prototype.hasOwnProperty.call(node, key))) {
      throw new Error(`${path}: attachment nodes cannot contain text, marks, or child content`);
    }
    const attrs = canonicalRequiredAttrs(node, path);
    if (attrs.mediaAnnotations !== undefined && attrs.mediaAnnotations !== null) {
      const timeBasedDomain = rule.time_based_kind_domain;
      if (typeof timeBasedDomain !== 'string' || !specDomain(timeBasedDomain).includes(attrs.kind)) {
        throw new Error(`${path}.attrs.mediaAnnotations is only supported for audio/video attachments`);
      }
      const seen = new Set();
      let previousKey = null;
      attrs.mediaAnnotations.forEach((annotation, index) => {
        const annotationPath = `${path}.attrs.mediaAnnotations[${index}]`;
        validateAttributeObject(annotation, rule.media_annotation_attrs || {}, 'strict', annotationPath);
        if (seen.has(annotation.id)) throw new Error(`${annotationPath}.id is duplicated`);
        seen.add(annotation.id);
        if (annotation.endMs !== null && annotation.endMs !== undefined && annotation.endMs <= annotation.startMs) {
          throw new Error(`${annotationPath}.endMs must be greater than startMs`);
        }
        const key = mediaAnnotationSortKey(annotation, rule, annotationPath);
        if (previousKey && compareMediaAnnotationSortKeys(key, previousKey) < 0) {
          throw new Error(`${path}.attrs.mediaAnnotations must follow the canonical sort order`);
        }
        previousKey = key;
      });
    }
    const hasTrim = attrs.mediaTrimStartMs !== undefined && attrs.mediaTrimStartMs !== null
      || attrs.mediaTrimEndMs !== undefined && attrs.mediaTrimEndMs !== null;
    const timeBasedDomain = rule.time_based_kind_domain;
    if (hasTrim && (typeof timeBasedDomain !== 'string' || !specDomain(timeBasedDomain).includes(attrs.kind))) {
      throw new Error(`${path}: media trim bounds are only supported for audio/video attachments`);
    }
    if (Number.isSafeInteger(attrs.mediaTrimStartMs) && Number.isSafeInteger(attrs.mediaTrimEndMs)
        && attrs.mediaTrimEndMs <= attrs.mediaTrimStartMs) {
      throw new Error(`${path}.attrs.mediaTrimEndMs must be greater than mediaTrimStartMs`);
    }
  },
  table(node, rule, path) {
    if (['text', 'marks', 'attrs'].some(key => Object.prototype.hasOwnProperty.call(node, key))) {
      throw new Error(`${path}: structural table nodes cannot contain attributes or inline fields`);
    }
    const content = requireMinimumContent(node, rule, path);
    validateChildren(content, rule, path);
    validateCanonicalTableGeometry(content, path);
  },
  table_row(node, rule, path) {
    if (['text', 'marks', 'attrs'].some(key => Object.prototype.hasOwnProperty.call(node, key))) {
      throw new Error(`${path}: structural table nodes cannot contain attributes or inline fields`);
    }
    const content = requireMinimumContent(node, rule, path);
    validateChildren(content, rule, path);
  },
  table_cell(node, rule, path) {
    if (Object.prototype.hasOwnProperty.call(node, 'text') || Object.prototype.hasOwnProperty.call(node, 'marks')) {
      throw new Error(`${path}: table cells cannot contain inline fields directly`);
    }
    const attrs = canonicalRequiredAttrs(node, path);
    if (Array.isArray(attrs.colwidth)) {
      if (attrs.colwidth.length !== attrs.colspan) {
        throw new Error(`${path}.attrs.colwidth must contain one width for each spanned column`);
      }
      const min = specLimit('table_column_width_min');
      const max = specLimit('table_column_width_max');
      attrs.colwidth.forEach((width, index) => {
        if (!Number.isSafeInteger(width) || width < min || width > max) {
          throw new Error(`${path}.attrs.colwidth[${index}] is outside the allowed width range`);
        }
      });
    }
    const content = requireMinimumContent(node, rule, path);
    validateChildren(content, rule, path);
  },
});

const NODE_VALIDATOR_INVARIANTS = Object.freeze({
  doc: ['children_match_context', 'content_minimum'],
  textblock: ['boundary_layout_default', 'children_match_context', 'drop_cap_first_line_exclusive'],
  heading: ['children_match_context', 'drop_cap_first_line_exclusive'],
  blockquote: ['children_match_context', 'content_minimum'],
  code_block: ['allowed_mark_restriction', 'child_type_restriction', 'children_match_context'],
  list: ['content_minimum', 'list_item_context', 'list_item_type'],
  list_item: ['children_match_context', 'content_minimum', 'first_child_type'],
  text: ['markless_context', 'marks_array', 'max_one_of_marks', 'mutually_exclusive_marks', 'text_leaf', 'text_required_non_empty'],
  hard_break: ['hard_break_leaf'],
  horizontal_rule: ['horizontal_rule_leaf'],
  scientific: ['child_type_restriction', 'children_match_context', 'scientific_plain_text_only'],
  pending_attachment: ['attachment_leaf', 'nested_item_attrs', 'unique_attribute'],
  attachment: ['attachment_leaf', 'media_annotation_attrs', 'media_annotation_end_after_start', 'media_annotation_sort_order', 'media_annotation_unique_id', 'media_trim_end_after_start', 'time_based_annotation_scope', 'time_based_trim_scope'],
  table: ['child_type_restriction', 'children_match_context', 'content_minimum', 'structural_table_fields', 'table_geometry'],
  table_row: ['child_type_restriction', 'children_match_context', 'content_minimum', 'structural_table_fields'],
  table_cell: ['children_match_context', 'column_width_shape', 'content_minimum', 'table_cell_inline_fields'],
});

const MARK_VALIDATOR_INVARIANTS = Object.freeze({
  simple: [],
  font_family: [],
  font_size: [],
  text_color: [],
  highlight: ['highlight_explicit_attrs_color'],
  link: ['safe_href'],
  note_link: ['valid_note_id'],
  media_link: ['media_link_end_after_start', 'valid_note_id'],
});

function sameStringSet(left, right) {
  if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) return false;
  const a = [...new Set(left)].sort();
  const b = [...new Set(right)].sort();
  return a.length === left.length && b.length === right.length && a.every((value, index) => value === b[index]);
}

function assertInvariantContracts(kind, implementations, rules) {
  for (const [name, rule] of Object.entries(rules)) {
    const implemented = implementations[rule.validator];
    if (!implemented || !sameStringSet(rule.invariants || [], implemented)) {
      throw new Error(`Rustnote ${kind} ${name} invariant contract does not match validator class ${String(rule.validator)}`);
    }
  }
}

function validatorClasses(rules) {
  return [...new Set(Object.values(rules).map(rule => rule.validator))].sort();
}

function assertValidatorClassCoverage(kind, validators, rules) {
  const implemented = Object.keys(validators).sort();
  const specified = validatorClasses(rules);
  if (implemented.length !== specified.length || implemented.some((name, index) => name !== specified[index])) {
    throw new Error(`Rustnote ${kind} validator classes do not match document-spec authority`);
  }
}

assertValidatorClassCoverage('node', NODE_VALIDATORS, DOCUMENT_SPEC.nodes);
assertValidatorClassCoverage('mark', MARK_VALIDATORS, DOCUMENT_SPEC.marks);
assertInvariantContracts('node', NODE_VALIDATOR_INVARIANTS, DOCUMENT_SPEC.nodes);
assertInvariantContracts('mark', MARK_VALIDATOR_INVARIANTS, DOCUMENT_SPEC.marks);

function validateCanonicalMark(value, path) {
  const mark = canonicalObject(value, path);
  const markType = canonicalRequiredString(mark, 'type', path);
  const rule = DOCUMENT_SPEC.marks[markType];
  if (!rule) throw new Error(`${path} contains unsupported mark type ${markType}`);
  validateSpecAttrs({value: mark, rule}, path);
  MARK_VALIDATORS[rule.validator](mark, rule, path, markType);
}

function validateCanonicalNode(value, context, path) {
  const node = canonicalObject(value, path);
  const nodeType = canonicalRequiredString(node, 'type', path);
  const rule = DOCUMENT_SPEC.nodes[nodeType];
  if (!rule) throw new Error(`${path} contains unsupported node type ${nodeType}`);
  if (!contextAllowed(rule, context)) throw new Error(`${path}: ${nodeType} appears in an invalid document position`);
  validateSpecAttrs({value: node, rule}, path);
  NODE_VALIDATORS[rule.validator](node, rule, path, nodeType, context);
}

function validateUniqueNodeAttributes(documentNode) {
  const seen = new Map();
  const visit = node => {
    if (!node || typeof node !== 'object' || Array.isArray(node)) return;
    const rule = DOCUMENT_SPEC.nodes[node.type];
    if (rule && typeof rule.unique_attr === 'string') {
      const value = node.attrs?.[rule.unique_attr];
      const key = `${node.type}.${rule.unique_attr}`;
      if (!seen.has(key)) seen.set(key, new Set());
      const values = seen.get(key);
      if (values.has(value)) throw new Error(`document contains duplicate ${key} values`);
      values.add(value);
    }
    if (Array.isArray(node.content)) node.content.forEach(visit);
  };
  visit(documentNode);
}

function validateCanonicalEnvelope(parsed) {
  const envelope = canonicalObject(parsed, 'editor state');
  if (envelope.schema_version !== ENVELOPE_SPEC.schema_version) {
    throw new Error(`editor state schema ${String(envelope.schema_version)} is unsupported`);
  }
  if (envelope.editor_engine !== ENVELOPE_SPEC.editor_engine) {
    throw new Error(`editor engine ${String(envelope.editor_engine)} is unsupported`);
  }
  if (envelope.document_schema !== ENVELOPE_SPEC.document_schema) {
    throw new Error(`document schema ${String(envelope.document_schema)} is unsupported`);
  }
  if (!Number.isInteger(envelope.document_schema_version)
      || envelope.document_schema_version !== ENVELOPE_SPEC.document_schema_version) {
    throw new Error(`document schema version ${String(envelope.document_schema_version)} is unsupported`);
  }
  validateCanonicalNode(envelope.document, 'root', 'document');
  validateUniqueNodeAttributes(envelope.document);
}


function validateSerializedEditorState(serialized) {
  if (typeof serialized !== 'string' || serialized.trim() === '') {
    throw new Error('Rustnote requires a canonical ProseMirror editor state.');
  }
  const parsed = JSON.parse(serialized);
  validateCanonicalEnvelope(parsed);
  return parsed;
}

export {normalizeSafeHref, safeHref, validAttachmentId, validNoteId, validateCanonicalEnvelope, validateSerializedEditorState};

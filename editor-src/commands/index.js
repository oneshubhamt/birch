import {emitEditorEvent} from "../bridge/events.js";
import {AUTO_INDENT_SUPPRESSION_META, FONT_FAMILIES, HIGHLIGHT_COLORS, PARAGRAPH_FIRST_LINE_INDENT_MAX, PARAGRAPH_INDENT_STEP, PARAGRAPH_SIDE_INDENT_MAX, preservedSelectionKey, SCIENTIFIC_CLIPBOARD_MIME, SCIENTIFIC_CLIPBOARD_VERSION, TABLE_GRID_SIZE, TABLE_MIN_COLUMN_WIDTH, TEXT_ALIGNMENTS, TEXT_COLORS} from "../config.js";
import {bulletListAttrs, normalizedBulletMarker, normalizedFontSize, normalizedLineSpacing, normalizedTextblockIndent, schema} from "../document/schema.js";
import {lift as commandLift, setBlockType as commandSetBlockType, wrapIn} from "prosemirror-commands";
import {GapCursor} from "prosemirror-gapcursor";
import {NodeSelection, TextSelection} from "prosemirror-state";
import {CellSelection} from "prosemirror-tables";
import {ancestorDepth, atomicBoundaryParagraph, clamp, clampDocumentPosition, currentTableContext, deleteSelectedTable, editorEnvelope, renderScientificSource, replaceTableWithParagraph, selectedLayoutTextblocks, selectedMarkAttribute, selectedTextblockLayoutValue, selectedTextblocks, textblockLayoutAttrs, tableDomContext, tableHasHeaderRow} from "../editor/core.js";
import {adjacentOrderedContext, commonSelectedListContext, liftSelectedListItems, orderedListAttrs, selectedListContexts, toggleList} from "../editor/list_system.js";



function setBlockTypeWithDefaultSize(nodeType, attrs = null) {
  const setType = commandSetBlockType(nodeType, attrs);
  return (state, dispatch, view) => {
    if (!dispatch) return setType(state, undefined, view);
    const affected = selectedTextblocks(state).map(({node, position}) => ({
      position,
      layout: textblockLayoutAttrs(node, node.type),
    }));
    let transaction = null;
    const applied = setType(state, next => { transaction = next; }, view);
    if (!applied || transaction === null) return false;
    for (const {position, layout} of affected) {
      const mapped = transaction.mapping.map(position);
      let node = transaction.doc.nodeAt(mapped);
      if (!node?.isTextblock) continue;
      const preserved = textblockLayoutAttrs({attrs: layout}, node.type);
      if (Object.keys(preserved).length > 0) {
        transaction = transaction.setNodeMarkup(mapped, undefined, {
          ...node.attrs,
          ...preserved,
        }, node.marks);
        node = transaction.doc.nodeAt(mapped);
      }
      if (!node?.isTextblock || node.content.size === 0) continue;
      transaction = transaction.removeMark(
        mapped + 1,
        mapped + 1 + node.content.size,
        schema.marks.font_size,
      );
    }
    transaction = transaction.removeStoredMark(schema.marks.font_size);
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function commandForBlock(action, state) {
  switch (action) {
    case 'paragraph':
      return commonSelectedListContext(state)
        ? liftSelectedListItems
        : setBlockTypeWithDefaultSize(schema.nodes.paragraph);
    case 'heading1':
      return setBlockTypeWithDefaultSize(schema.nodes.heading, {level: 1});
    case 'heading2':
      return setBlockTypeWithDefaultSize(schema.nodes.heading, {level: 2});
    case 'heading3':
      return setBlockTypeWithDefaultSize(schema.nodes.heading, {level: 3});
    case 'quote':
      return ancestorDepth(state, ['blockquote']) >= 0 ? commandLift : wrapIn(schema.nodes.blockquote);
    case 'code':
      return state.selection.$from.parent.type === schema.nodes.code_block
        ? commandSetBlockType(schema.nodes.paragraph)
        : commandSetBlockType(schema.nodes.code_block);
    default:
      return null;
  }
}

function setBulletListMarker(marker) {
  const normalized = normalizedBulletMarker(marker);
  return (state, dispatch) => {
    const contexts = selectedListContexts(state).filter(context => context.node.type === schema.nodes.bullet_list);
    if (contexts.length === 0) return false;
    if (!dispatch) return true;
    let transaction = state.tr;
    for (const context of contexts) {
      const mapped = transaction.mapping.map(context.pos);
      const node = transaction.doc.nodeAt(mapped);
      if (node?.type === schema.nodes.bullet_list && normalizedBulletMarker(node.attrs.marker) !== normalized) {
        transaction = transaction.setNodeMarkup(mapped, undefined, bulletListAttrs(normalized), node.marks);
      }
    }
    if (!transaction.docChanged) return true;
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function setOrderedListStyle(style) {
  return (state, dispatch) => {
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
          orderedListAttrs(node.attrs.order, style, node.attrs.restart),
          node.marks,
        );
      }
    }
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function selectionAllowsTypographyMark(state, markType) {
  const {selection} = state;
  if (selection.empty) {
    return selection.$from.parent.type.allowsMarkType(markType);
  }
  let allowed = false;
  state.doc.nodesBetween(selection.from, selection.to, (node, _position, parent) => {
    if (node.isText && parent?.type.allowsMarkType(markType)) {
      allowed = true;
      return false;
    }
    return !allowed;
  });
  return allowed;
}

function setInlineTypographyMark(markType, attrs) {
  return (state, dispatch) => {
    const {selection} = state;
    if (!selectionAllowsTypographyMark(state, markType)) {
      return false;
    }
    if (!dispatch) {
      return true;
    }
    let transaction = state.tr;
    if (selection.empty) {
      let marks = markType.removeFromSet(state.storedMarks || selection.$from.marks());
      if (attrs !== null) {
        marks = markType.create(attrs).addToSet(marks);
      }
      transaction = transaction.setStoredMarks(marks);
    } else {
      transaction = transaction.removeMark(selection.from, selection.to, markType);
      if (attrs !== null) {
        transaction = transaction.addMark(selection.from, selection.to, markType.create(attrs));
      }
    }
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function setFontFamily(family) {
  if (!FONT_FAMILIES.includes(family)) {
    return () => false;
  }
  return family === 'system'
    ? setInlineTypographyMark(schema.marks.font_family, null)
    : setInlineTypographyMark(schema.marks.font_family, {family});
}

function setFontSize(size) {
  const normalized = normalizedFontSize(size);
  return normalized === null
    ? () => false
    : setInlineTypographyMark(schema.marks.font_size, {size: normalized});
}

function setTextColor(color) {
  if (color === 'default') {
    return setInlineTypographyMark(schema.marks.text_color, null);
  }
  if (!TEXT_COLORS.includes(color)) {
    return () => false;
  }
  return setInlineTypographyMark(schema.marks.text_color, {color});
}

function setHighlightColor(color) {
  if (color === 'none') {
    return setInlineTypographyMark(schema.marks.highlight, null);
  }
  if (!HIGHLIGHT_COLORS.includes(color)) {
    return () => false;
  }
  return setInlineTypographyMark(schema.marks.highlight, {color});
}

function toggleHighlightColor(color) {
  return (state, dispatch) => {
    if (!HIGHLIGHT_COLORS.includes(color)) {
      return false;
    }
    const selected = selectedMarkAttribute(
      state,
      schema.marks.highlight,
      'color',
      'none',
    );
    return setHighlightColor(selected === color ? 'none' : color)(state, dispatch);
  };
}

function setLineSpacing(value) {
  const normalized = value === 'default' ? null : normalizedLineSpacing(value);
  if (value !== 'default' && normalized === null) {
    return () => false;
  }
  return (state, dispatch) => {
    const blocks = selectedTextblocks(state).filter(({node}) => (
      node.type.spec.attrs
      && Object.prototype.hasOwnProperty.call(node.type.spec.attrs, 'lineSpacing')
      && node.attrs.boundary !== true
    ));
    if (blocks.length === 0) {
      return false;
    }
    if (!dispatch) {
      return true;
    }
    let transaction = state.tr;
    for (const {position} of blocks) {
      const mapped = transaction.mapping.map(position);
      const node = transaction.doc.nodeAt(mapped);
      if (!node?.isTextblock || !node.type.spec.attrs?.lineSpacing) {
        continue;
      }
      transaction = transaction.setNodeMarkup(mapped, undefined, {
        ...node.attrs,
        lineSpacing: normalized,
      }, node.marks);
    }
    if (!transaction.docChanged) {
      return false;
    }
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function paragraphIndentMaximum(attribute) {
  return attribute === 'firstLineIndent'
    ? PARAGRAPH_FIRST_LINE_INDENT_MAX
    : PARAGRAPH_SIDE_INDENT_MAX;
}

function setTextAlignment(value) {
  const normalized = String(value || '').toLowerCase();
  if (!TEXT_ALIGNMENTS.includes(normalized)) return () => false;
  return (state, dispatch) => {
    const blocks = selectedLayoutTextblocks(state);
    if (blocks.length === 0) return false;
    if (!dispatch) return true;
    let transaction = state.tr;
    for (const {position} of blocks) {
      const mapped = transaction.mapping.map(position);
      const node = transaction.doc.nodeAt(mapped);
      if (!node || (node.type !== schema.nodes.paragraph && node.type !== schema.nodes.heading) || node.attrs.boundary === true) continue;
      transaction = transaction.setNodeMarkup(mapped, undefined, {
        ...node.attrs,
        textAlignment: normalized,
      }, node.marks);
    }
    if (!transaction.docChanged) return false;
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function setParagraphLayoutAttribute(attribute, value) {
  const supported = ['leftIndent', 'rightIndent', 'firstLineIndent'];
  if (!supported.includes(attribute)) return () => false;
  const normalized = normalizedTextblockIndent(value, paragraphIndentMaximum(attribute));
  return (state, dispatch) => {
    const blocks = selectedLayoutTextblocks(state);
    if (blocks.length === 0) return false;
    if (!dispatch) return true;
    let transaction = state.tr;
    let suppressAt = null;
    for (const {position} of blocks) {
      const mapped = transaction.mapping.map(position);
      const node = transaction.doc.nodeAt(mapped);
      if (!node || (node.type !== schema.nodes.paragraph && node.type !== schema.nodes.heading) || node.attrs.boundary === true) continue;
      if (blocks.length === 1 && attribute === 'firstLineIndent' && Number(node.attrs.firstLineIndent || 0) > 0 && normalized === 0) suppressAt = mapped;
      const nextAttrs = {...node.attrs, [attribute]: normalized};
      if (attribute === 'firstLineIndent' && normalized > 0) nextAttrs.dropCap = false;
      transaction = transaction.setNodeMarkup(mapped, undefined, nextAttrs, node.marks);
    }
    if (!transaction.docChanged) return false;
    if (suppressAt !== null) transaction = transaction.setMeta(AUTO_INDENT_SUPPRESSION_META, {position: suppressAt});
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function adjustParagraphIndent(attribute, direction) {
  const supported = ['leftIndent', 'rightIndent', 'firstLineIndent'];
  if (!supported.includes(attribute) || !Number.isFinite(Number(direction))) return () => false;
  const delta = Math.sign(Number(direction)) * PARAGRAPH_INDENT_STEP;
  return (state, dispatch) => {
    const blocks = selectedLayoutTextblocks(state);
    if (blocks.length === 0) return false;
    if (!dispatch) return true;
    let transaction = state.tr;
    let suppressAt = null;
    for (const {position} of blocks) {
      const mapped = transaction.mapping.map(position);
      const node = transaction.doc.nodeAt(mapped);
      if (!node || (node.type !== schema.nodes.paragraph && node.type !== schema.nodes.heading) || node.attrs.boundary === true) continue;
      const previous = Number(node.attrs[attribute] || 0);
      const next = normalizedTextblockIndent(previous + delta, paragraphIndentMaximum(attribute));
      if (blocks.length === 1 && attribute === 'firstLineIndent' && previous > 0 && next === 0) suppressAt = mapped;
      const nextAttrs = {...node.attrs, [attribute]: next};
      if (attribute === 'firstLineIndent' && next > 0) nextAttrs.dropCap = false;
      transaction = transaction.setNodeMarkup(mapped, undefined, nextAttrs, node.marks);
    }
    if (!transaction.docChanged) return false;
    if (suppressAt !== null) transaction = transaction.setMeta(AUTO_INDENT_SUPPRESSION_META, {position: suppressAt});
    dispatch(transaction.scrollIntoView());
    return true;
  };
}

function toggleDropCap(state, dispatch) {
  const blocks = selectedLayoutTextblocks(state);
  if (blocks.length === 0) return false;
  const current = selectedTextblockLayoutValue(state, 'dropCap', false);
  const enabled = current !== true;
  if (!dispatch) return true;
  let transaction = state.tr;
  for (const {position} of blocks) {
    const mapped = transaction.mapping.map(position);
    const node = transaction.doc.nodeAt(mapped);
    if (!node || (node.type !== schema.nodes.paragraph && node.type !== schema.nodes.heading) || node.attrs.boundary === true) continue;
    transaction = transaction.setNodeMarkup(mapped, undefined, {
      ...node.attrs,
      dropCap: enabled,
      firstLineIndent: enabled ? 0 : node.attrs.firstLineIndent,
    }, node.marks);
  }
  if (!transaction.docChanged) return false;
  dispatch(transaction.scrollIntoView());
  return true;
}

function clearParagraphLayout(state, dispatch) {
  const blocks = selectedLayoutTextblocks(state);
  if (blocks.length === 0) return false;
  if (!dispatch) return true;
  let transaction = state.tr;
  let suppressAt = null;
  for (const {position} of blocks) {
    const mapped = transaction.mapping.map(position);
    const node = transaction.doc.nodeAt(mapped);
    if (!node || (node.type !== schema.nodes.paragraph && node.type !== schema.nodes.heading) || node.attrs.boundary === true) continue;
    if (blocks.length === 1 && Number(node.attrs.firstLineIndent || 0) > 0) suppressAt = mapped;
    transaction = transaction.setNodeMarkup(mapped, undefined, {
      ...node.attrs,
      lineSpacing: null,
      textAlignment: 'left',
      leftIndent: 0,
      rightIndent: 0,
      firstLineIndent: 0,
      dropCap: false,
    }, node.marks);
  }
  if (!transaction.docChanged) return false;
  if (suppressAt !== null) transaction = transaction.setMeta(AUTO_INDENT_SUPPRESSION_META, {position: suppressAt});
  dispatch(transaction.scrollIntoView());
  return true;
}

function clearTypography(state, dispatch) {
  const blocks = selectedTextblocks(state).filter(({node}) => (
    node.type.spec.attrs && Object.prototype.hasOwnProperty.call(node.type.spec.attrs, 'lineSpacing')
  ));
  if (!dispatch) {
    return true;
  }
  const {selection} = state;
  let transaction = state.tr;
  const types = [
    schema.marks.font_family,
    schema.marks.font_size,
    schema.marks.text_color,
    schema.marks.strike,
    schema.marks.superscript,
    schema.marks.subscript,
  ];
  if (selection.empty) {
    let marks = state.storedMarks || selection.$from.marks();
    for (const type of types) {
      marks = type.removeFromSet(marks);
    }
    transaction = transaction.setStoredMarks(marks);
  } else {
    for (const type of types) {
      transaction = transaction.removeMark(selection.from, selection.to, type);
    }
  }
  for (const {position} of blocks) {
    const mapped = transaction.mapping.map(position);
    const node = transaction.doc.nodeAt(mapped);
    if (!node?.isTextblock || !node.type.spec.attrs?.lineSpacing || node.attrs.lineSpacing === null) {
      continue;
    }
    transaction = transaction.setNodeMarkup(mapped, undefined, {
      ...node.attrs,
      lineSpacing: null,
    }, node.marks);
  }
  dispatch(transaction.scrollIntoView());
  return true;
}

function commandForList(action) {
  if (action === 'bullet') {
    return (state, dispatch) => toggleList(state, dispatch, schema.nodes.bullet_list, schema.nodes.list_item);
  }
  if (action === 'number' || action === 'numbered') {
    return (state, dispatch) => toggleList(state, dispatch, schema.nodes.ordered_list, schema.nodes.list_item);
  }
  return (state, dispatch) => toggleList(state, dispatch, schema.nodes.task_list, schema.nodes.task_item);
}

function captureSelection(selection) {
  if (selection instanceof CellSelection) {
    return {
      kind: 'cell',
      anchorCell: selection.$anchorCell.pos,
      headCell: selection.$headCell.pos,
    };
  }
  if (selection instanceof NodeSelection) {
    return {kind: 'node', pos: selection.from};
  }
  if (selection instanceof GapCursor) {
    return {kind: 'gap', pos: selection.from};
  }
  return {
    kind: 'text',
    anchor: selection.anchor,
    head: selection.head,
  };
}

function selectionFromCapture(doc, captured) {
  if (!captured || typeof captured !== 'object') {
    return null;
  }
  if (captured.kind === 'cell') {
    return CellSelection.create(
      doc,
      clampDocumentPosition(doc, captured.anchorCell),
      clampDocumentPosition(doc, captured.headCell),
    );
  }
  if (captured.kind === 'node') {
    const position = clampDocumentPosition(doc, captured.pos);
    const node = doc.nodeAt(position);
    return node && NodeSelection.isSelectable(node)
      ? NodeSelection.create(doc, position)
      : TextSelection.near(doc.resolve(position), 1);
  }
  if (captured.kind === 'gap') {
    const $position = doc.resolve(clampDocumentPosition(doc, captured.pos));
    return GapCursor.valid($position)
      ? new GapCursor($position)
      : TextSelection.near($position, 1);
  }
  if (captured.kind === 'text') {
    return TextSelection.create(
      doc,
      clampDocumentPosition(doc, captured.anchor),
      clampDocumentPosition(doc, captured.head),
    );
  }
  if ('from' in captured || 'to' in captured) {
    const from = clampDocumentPosition(doc, captured.from);
    const to = clampDocumentPosition(doc, captured.to ?? captured.from);
    return TextSelection.create(doc, from, to);
  }
  return null;
}

function restoreSelection(view, preserved) {
  if (preserved === null) {
    return true;
  }
  try {
    const selection = selectionFromCapture(view.state.doc, preserved);
    if (!selection) {
      return false;
    }
    view.dispatch(
      view.state.tr
        .setSelection(selection)
        .setMeta(preservedSelectionKey, false)
        .setMeta('addToHistory', false),
    );
    return true;
  } catch {
    return false;
  }
}

function focusWithoutScroll(view) {
  // ProseMirror owns the logical editor selection. Calling the DOM node's
  // focus() directly can let the browser establish a different DOM selection
  // after toolbar interactions. EditorView.focus() restores the ProseMirror
  // selection while using ProseMirror's own no-scroll focus path.
  view.focus();
}

async function copyPlainText(text) {
  if (!navigator.clipboard?.writeText) return false;
  try {
    await navigator.clipboard.writeText(String(text || ''));
    return true;
  } catch {
    return false;
  }
}

function scientificClipboardContent(node) {
  const source = node.textContent;
  const displayMode = node.type === schema.nodes.math_display;
  const wrapper = document.createElement(displayMode ? 'div' : 'span');
  wrapper.className = displayMode ? 'rustnote-scientific-copy katex-display' : 'rustnote-scientific-copy';
  wrapper.setAttribute('data-rustnote-scientific', String(SCIENTIFIC_CLIPBOARD_VERSION));
  wrapper.setAttribute('data-rustnote-display', displayMode ? 'true' : 'false');
  wrapper.setAttribute('data-latex', source);
  const rendered = document.createElement('span');
  renderScientificSource(rendered, source, displayMode);
  wrapper.appendChild(rendered);
  const mathml = rendered.querySelector('math')?.outerHTML || '';
  return {
    source,
    displayMode,
    html: wrapper.outerHTML,
    mathml,
    custom: JSON.stringify({
      version: SCIENTIFIC_CLIPBOARD_VERSION,
      source,
      type: displayMode ? 'display' : 'inline',
    }),
  };
}

function clipboardTypeSupported(type) {
  return (
    typeof ClipboardItem === 'undefined'
    || typeof ClipboardItem.supports !== 'function'
    || ClipboardItem.supports(type)
  );
}

async function copyScientificClipboard(node, plainText) {
  const fallback = () => copyPlainText(plainText);
  if (!navigator.clipboard?.write || typeof ClipboardItem === 'undefined') return fallback();
  const content = scientificClipboardContent(node);
  const rich = {
    'text/plain': new Blob([plainText], {type: 'text/plain'}),
    'text/html': new Blob([content.html], {type: 'text/html'}),
  };
  if (clipboardTypeSupported(SCIENTIFIC_CLIPBOARD_MIME)) {
    rich[SCIENTIFIC_CLIPBOARD_MIME] = new Blob([content.custom], {type: SCIENTIFIC_CLIPBOARD_MIME});
  }
  if (content.mathml && clipboardTypeSupported('application/mathml+xml')) {
    rich['application/mathml+xml'] = new Blob([content.mathml], {type: 'application/mathml+xml'});
  }
  try {
    await navigator.clipboard.write([new ClipboardItem(rich)]);
    return true;
  } catch {
    try {
      await navigator.clipboard.write([new ClipboardItem({
        'text/plain': rich['text/plain'],
        'text/html': rich['text/html'],
      })]);
      return true;
    } catch {
      return fallback();
    }
  }
}

function serializedEditorEnvelope(controller) {
  if (
    controller.cachedEnvelopeRevision !== controller.documentRevision
    || controller.cachedEnvelopeGeneration !== controller.generation
  ) {
    controller.cachedEnvelope = JSON.stringify(editorEnvelope(controller));
    controller.cachedEnvelopeRevision = controller.documentRevision;
    controller.cachedEnvelopeGeneration = controller.generation;
  }
  return controller.cachedEnvelope;
}

function writeBridge(controller, dispatchInput) {
  const serialized = serializedEditorEnvelope(controller);
  if (
    dispatchInput
    && controller.lastSynchronizedRevision === controller.documentRevision
    && controller.lastSynchronizedGeneration === controller.generation
  ) {
    return serialized;
  }
  if (dispatchInput) {
    emitEditorEvent('document_changed', editorEnvelope(controller));
  }
  controller.lastSynchronizedRevision = controller.documentRevision;
  controller.lastSynchronizedGeneration = controller.generation;
  return serialized;
}

function foldFindText(value) {
  return String(value || '').toLocaleLowerCase().replace(/ς/g, 'σ');
}

function normalizedFindQuery(query) {
  return foldFindText(String(query || '').trim());
}

function pushFindMappingSegment(index, foldedText, sourceFrom, sourceTo, direct) {
  if (foldedText === '') return;
  const foldedFrom = index.foldedLength;
  const foldedTo = foldedFrom + foldedText.length;
  index.textParts.push(foldedText);
  const previous = index.segments[index.segments.length - 1];
  if (
    direct
    && previous
    && previous.direct
    && previous.foldedTo === foldedFrom
    && previous.sourceTo === sourceFrom
  ) {
    previous.foldedTo = foldedTo;
    previous.sourceTo = sourceTo;
  } else {
    index.segments.push({foldedFrom, foldedTo, sourceFrom, sourceTo, direct});
  }
  index.foldedLength = foldedTo;
}

function appendFoldedSearchText(index, source, position) {
  const text = String(source || '');
  if (text === '') return;
  const folded = foldFindText(text);
  if (folded.length === text.length) {
    pushFindMappingSegment(index, folded, position, position + text.length, true);
    return;
  }

  // Length-changing Unicode case folds are uncommon. Keep compact direct
  // segments around them and map only each exceptional character as a unit.
  let sourceOffset = 0;
  for (const character of text) {
    const characterFolded = foldFindText(character);
    const from = position + sourceOffset;
    const to = from + character.length;
    pushFindMappingSegment(
      index,
      characterFolded,
      from,
      to,
      characterFolded.length === character.length,
    );
    sourceOffset += character.length;
  }
}

function findMappingSegment(segments, foldedOffset) {
  let low = 0;
  let high = segments.length - 1;
  while (low <= high) {
    const middle = (low + high) >> 1;
    const segment = segments[middle];
    if (foldedOffset < segment.foldedFrom) {
      high = middle - 1;
    } else if (foldedOffset >= segment.foldedTo) {
      low = middle + 1;
    } else {
      return segment;
    }
  }
  return null;
}

function sourceStartForFoldedOffset(index, foldedOffset) {
  const segment = findMappingSegment(index.segments, foldedOffset);
  if (!segment) return null;
  return segment.direct
    ? segment.sourceFrom + (foldedOffset - segment.foldedFrom)
    : segment.sourceFrom;
}

function sourceEndForFoldedOffset(index, foldedOffsetExclusive) {
  if (foldedOffsetExclusive <= 0) return null;
  const segment = findMappingSegment(index.segments, foldedOffsetExclusive - 1);
  if (!segment) return null;
  return segment.direct
    ? segment.sourceFrom + (foldedOffsetExclusive - segment.foldedFrom)
    : segment.sourceTo;
}

function textblockSearchIndex(node, position) {
  const index = {textParts: [], segments: [], foldedLength: 0, scientificRanges: []};
  node.forEach((child, offset) => {
    const childPosition = position + 1 + offset;
    if (child.isText && child.text) {
      appendFoldedSearchText(index, child.text, childPosition);
    } else if (child.type === schema.nodes.hard_break) {
      appendFoldedSearchText(index, '\n', childPosition);
    } else if (child.type === schema.nodes.math_inline) {
      const foldedFrom = index.foldedLength;
      appendFoldedSearchText(index, child.textContent, childPosition + 1);
      index.scientificRanges.push({
        foldedFrom,
        foldedTo: index.foldedLength,
        nodeFrom: childPosition,
        nodeTo: childPosition + child.nodeSize,
      });
    }
  });
  if (node.type === schema.nodes.math_display) {
    index.scientificRanges.push({
      foldedFrom: 0,
      foldedTo: index.foldedLength,
      nodeFrom: position,
      nodeTo: position + node.nodeSize,
    });
  }
  return {
    text: index.textParts.join(''),
    segments: index.segments,
    scientificRanges: index.scientificRanges,
  };
}

function buildFindDocumentIndex(doc) {
  const blocks = [];
  doc.descendants((node, position) => {
    if (!node.isTextblock) return true;
    blocks.push(textblockSearchIndex(node, position));
    return false;
  });
  return blocks;
}

function findMatches(documentIndex, normalizedQuery) {
  if (normalizedQuery === '') return [];
  const matches = [];
  for (const index of documentIndex) {
    let cursor = 0;
    while (cursor <= index.text.length - normalizedQuery.length) {
      const found = index.text.indexOf(normalizedQuery, cursor);
      if (found < 0) break;
      const matchEnd = found + normalizedQuery.length;
      const from = sourceStartForFoldedOffset(index, found);
      const to = sourceEndForFoldedOffset(index, matchEnd);
      if (Number.isInteger(from) && Number.isInteger(to) && from < to) {
        const scientificRange = index.scientificRanges.find(range => (
          range.foldedFrom <= found && matchEnd <= range.foldedTo
        ));
        matches.push({
          from,
          to,
          decorationKind: scientificRange ? 'node' : 'inline',
          decorationFrom: scientificRange ? scientificRange.nodeFrom : from,
          decorationTo: scientificRange ? scientificRange.nodeTo : to,
        });
      }
      cursor = found + Math.max(normalizedQuery.length, 1);
    }
  }
  return matches;
}

function commandMatchesQuery(command, rawQuery) {
  const query = rawQuery.trim().toLocaleLowerCase();
  if (query === '') {
    return true;
  }
  const haystack = [command.label, command.description, ...command.keywords]
    .join(' ')
    .toLocaleLowerCase();
  return query.split(/\s+/).every(token => haystack.includes(token));
}

function insertHorizontalRule(state, dispatch) {
  if (!dispatch) {
    return true;
  }
  const rule = schema.nodes.horizontal_rule.create();
  let transaction = state.tr.replaceSelectionWith(rule).scrollIntoView();
  if (transaction.selection.to >= transaction.doc.content.size) {
    const paragraphPosition = transaction.doc.content.size;
    transaction = transaction.insert(paragraphPosition, atomicBoundaryParagraph());
    transaction = transaction.setSelection(TextSelection.create(transaction.doc, paragraphPosition + 1));
  }
  dispatch(transaction);
  return true;
}

function createTableNode(rows, columns, headerRow) {
  const rowNodes = [];
  const safeRows = clamp(Number(rows) || 1, 1, TABLE_GRID_SIZE);
  const safeColumns = clamp(Number(columns) || 1, 1, TABLE_GRID_SIZE);
  for (let rowIndex = 0; rowIndex < safeRows; rowIndex += 1) {
    const cellNodes = [];
    for (let columnIndex = 0; columnIndex < safeColumns; columnIndex += 1) {
      const type = headerRow && rowIndex === 0 ? schema.nodes.table_header : schema.nodes.table_cell;
      cellNodes.push(type.createAndFill());
    }
    rowNodes.push(schema.nodes.table_row.create(null, cellNodes));
  }
  return schema.nodes.table.create(null, rowNodes);
}

function nearestInsertedTablePosition(doc, expectedPosition) {
  let best = null;
  doc.descendants((node, position) => {
    if (node.type !== schema.nodes.table) {
      return true;
    }
    const distance = Math.abs(position - expectedPosition);
    if (best === null || distance < best.distance) {
      best = {position, distance, node};
    }
    return false;
  });
  return best;
}

function insertTable(rows, columns, headerRow) {
  return (state, dispatch) => {
    if (selectionIsInsideTable(state)) {
      return false;
    }
    const table = createTableNode(rows, columns, headerRow);
    if (!table) {
      return false;
    }
    if (!dispatch) {
      return true;
    }
    const expectedPosition = state.selection.from;
    let transaction = state.tr.replaceSelectionWith(table).scrollIntoView();
    const inserted = nearestInsertedTablePosition(transaction.doc, expectedPosition);
    if (!inserted) {
      return false;
    }
    const afterTable = inserted.position + inserted.node.nodeSize;
    if (afterTable >= transaction.doc.content.size) {
      transaction = transaction.insert(afterTable, atomicBoundaryParagraph());
    }
    const firstCellTextPosition = Math.min(inserted.position + 4, transaction.doc.content.size);
    transaction = transaction.setSelection(TextSelection.near(transaction.doc.resolve(firstCellTextPosition), 1));
    dispatch(transaction);
    return true;
  };
}

function selectionIsInsideTable(state) {
  return ancestorDepth(state, ['table']) >= 0;
}

function toggleActiveTableHeaderRow(state, dispatch, view) {
  const context = currentTableContext(view);
  if (!context || context.map.height < 1 || context.map.width < 1) {
    return false;
  }
  if (!dispatch) {
    return true;
  }
  const nextType = tableHasHeaderRow(context)
    ? schema.nodes.table_cell
    : schema.nodes.table_header;
  const positions = new Set();
  for (let column = 0; column < context.map.width; column += 1) {
    positions.add(context.map.positionAt(0, column, context.table.node));
  }
  let transaction = state.tr;
  for (const relativePosition of positions) {
    const position = context.table.start + relativePosition;
    const cell = transaction.doc.nodeAt(position);
    if (cell?.type === schema.nodes.table_cell || cell?.type === schema.nodes.table_header) {
      transaction = transaction.setNodeMarkup(position, nextType, cell.attrs, cell.marks);
    }
  }
  if (!transaction.docChanged) {
    return false;
  }
  dispatch(transaction.scrollIntoView());
  return true;
}

function deleteActiveTable(state, dispatch, view) {
  const context = currentTableContext(view);
  if (!context) {
    return deleteSelectedTable(state, dispatch);
  }
  return replaceTableWithParagraph(
    state,
    dispatch,
    context.table.pos,
    context.table.node,
  );
}

function setActiveTableColumnWidths(state, dispatch, view, mode) {
  const context = currentTableContext(view);
  const dom = tableDomContext(view, context);
  if (!context || !dom) {
    return false;
  }
  if (!dispatch) {
    return true;
  }
  const width = mode === 'equal'
    ? Math.max(TABLE_MIN_COLUMN_WIDTH, Math.floor(dom.wrapper.clientWidth / Math.max(1, context.map.width)))
    : null;
  let transaction = state.tr;
  context.table.node.descendants((node, position) => {
    if (node.type !== schema.nodes.table_cell && node.type !== schema.nodes.table_header) {
      return true;
    }
    const colwidth = width === null
      ? null
      : Array(Math.max(1, Number(node.attrs.colspan) || 1)).fill(width);
    transaction = transaction.setNodeMarkup(context.table.start + position, undefined, {
      ...node.attrs,
      colwidth,
    });
    return false;
  });
  dispatch(transaction.scrollIntoView());
  return true;
}

function newDurableRequestId(prefix) {
  const safePrefix = String(prefix || 'request').replace(/[^a-z0-9_-]/gi, '').slice(0, 24) || 'request';
  if (globalThis.crypto && typeof globalThis.crypto.randomUUID === 'function') {
    return `${safePrefix}_${globalThis.crypto.randomUUID()}`;
  }
  if (globalThis.crypto && typeof globalThis.crypto.getRandomValues === 'function') {
    const bytes = new Uint8Array(16);
    globalThis.crypto.getRandomValues(bytes);
    const randomHex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
    return `${safePrefix}_${randomHex}`;
  }
  return `${safePrefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 14)}`;
}
export {buildFindDocumentIndex, captureSelection, clearParagraphLayout, clearTypography, commandForBlock, commandForList, commandMatchesQuery, copyPlainText, copyScientificClipboard, deleteActiveTable, findMatches, adjustParagraphIndent, focusWithoutScroll, insertHorizontalRule, insertTable, newDurableRequestId, normalizedFindQuery, restoreSelection, selectionIsInsideTable, setActiveTableColumnWidths, setBulletListMarker, setFontFamily, setFontSize, setHighlightColor, setLineSpacing, setParagraphLayoutAttribute, setTextAlignment, setOrderedListStyle, setTextColor, toggleActiveTableHeaderRow, toggleDropCap, toggleHighlightColor, writeBridge};

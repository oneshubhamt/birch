import assert from 'node:assert/strict';
import test from 'node:test';

globalThis.__RUSTNOTE_RUNTIME_MARKER__ = 'b4-behavior-test';
globalThis.window = {addEventListener() {}};

const {EditorState, TextSelection} = await import('prosemirror-state');
const {Fragment, Slice} = await import('prosemirror-model');
const {closeHistory, history, redo, undo} = await import('prosemirror-history');
const {CellSelection} = await import('prosemirror-tables');
const {schema} = await import('../../editor-src/document/schema.js');
const {
  blockStyleLabel,
  clearFirstLineIndentOnBackspace,
  normalizeAtomicBoundaryDocument,
  normalizePastedIndentSlice,
  normalizePastedIndentText,
  orderedListAttrs,
  paragraphLayoutStatePlugin,
  plugins,
  scientificTemplateCaretOffset,
  selectedBlockStyle,
  selectedListType,
  selectedLayoutTextblocks,
  splitTextblockWithParagraphDefaults,
  splitTextblockWithFirstLineIndent,
} = await import('../../editor-src/editor/core.js');
const {
  continueEmDashSequence,
  createIntentionalBlankLineFromAtomicBoundary,
  exitEmptyEmDashSequenceOnBackspace,
  firstBodyTextblockPosition,
  lineStartEmDashInputRule,
} = await import('../../editor-src/editor/interactions.js');
const {
  liftSelectedListItems,
  sinkSelectedListItems,
} = await import('../../editor-src/editor/list_system.js');
const {
  editorClipboardDataIsTrusted,
  plainTextPastePlugin,
  requestPlainTextPaste,
  stripUntrustedEditorLayoutSlice,
} = await import('../../editor-src/editor/clipboard.js');
const {
  buildFindDocumentIndex,
  commandForBlock,
  commandForList,
  findMatches,
  insertHorizontalRule,
  insertTable,
  normalizedFindQuery,
  setParagraphLayoutAttribute,
  setTextAlignment,
  toggleDropCap,
} = await import('../../editor-src/commands/index.js');

function paragraph(text = '', attrs = null) {
  return schema.nodes.paragraph.create(attrs, text ? schema.text(text) : null);
}

function firstPositionOfType(doc, type, occurrence = 0) {
  let seen = 0;
  let result = null;
  doc.descendants((node, position) => {
    if (result !== null || node.type !== type) return true;
    if (seen === occurrence) {
      result = position;
      return false;
    }
    seen += 1;
    return true;
  });
  return result;
}

function applyCommand(state, command) {
  let next = state;
  const applied = command(state, transaction => {
    next = state.apply(transaction);
  });
  assert.equal(applied, true);
  return next;
}

function createEditorState(doc) {
  return EditorState.create({doc, plugins: plugins()});
}

function applyEditorKey(state, key, modifiers = {}) {
  let next = state;
  const view = {
    get state() { return next; },
    dispatch(transaction) { next = next.apply(transaction); },
    endOfTextblock(direction, suppliedState = next) {
      const selection = suppliedState.selection;
      if (!(selection instanceof TextSelection) || !selection.empty) return false;
      return direction === 'backward'
        ? selection.$from.parentOffset === 0
        : selection.$from.parentOffset === selection.$from.parent.content.size;
    },
  };
  const event = {
    key,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    preventDefault() {},
    ...modifiers,
  };
  let handled = false;
  for (const plugin of next.plugins) {
    const handler = plugin.props.handleKeyDown;
    if (handler && handler(view, event)) {
      handled = true;
      break;
    }
  }
  return {state: next, handled};
}

function listDocument(kind, texts, options = {}) {
  const itemType = kind === 'task' ? schema.nodes.task_item : schema.nodes.list_item;
  const listType = kind === 'bullet'
    ? schema.nodes.bullet_list
    : kind === 'ordered'
      ? schema.nodes.ordered_list
      : schema.nodes.task_list;
  const items = texts.map((text, index) => {
    const paragraphAttrs = Array.isArray(options.paragraphAttrs)
      ? options.paragraphAttrs[index] ?? null
      : options.paragraphAttrs ?? null;
    return itemType.create(
      kind === 'task' ? {checked: Boolean(options.checked?.[index])} : null,
      [paragraph(text, paragraphAttrs)],
    );
  });
  const attrs = kind === 'ordered'
    ? orderedListAttrs(options.order ?? 1, options.style ?? 'auto', options.restart ?? false)
    : kind === 'bullet'
      ? {marker: options.marker ?? 'disc'}
      : null;
  return schema.nodes.doc.create(null, [listType.create(attrs, items)]);
}

function topLevelTypes(doc) {
  return Array.from({length: doc.childCount}, (_, index) => doc.child(index).type.name);
}

test('boundary promotion neutralizes paragraph layout before structural use', () => {
  const attachment = schema.nodes.attachment.create({
    attachmentId: 'att_boundary_layout',
    displayName: 'sample.pdf',
    kind: 'pdf',
  });
  const indentedEmpty = paragraph('', {
    lineSpacing: '1.5',
    textAlignment: 'center',
    leftIndent: 36,
    rightIndent: 18,
    firstLineIndent: 18,
    dropCap: false,
    boundary: false,
  });
  const normalized = normalizeAtomicBoundaryDocument(
    schema.nodes.doc.create(null, [indentedEmpty, attachment]),
  );
  const boundary = normalized.firstChild;
  assert.equal(boundary.type, schema.nodes.paragraph);
  assert.equal(boundary.attrs.boundary, true);
  assert.equal(boundary.attrs.lineSpacing, null);
  assert.equal(boundary.attrs.textAlignment, 'left');
  assert.equal(boundary.attrs.leftIndent, 0);
  assert.equal(boundary.attrs.rightIndent, 0);
  assert.equal(boundary.attrs.firstLineIndent, 0);
  assert.equal(boundary.attrs.dropCap, false);
});

test('atomic nodes receive exactly the required document-edge boundary paragraphs', () => {
  const attachment = schema.nodes.attachment.create({
    attachmentId: 'att_b4',
    displayName: 'sample.pdf',
    kind: 'pdf',
  });
  const raw = schema.nodes.doc.create(null, [attachment]);
  const normalized = normalizeAtomicBoundaryDocument(raw);
  assert.equal(normalized.childCount, 3);
  assert.equal(normalized.child(0).type, schema.nodes.paragraph);
  assert.equal(normalized.child(0).attrs.boundary, true);
  assert.equal(normalized.child(1).type, schema.nodes.attachment);
  assert.equal(normalized.child(2).type, schema.nodes.paragraph);
  assert.equal(normalized.child(2).attrs.boundary, true);

  const stable = normalizeAtomicBoundaryDocument(normalized);
  assert.equal(stable.eq(normalized), true, 'normalization must be idempotent');
});

test('block and list commands perform real ProseMirror transformations', () => {
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('Alpha')])});
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1, 6)));
  state = applyCommand(state, commandForBlock('heading1', state));
  assert.equal(state.doc.firstChild.type, schema.nodes.heading);
  assert.equal(state.doc.firstChild.attrs.level, 1);

  let listState = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('Item')])});
  listState = listState.apply(listState.tr.setSelection(TextSelection.create(listState.doc, 1, 5)));
  listState = applyCommand(listState, commandForList('bullet'));
  assert.equal(listState.doc.firstChild.type, schema.nodes.bullet_list);
  assert.equal(listState.doc.firstChild.firstChild.type, schema.nodes.list_item);
});

test('style labels follow the caret block and list context', () => {
  let headingState = EditorState.create({
    doc: schema.nodes.doc.create(null, [schema.nodes.heading.create({level: 3}, schema.text('Heading'))]),
  });
  headingState = headingState.apply(headingState.tr.setSelection(TextSelection.create(headingState.doc, 2)));
  assert.equal(selectedBlockStyle(headingState), 'h3');
  assert.equal(blockStyleLabel(selectedBlockStyle(headingState), selectedListType(headingState)), 'Heading 3');

  const list = schema.nodes.bullet_list.create(null, [
    schema.nodes.list_item.create(null, [paragraph('Item')]),
  ]);
  let listState = EditorState.create({doc: schema.nodes.doc.create(null, [list])});
  const paragraphPos = firstPositionOfType(listState.doc, schema.nodes.paragraph);
  listState = listState.apply(listState.tr.setSelection(TextSelection.create(listState.doc, paragraphPos + 1)));
  assert.equal(selectedBlockStyle(listState), 'list');
  assert.equal(selectedListType(listState), 'bullet_list');
  assert.equal(blockStyleLabel('list', 'bullet_list'), 'Bullet list');
});

test('leaving an outer list resets paragraph indentation without rewriting other paragraph layout', () => {
  const attrs = {
    lineSpacing: '1.5',
    textAlignment: 'right',
    leftIndent: 36,
    rightIndent: 18,
    firstLineIndent: 18,
    dropCap: false,
    boundary: false,
  };
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('Item', attrs)])});
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1, 5)));
  state = applyCommand(state, commandForList('bullet'));
  state = applyCommand(state, commandForList('bullet'));
  assert.equal(state.doc.firstChild.type, schema.nodes.paragraph);
  assert.equal(state.doc.firstChild.attrs.leftIndent, 0);
  assert.equal(state.doc.firstChild.attrs.rightIndent, 0);
  assert.equal(state.doc.firstChild.attrs.firstLineIndent, 0);
  assert.equal(state.doc.firstChild.attrs.lineSpacing, '1.5');
  assert.equal(state.doc.firstChild.attrs.textAlignment, 'right');
});

test('native middle-item lift resets indentation only after the item leaves the outer list', () => {
  const attrs = {
    lineSpacing: null,
    leftIndent: 36,
    rightIndent: 18,
    firstLineIndent: 18,
    dropCap: false,
    boundary: false,
  };
  const list = schema.nodes.bullet_list.create(null, [
    schema.nodes.list_item.create(null, [paragraph('First', attrs)]),
    schema.nodes.list_item.create(null, [paragraph('Middle', attrs)]),
    schema.nodes.list_item.create(null, [paragraph('Last', attrs)]),
  ]);
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [list])});
  const middlePos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, middlePos + 2)));
  state = applyCommand(state, commandForList('bullet'));

  assert.deepEqual(topLevelTypes(state.doc), ['bullet_list', 'paragraph', 'bullet_list']);
  assert.equal(state.doc.child(1).textContent, 'Middle');
  assert.equal(state.doc.child(1).attrs.leftIndent, 0);
  assert.equal(state.doc.child(1).attrs.rightIndent, 0);
  assert.equal(state.doc.child(1).attrs.firstLineIndent, 0);
  assert.equal(selectedListType(state), null);
});

test('Normal text on a current list item exits that item from the list', () => {
  const list = schema.nodes.bullet_list.create(null, [
    schema.nodes.list_item.create(null, [paragraph('First')]),
    schema.nodes.list_item.create(null, [paragraph('Middle')]),
    schema.nodes.list_item.create(null, [paragraph('Last')]),
  ]);
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [list])});
  const middlePos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, middlePos + 2)));
  state = applyCommand(state, commandForBlock('paragraph', state));

  assert.equal(state.doc.childCount, 3);
  assert.equal(state.doc.child(0).type, schema.nodes.bullet_list);
  assert.equal(state.doc.child(1).type, schema.nodes.paragraph);
  assert.equal(state.doc.child(1).textContent, 'Middle');
  assert.equal(state.doc.child(2).type, schema.nodes.bullet_list);
  assert.equal(selectedListType(state), null);
});

test('first body textblock lookup is stable in multi-paragraph documents', () => {
  const doc = schema.nodes.doc.create(null, [
    paragraph('First'),
    paragraph('Second'),
    paragraph('Third'),
  ]);
  assert.equal(firstBodyTextblockPosition(doc), 0);
});

test('nested list outdent preserves paragraph layout while the item remains a list item', () => {
  const attrs = {
    lineSpacing: null,
    leftIndent: 36,
    rightIndent: 18,
    firstLineIndent: 18,
    dropCap: false,
    boundary: false,
  };
  const nested = schema.nodes.bullet_list.create(null, [
    schema.nodes.list_item.create(null, [
      paragraph('Outer'),
      schema.nodes.bullet_list.create(null, [
        schema.nodes.list_item.create(null, [paragraph('Inner', attrs)]),
      ]),
    ]),
  ]);
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [nested])});
  const innerPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, innerPos + 1)));
  state = applyCommand(state, liftSelectedListItems);
  const liftedInnerPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  const liftedInner = state.doc.nodeAt(liftedInnerPos);
  assert.equal(selectedListType(state), 'bullet_list');
  assert.equal(liftedInner.attrs.leftIndent, 36);
  assert.equal(liftedInner.attrs.rightIndent, 18);
  assert.equal(liftedInner.attrs.firstLineIndent, 18);
});


const listBehaviorCases = [
  {kind: 'bullet'},
  {kind: 'ordered'},
  {kind: 'task'},
];

for (const {kind} of listBehaviorCases) {
  const listType = kind === 'bullet' ? 'bullet_list' : kind === 'ordered' ? 'ordered_list' : 'task_list';

  test(`${kind} list toggle-off resets indentation after leaving the outer list`, () => {
    let state = createEditorState(listDocument(kind, ['Item'], {
      paragraphAttrs: {
        lineSpacing: '1.5',
        textAlignment: 'right',
        leftIndent: 36,
        rightIndent: 18,
        firstLineIndent: 18,
        dropCap: false,
        boundary: false,
      },
    }));
    const paragraphPos = firstPositionOfType(state.doc, schema.nodes.paragraph);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, paragraphPos + 1)));
    const action = kind === 'bullet' ? 'bullet' : kind === 'ordered' ? 'numbered' : 'checklist';
    state = applyCommand(state, commandForList(action));

    assert.equal(state.doc.firstChild.type, schema.nodes.paragraph);
    assert.equal(state.doc.firstChild.attrs.leftIndent, 0);
    assert.equal(state.doc.firstChild.attrs.rightIndent, 0);
    assert.equal(state.doc.firstChild.attrs.firstLineIndent, 0);
    assert.equal(state.doc.firstChild.attrs.lineSpacing, '1.5');
    assert.equal(state.doc.firstChild.attrs.textAlignment, 'right');
  });

  test(`${kind} middle-item Backspace lifts the item out of the outer list and resets indentation`, () => {
    let state = createEditorState(listDocument(kind, ['First', 'Middle', 'Last'], {
      ...(kind === 'ordered' ? {order: 4, style: 'lower-alpha', restart: true} : {}),
      paragraphAttrs: [
        null,
        {
          lineSpacing: '1.5',
          textAlignment: 'right',
          leftIndent: 36,
          rightIndent: 18,
          firstLineIndent: 18,
          dropCap: false,
          boundary: false,
        },
        null,
      ],
    }));
    const middlePos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, middlePos + 1)));

    const result = applyEditorKey(state, 'Backspace');
    assert.equal(result.handled, true);
    assert.deepEqual(topLevelTypes(result.state.doc), [listType, 'paragraph', listType]);
    const lifted = result.state.doc.child(1);
    assert.equal(lifted.textContent, 'Middle');
    assert.equal(lifted.attrs.leftIndent, 0);
    assert.equal(lifted.attrs.rightIndent, 0);
    assert.equal(lifted.attrs.firstLineIndent, 0);
    assert.equal(lifted.attrs.lineSpacing, '1.5');
    assert.equal(lifted.attrs.textAlignment, 'right');
    assert.equal(selectedListType(result.state), null);
    if (kind === 'ordered') {
      assert.equal(result.state.doc.child(0).attrs.order, 4);
      assert.equal(result.state.doc.child(0).attrs.style, 'lower-alpha');
      assert.equal(result.state.doc.child(0).attrs.restart, true);
      assert.equal(result.state.doc.child(2).attrs.order, 6, 'the trailing fragment must keep its original visible number');
      assert.equal(result.state.doc.child(2).attrs.style, 'lower-alpha');
      assert.equal(result.state.doc.child(2).attrs.restart, false, 'a split continuation is not a new explicit restart');
    }
    assert.equal(schema.nodeFromJSON(result.state.doc.toJSON()).eq(result.state.doc), true);
  });

  test(`${kind} Backspace inside list-item text is not intercepted by the structural list keymap`, () => {
    let state = createEditorState(listDocument(kind, ['First', 'Middle', 'Last']));
    const middlePos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, middlePos + 3)));
    const before = state.doc;
    const result = applyEditorKey(state, 'Backspace');
    assert.equal(result.handled, false);
    assert.equal(result.state.doc.eq(before), true);
  });

  test(`${kind} Backspace twice exits the item and then merges text without recreating list membership`, () => {
    const options = kind === 'ordered'
      ? {order: 4, style: 'lower-alpha', restart: true}
      : kind === 'bullet'
        ? {marker: 'dash'}
        : {};
    let state = createEditorState(listDocument(kind, ['First', 'Middle', 'Third'], options));
    const middlePos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, middlePos + 1)));

    let result = applyEditorKey(state, 'Backspace');
    assert.equal(result.handled, true);
    state = result.state;
    assert.deepEqual(topLevelTypes(state.doc), [listType, 'paragraph', listType]);
    assert.equal(state.doc.child(1).textContent, 'Middle');

    result = applyEditorKey(state, 'Backspace');
    assert.equal(result.handled, true);
    state = result.state;
    assert.deepEqual(topLevelTypes(state.doc), [listType], 'compatible list fragments should reunite after the textblocks merge');
    assert.equal(state.doc.firstChild.childCount, 2);
    assert.equal(state.doc.firstChild.child(0).childCount, 1, 'the former paragraph must merge into the previous textblock, not become a second block in its list item');
    assert.equal(state.doc.firstChild.child(0).firstChild.textContent, 'FirstMiddle');
    assert.equal(state.doc.firstChild.child(1).firstChild.textContent, 'Third');
    if (kind === 'ordered') {
      assert.equal(state.doc.firstChild.attrs.order, 4);
      assert.equal(state.doc.firstChild.attrs.style, 'lower-alpha');
      assert.equal(state.doc.firstChild.attrs.restart, true);
    }
    if (kind === 'bullet') assert.equal(state.doc.firstChild.attrs.marker, 'dash');
    assert.equal(schema.nodeFromJSON(state.doc.toJSON()).eq(state.doc), true);
  });

  test(`${kind} explicitly reapplying the list joins compatible fragments instead of creating a list island`, () => {
    const options = kind === 'ordered'
      ? {order: 4, style: 'lower-alpha', restart: true}
      : kind === 'bullet'
        ? {marker: 'dash'}
        : {};
    let state = createEditorState(listDocument(kind, ['First', 'Middle', 'Third'], options));
    const middlePos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, middlePos + 1)));
    state = applyEditorKey(state, 'Backspace').state;
    assert.deepEqual(topLevelTypes(state.doc), [listType, 'paragraph', listType]);

    const action = kind === 'bullet' ? 'bullet' : kind === 'ordered' ? 'numbered' : 'checklist';
    state = applyCommand(state, commandForList(action));
    assert.deepEqual(topLevelTypes(state.doc), [listType]);
    assert.equal(state.doc.firstChild.childCount, 3);
    assert.deepEqual(
      Array.from({length: 3}, (_, index) => state.doc.firstChild.child(index).firstChild.textContent),
      ['First', 'Middle', 'Third'],
    );
    if (kind === 'ordered') {
      assert.equal(state.doc.firstChild.attrs.order, 4);
      assert.equal(state.doc.firstChild.attrs.style, 'lower-alpha');
      assert.equal(state.doc.firstChild.attrs.restart, true);
    }
    if (kind === 'bullet') assert.equal(state.doc.firstChild.attrs.marker, 'dash');
  });

  test(`${kind} empty list item Enter is handled by native split/base-keymap composition`, () => {
    let state = createEditorState(listDocument(kind, ['First', '', 'Third'], kind === 'ordered' ? {order: 1} : {}));
    const emptyPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, emptyPos + 1)));
    const before = state.doc;
    const result = applyEditorKey(state, 'Enter');
    assert.equal(result.handled, true);
    assert.equal(result.state.doc.eq(before), false);
    assert.equal(schema.nodeFromJSON(result.state.doc.toJSON()).eq(result.state.doc), true);
  });

  test(`${kind} Tab and Shift-Tab use native sink/lift commands`, () => {
    let state = createEditorState(listDocument(kind, ['First', 'Second']));
    const secondPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, secondPos + 1)));

    let result = applyEditorKey(state, 'Tab');
    assert.equal(result.handled, true);
    state = result.state;
    assert.equal(state.doc.firstChild.childCount, 1);

    result = applyEditorKey(state, 'Tab', {shiftKey: true});
    assert.equal(result.handled, true);
    state = result.state;
    assert.equal(state.doc.firstChild.childCount, 2);
  });

  test(`${kind} toolbar toggle uses native lift for a multi-item selection`, () => {
    let state = createEditorState(listDocument(kind, ['One', 'Two', 'Three', 'Four'], kind === 'ordered' ? {order: 5, style: 'lower-roman', restart: true} : {}));
    const secondPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    const thirdPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 2);
    const third = state.doc.nodeAt(thirdPos);
    state = state.apply(state.tr.setSelection(TextSelection.create(
      state.doc,
      secondPos + 1,
      thirdPos + 1 + third.content.size,
    )));
    state = applyCommand(state, commandForList(kind === 'bullet' ? 'bullet' : kind === 'ordered' ? 'numbered' : 'checklist'));
    assert.deepEqual(topLevelTypes(state.doc), [listType, 'paragraph', 'paragraph', listType]);
    assert.equal(state.doc.child(1).textContent, 'Two');
    assert.equal(state.doc.child(2).textContent, 'Three');
  });

  test(`${kind} native list transformations survive serialization and undo/redo`, () => {
    const original = listDocument(kind, ['First', 'Middle', 'Last'], kind === 'ordered' ? {order: 4, style: 'lower-alpha', restart: true} : {});
    let state = createEditorState(original);
    const middlePos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, middlePos + 1)));
    state = applyCommand(state, commandForList(kind === 'bullet' ? 'bullet' : kind === 'ordered' ? 'numbered' : 'checklist'));
    const transformed = state.doc;
    assert.equal(schema.nodeFromJSON(transformed.toJSON()).eq(transformed), true);

    state = applyCommand(state, undo);
    assert.equal(state.doc.eq(original), true);
    state = applyCommand(state, redo);
    assert.equal(state.doc.eq(transformed), true);
  });
}

test('ordinary paragraph after a list uses narrow Backspace joining while Delete remains generic', () => {
  const list = listDocument('bullet', ['Item']).firstChild;

  let state = createEditorState(schema.nodes.doc.create(null, [list, paragraph('After')]));
  const afterPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, afterPos + 1)));
  let result = applyEditorKey(state, 'Backspace');
  assert.equal(result.handled, true);
  assert.deepEqual(topLevelTypes(result.state.doc), ['bullet_list']);
  assert.equal(result.state.doc.firstChild.childCount, 1);
  assert.equal(result.state.doc.firstChild.firstChild.childCount, 1);
  assert.equal(result.state.doc.firstChild.firstChild.firstChild.textContent, 'ItemAfter');

  state = createEditorState(schema.nodes.doc.create(null, [list, paragraph('After')]));
  const itemPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 0);
  const item = state.doc.nodeAt(itemPos);
  state = state.apply(state.tr.setSelection(TextSelection.create(
    state.doc,
    itemPos + 1 + item.content.size,
  )));
  const beforeDelete = state.doc;
  result = applyEditorKey(state, 'Delete');
  assert.equal(result.handled, true);
  assert.equal(result.state.doc.eq(beforeDelete), false, 'Delete must reach ProseMirror instead of a Rustnote no-op');
});

test('stale atomic-boundary markers at list edges are normalized to ordinary paragraphs', () => {
  const list = listDocument('bullet', ['Item']).firstChild;
  const boundary = paragraph('', {
    lineSpacing: null,
    leftIndent: 0,
    rightIndent: 0,
    firstLineIndent: 0,
    dropCap: false,
    boundary: true,
  });

  const trailingNormalized = normalizeAtomicBoundaryDocument(schema.nodes.doc.create(null, [list, boundary]));
  assert.deepEqual(topLevelTypes(trailingNormalized), ['bullet_list', 'paragraph']);
  assert.equal(trailingNormalized.lastChild.attrs.boundary, false);

  const leadingNormalized = normalizeAtomicBoundaryDocument(schema.nodes.doc.create(null, [boundary, list]));
  assert.deepEqual(topLevelTypes(leadingNormalized), ['paragraph', 'bullet_list']);
  assert.equal(leadingNormalized.firstChild.attrs.boundary, false);

  let state = createEditorState(schema.nodes.doc.create(null, [list, boundary]));
  const boundaryPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, boundaryPos + 1)));
  let result = applyEditorKey(state, 'Backspace');
  assert.equal(result.handled, true);

  let ordinaryState = createEditorState(schema.nodes.doc.create(null, [list, paragraph('')]));
  const ordinaryBoundaryPos = firstPositionOfType(ordinaryState.doc, schema.nodes.paragraph, 1);
  ordinaryState = ordinaryState.apply(
    ordinaryState.tr.setSelection(TextSelection.create(ordinaryState.doc, ordinaryBoundaryPos + 1)),
  );
  const ordinaryBackspace = applyEditorKey(ordinaryState, 'Backspace');
  assert.equal(ordinaryBackspace.handled, true);
  assert.equal(
    result.state.doc.eq(ordinaryBackspace.state.doc),
    true,
    'a demoted stale marker must follow ordinary blank-paragraph Backspace semantics',
  );

  state = createEditorState(schema.nodes.doc.create(null, [list, boundary]));
  const itemPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 0);
  const item = state.doc.nodeAt(itemPos);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, itemPos + 1 + item.content.size)));
  result = applyEditorKey(state, 'Delete');
  assert.equal(result.handled, true);

  ordinaryState = createEditorState(schema.nodes.doc.create(null, [list, paragraph('')]));
  const ordinaryItemPos = firstPositionOfType(ordinaryState.doc, schema.nodes.paragraph, 0);
  const ordinaryItem = ordinaryState.doc.nodeAt(ordinaryItemPos);
  ordinaryState = ordinaryState.apply(ordinaryState.tr.setSelection(TextSelection.create(
    ordinaryState.doc,
    ordinaryItemPos + 1 + ordinaryItem.content.size,
  )));
  const ordinaryDelete = applyEditorKey(ordinaryState, 'Delete');
  assert.equal(ordinaryDelete.handled, true);
  assert.equal(
    result.state.doc.eq(ordinaryDelete.state.doc),
    true,
    'a demoted stale marker must follow ordinary blank-paragraph Delete semantics',
  );
});

test('a real blank paragraph beside list fragments remains editable through normal ProseMirror Backspace', () => {
  const upper = listDocument('bullet', ['First']).firstChild;
  const lower = listDocument('bullet', ['Third']).firstChild;
  let state = createEditorState(schema.nodes.doc.create(null, [upper, paragraph('Middle'), paragraph(''), lower]));
  const emptyPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 2);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, emptyPos + 1)));
  const before = state.doc;
  const result = applyEditorKey(state, 'Backspace');
  assert.equal(result.handled, true);
  assert.equal(result.state.doc.eq(before), false, 'a real blank paragraph must not be trapped by list policy');
  assert.equal(schema.nodeFromJSON(result.state.doc.toJSON()).eq(result.state.doc), true);
});

test('Delete from a multi-block list item is left to normal ProseMirror structural editing', () => {
  const item = schema.nodes.list_item.create(null, [paragraph('Lead'), paragraph('Tail')]);
  const list = schema.nodes.bullet_list.create({marker: 'disc'}, [item]);
  let state = createEditorState(schema.nodes.doc.create(null, [list, paragraph('After')]));
  const tailPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  const tail = state.doc.nodeAt(tailPos);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, tailPos + 1 + tail.content.size)));
  const before = state.doc;
  const result = applyEditorKey(state, 'Delete');
  assert.equal(result.handled, true);
  assert.equal(result.state.doc.eq(before), false);
  assert.equal(schema.nodeFromJSON(result.state.doc.toJSON()).eq(result.state.doc), true);
});

test('Normal text uses the same native multi-item lift authority as list toggle', () => {
  let state = createEditorState(listDocument('bullet', ['One', 'Two', 'Three', 'Four']));
  const secondPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  const thirdPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 2);
  const third = state.doc.nodeAt(thirdPos);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, secondPos + 1, thirdPos + 1 + third.content.size)));
  state = applyCommand(state, commandForBlock('paragraph', state));
  assert.deepEqual(topLevelTypes(state.doc), ['bullet_list', 'paragraph', 'paragraph', 'bullet_list']);
  assert.equal(state.doc.child(1).textContent, 'Two');
  assert.equal(state.doc.child(2).textContent, 'Three');
});

test('native Backspace after list creation remains one undoable ProseMirror transaction', () => {
  let state = createEditorState(schema.nodes.doc.create(null, [paragraph('Alpha'), paragraph('Beta')]));
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1, state.doc.content.size - 1)));
  state = applyCommand(state, commandForList('bullet'));
  const toggled = state.doc;
  state = state.apply(closeHistory(state.tr));
  const secondPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, secondPos + 1)));
  const result = applyEditorKey(state, 'Backspace');
  assert.equal(result.handled, true);
  state = result.state;
  const transformed = state.doc;
  assert.equal(transformed.eq(toggled), false);
  state = applyCommand(state, undo);
  assert.equal(state.doc.eq(toggled), true);
  state = applyCommand(state, redo);
  assert.equal(state.doc.eq(transformed), true);
});

test('list type conversion remains explicit product policy while structural exit uses native list commands', () => {
  let state = createEditorState(listDocument('bullet', ['One', 'Two']));
  const firstPos = firstPositionOfType(state.doc, schema.nodes.paragraph);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, firstPos + 1)));
  state = applyCommand(state, commandForList('numbered'));
  assert.equal(state.doc.firstChild.type, schema.nodes.ordered_list);
  assert.equal(state.doc.firstChild.firstChild.type, schema.nodes.list_item);

  state = applyCommand(state, commandForList('checklist'));
  assert.equal(state.doc.firstChild.type, schema.nodes.task_list);
  assert.equal(state.doc.firstChild.firstChild.type, schema.nodes.task_item);
  assert.equal(state.doc.firstChild.firstChild.attrs.checked, false);

  state = applyCommand(state, commandForList('bullet'));
  assert.equal(state.doc.firstChild.type, schema.nodes.bullet_list);
  assert.equal(state.doc.firstChild.firstChild.type, schema.nodes.list_item);
});

test('numbered list application before a continuation derives the preceding number and joins it', () => {
  const next = listDocument('ordered', ['Six'], {order: 6, style: 'lower-alpha', restart: false}).firstChild;
  let state = createEditorState(schema.nodes.doc.create(null, [paragraph('Five'), next]));
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1)));
  state = applyCommand(state, commandForList('numbered'));
  assert.deepEqual(topLevelTypes(state.doc), ['ordered_list']);
  assert.equal(state.doc.firstChild.attrs.order, 5);
  assert.equal(state.doc.firstChild.attrs.style, 'lower-alpha');
  assert.equal(state.doc.firstChild.childCount, 2);
  assert.equal(state.doc.firstChild.child(0).firstChild.textContent, 'Five');
  assert.equal(state.doc.firstChild.child(1).firstChild.textContent, 'Six');
});

test('an explicitly restarted following numbered list remains a separate list', () => {
  const restarted = listDocument('ordered', ['Restarted'], {order: 1, style: 'lower-alpha', restart: true}).firstChild;
  let state = createEditorState(schema.nodes.doc.create(null, [paragraph('Before restart'), restarted]));
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1)));
  state = applyCommand(state, commandForList('numbered'));
  assert.deepEqual(topLevelTypes(state.doc), ['ordered_list', 'ordered_list']);
  assert.equal(state.doc.child(1).attrs.restart, true);
});

test('dash-space at the start of a textblock becomes ordinary em-dash text', () => {
  const rule = lineStartEmDashInputRule();
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('- ')])});
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 3)));
  const match = rule.match.exec('- ');
  assert.ok(match);
  const transaction = rule.handler(state, match, 1, 3);
  assert.ok(transaction);
  state = state.apply(transaction);
  assert.equal(state.doc.firstChild.textContent, '— ');
  assert.equal(state.doc.firstChild.type, schema.nodes.paragraph);
});

test('em-dash text continues on Enter and an empty continuation exits with one Backspace', () => {
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('— Alpha')])});
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, state.doc.firstChild.nodeSize - 1)));
  state = applyCommand(state, continueEmDashSequence);
  assert.equal(state.doc.childCount, 2);
  assert.equal(state.doc.child(0).textContent, '— Alpha');
  assert.equal(state.doc.child(1).textContent, '— ');

  state = applyCommand(state, exitEmptyEmDashSequenceOnBackspace);
  assert.equal(state.doc.child(1).textContent, '');
  assert.equal(state.doc.child(1).type, schema.nodes.paragraph);
});

test('Enter on an atomic boundary preserves an intentional blank line through normalization, save, and undo', () => {
  const attachment = schema.nodes.attachment.create({
    attachmentId: 'att_blank_line',
    displayName: 'sample.pdf',
    kind: 'pdf',
  });
  const leadingBoundary = paragraph('', {boundary: true});
  const trailingBoundary = paragraph('', {boundary: true});
  const original = schema.nodes.doc.create(null, [leadingBoundary, attachment, trailingBoundary]);
  let state = EditorState.create({doc: original, plugins: [history()]});
  const trailingPos = firstPositionOfType(state.doc, schema.nodes.paragraph, 1);
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, trailingPos + 1)));
  state = applyCommand(state, createIntentionalBlankLineFromAtomicBoundary);

  assert.equal(state.doc.childCount, 4);
  assert.equal(state.doc.child(0).attrs.boundary, true);
  assert.equal(state.doc.child(1).type, schema.nodes.attachment);
  assert.equal(state.doc.child(2).type, schema.nodes.paragraph);
  assert.equal(state.doc.child(2).attrs.boundary, false);
  assert.equal(state.doc.child(2).content.size, 0);
  assert.equal(state.doc.child(3).type, schema.nodes.paragraph);
  assert.equal(state.doc.child(3).attrs.boundary, false);
  assert.equal(state.doc.child(3).content.size, 0);
  assert.equal(state.selection.$from.parent.eq(state.doc.child(3)), true);

  const normalized = normalizeAtomicBoundaryDocument(state.doc);
  assert.equal(normalized.eq(state.doc), true, 'intentional blank lines must survive boundary normalization');
  const restored = schema.nodeFromJSON(state.doc.toJSON());
  assert.equal(restored.eq(state.doc), true, 'intentional blank lines must survive document serialization');

  let undone = state;
  const undoneApplied = undo(state, transaction => { undone = state.apply(transaction); });
  assert.equal(undoneApplied, true);
  assert.equal(undone.doc.eq(original), true, 'one undo must restore the original atomic boundary');
});

test('intentional atomic-boundary blank lines work for rules and tables too', () => {
  const rule = schema.nodes.horizontal_rule.create();
  const cell = schema.nodes.table_cell.createAndFill();
  const row = schema.nodes.table_row.create(null, [cell]);
  const table = schema.nodes.table.create(null, [row]);

  for (const atomic of [rule, table]) {
    const leadingBoundary = paragraph('', {boundary: true});
    const trailingBoundary = paragraph('', {boundary: true});
    let state = EditorState.create({doc: schema.nodes.doc.create(null, [leadingBoundary, atomic, trailingBoundary])});
    const trailingPos = state.doc.content.size - state.doc.lastChild.nodeSize;
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, trailingPos + 1)));
    state = applyCommand(state, createIntentionalBlankLineFromAtomicBoundary);
    assert.equal(state.doc.childCount, 4);
    assert.equal(state.doc.child(0).attrs.boundary, true);
    assert.equal(state.doc.child(2).attrs.boundary, false);
    assert.equal(state.doc.child(3).attrs.boundary, false);
    assert.equal(normalizeAtomicBoundaryDocument(state.doc).eq(state.doc), true);
  }
});

test('atomic insertion commands keep a reachable paragraph after inserted content', () => {
  let ruleState = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('')])});
  ruleState = applyCommand(ruleState, insertHorizontalRule);
  assert.equal(ruleState.doc.child(0).type, schema.nodes.horizontal_rule);
  assert.equal(ruleState.doc.lastChild.type, schema.nodes.paragraph);

  let tableState = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('')])});
  tableState = applyCommand(tableState, insertTable(2, 2, true));
  assert.equal(tableState.doc.child(0).type, schema.nodes.table);
  assert.equal(tableState.doc.child(0).childCount, 2);
  assert.equal(tableState.doc.child(0).firstChild.firstChild.type, schema.nodes.table_header);
  assert.equal(tableState.doc.lastChild.type, schema.nodes.paragraph);
});

test('find maps folded text and scientific content back to source positions', () => {
  const math = schema.nodes.math_inline.create(null, schema.text('E=mc^2'));
  const doc = schema.nodes.doc.create(null, [
    schema.nodes.paragraph.create(null, [schema.text('Straße '), math, schema.text(' ΩMEGA')]),
  ]);
  const index = buildFindDocumentIndex(doc);

  const omega = findMatches(index, normalizedFindQuery('ωmega'));
  assert.equal(omega.length, 1);
  assert.equal(omega[0].decorationKind, 'inline');

  const scientific = findMatches(index, normalizedFindQuery('MC^2'));
  assert.equal(scientific.length, 1);
  assert.equal(scientific[0].decorationKind, 'node');
  assert.ok(scientific[0].decorationFrom < scientific[0].decorationTo);
});

test('ordered-list and scientific helper normalization is stable', () => {
  assert.deepEqual(orderedListAttrs(-9, 'not-a-style', 1), {
    order: 1,
    style: schema.nodes.ordered_list.defaultAttrs.style,
    restart: true,
  });
  assert.equal(scientificTemplateCaretOffset('\\frac{}{}'), 6);
  assert.equal(scientificTemplateCaretOffset('x+y'), 3);
});


test('paragraph and heading conversion preserve canonical textblock layout', () => {
  const attrs = {
    lineSpacing: '1.5',
    textAlignment: 'center',
    leftIndent: 36,
    rightIndent: 18,
    firstLineIndent: 18,
    dropCap: false,
    boundary: false,
  };
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('Alpha', attrs)])});
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1, 6)));
  state = applyCommand(state, commandForBlock('heading2', state));
  assert.equal(state.doc.firstChild.type, schema.nodes.heading);
  assert.equal(state.doc.firstChild.attrs.level, 2);
  assert.equal(state.doc.firstChild.attrs.lineSpacing, '1.5');
  assert.equal(state.doc.firstChild.attrs.textAlignment, 'center');
  assert.equal(state.doc.firstChild.attrs.leftIndent, 36);
  assert.equal(state.doc.firstChild.attrs.rightIndent, 18);
  assert.equal(state.doc.firstChild.attrs.firstLineIndent, 18);

  state = applyCommand(state, commandForBlock('paragraph', state));
  assert.equal(state.doc.firstChild.type, schema.nodes.paragraph);
  assert.equal(state.doc.firstChild.attrs.lineSpacing, '1.5');
  assert.equal(state.doc.firstChild.attrs.textAlignment, 'center');
  assert.equal(state.doc.firstChild.attrs.leftIndent, 36);
  assert.equal(state.doc.firstChild.attrs.rightIndent, 18);
  assert.equal(state.doc.firstChild.attrs.firstLineIndent, 18);
});

test('paragraph alignment supports left, center, right, and justify through one canonical attribute', () => {
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('Alpha')])});
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1, 6)));
  for (const alignment of ['center', 'right', 'justify', 'left']) {
    state = applyCommand(state, setTextAlignment(alignment));
    assert.equal(state.doc.firstChild.attrs.textAlignment, alignment);
  }
  assert.equal(setTextAlignment('diagonal')(state), false);
});

test('paragraph layout commands clamp values and keep drop caps exclusive with first-line indent', () => {
  let state = EditorState.create({doc: schema.nodes.doc.create(null, [paragraph('Alpha')])});
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 1, 6)));
  state = applyCommand(state, setParagraphLayoutAttribute('leftIndent', 999));
  assert.equal(state.doc.firstChild.attrs.leftIndent, 144);
  state = applyCommand(state, setParagraphLayoutAttribute('firstLineIndent', 999));
  assert.equal(state.doc.firstChild.attrs.firstLineIndent, 72);
  state = applyCommand(state, toggleDropCap);
  assert.equal(state.doc.firstChild.attrs.dropCap, true);
  assert.equal(state.doc.firstChild.attrs.firstLineIndent, 0);
});

test('paragraph layout excludes list items, boundary paragraphs, code blocks, and multi-cell table selections', () => {
  const list = schema.nodes.bullet_list.create(null, [
    schema.nodes.list_item.create(null, [paragraph('Item')]),
  ]);
  let listState = EditorState.create({doc: schema.nodes.doc.create(null, [list])});
  const listParagraphPos = firstPositionOfType(listState.doc, schema.nodes.paragraph);
  listState = listState.apply(listState.tr.setSelection(TextSelection.create(listState.doc, listParagraphPos + 1)));
  assert.equal(selectedLayoutTextblocks(listState).length, 0);
  assert.equal(setParagraphLayoutAttribute('leftIndent', 18)(listState), false);

  const boundary = paragraph('', {boundary: true});
  let boundaryState = EditorState.create({doc: schema.nodes.doc.create(null, [boundary])});
  boundaryState = boundaryState.apply(boundaryState.tr.setSelection(TextSelection.create(boundaryState.doc, 1)));
  assert.equal(selectedLayoutTextblocks(boundaryState).length, 0);

  const code = schema.nodes.code_block.create(null, schema.text('let x = 1;'));
  let codeState = EditorState.create({doc: schema.nodes.doc.create(null, [code])});
  codeState = codeState.apply(codeState.tr.setSelection(TextSelection.create(codeState.doc, 1)));
  assert.equal(selectedLayoutTextblocks(codeState).length, 0);

  const quote = schema.nodes.blockquote.create(null, [paragraph('Quoted')]);
  let quoteState = EditorState.create({
    doc: schema.nodes.doc.create(null, [quote]),
    plugins: [paragraphLayoutStatePlugin()],
  });
  const quotedParagraphPos = firstPositionOfType(quoteState.doc, schema.nodes.paragraph);
  quoteState = quoteState.apply(quoteState.tr.setSelection(TextSelection.create(quoteState.doc, quotedParagraphPos + 1)));
  assert.equal(splitTextblockWithFirstLineIndent(quoteState), false, 'auto-indent stays out of semantic quote containers');

  const cellA = schema.nodes.table_cell.createAndFill();
  const cellB = schema.nodes.table_cell.createAndFill();
  const row = schema.nodes.table_row.create(null, [cellA, cellB]);
  const table = schema.nodes.table.create(null, [row]);
  let tableState = EditorState.create({doc: schema.nodes.doc.create(null, [table])});
  const firstCellPos = firstPositionOfType(tableState.doc, schema.nodes.table_cell, 0);
  const secondCellPos = firstPositionOfType(tableState.doc, schema.nodes.table_cell, 1);
  tableState = tableState.apply(tableState.tr.setSelection(CellSelection.create(tableState.doc, firstCellPos, secondCellPos)));
  assert.equal(selectedLayoutTextblocks(tableState).length, 0);
});

test('new-paragraph line-spacing default is persisted without restyling existing paragraphs', () => {
  let state = EditorState.create({
    doc: schema.nodes.doc.create(null, [paragraph('Alpha')]),
    plugins: [paragraphLayoutStatePlugin()],
  });
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 6)));
  state = applyCommand(state, (current, dispatch) =>
    splitTextblockWithParagraphDefaults(current, dispatch, {defaultLineSpacing: '1.15'}));
  assert.equal(state.doc.child(0).attrs.lineSpacing, null, 'existing paragraph keeps Auto spacing');
  assert.equal(state.doc.child(1).attrs.lineSpacing, '1.15', 'new paragraph stores the configured default');

  let explicit = EditorState.create({
    doc: schema.nodes.doc.create(null, [paragraph('Beta', {lineSpacing: '1.75'})]),
    plugins: [paragraphLayoutStatePlugin()],
  });
  explicit = explicit.apply(explicit.tr.setSelection(TextSelection.create(explicit.doc, 5)));
  explicit = applyCommand(explicit, (current, dispatch) =>
    splitTextblockWithParagraphDefaults(current, dispatch, {defaultLineSpacing: '1.15'}));
  assert.equal(explicit.doc.child(1).attrs.lineSpacing, '1.75', 'explicit paragraph spacing inherits ahead of the preference default');

  const auto = EditorState.create({
    doc: schema.nodes.doc.create(null, [paragraph('Gamma')]),
    plugins: [paragraphLayoutStatePlugin()],
  });
  assert.equal(
    splitTextblockWithParagraphDefaults(auto, undefined, {defaultLineSpacing: null}),
    false,
    'Auto does not replace native Enter when no paragraph default needs applying',
  );
});

test('paragraph defaults apply when Enter at the end of a heading creates a normal paragraph', () => {
  let state = EditorState.create({
    doc: schema.nodes.doc.create(null, [schema.nodes.heading.create({level: 2}, schema.text('Heading'))]),
    plugins: [paragraphLayoutStatePlugin()],
  });
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 8)));
  state = applyCommand(state, (current, dispatch) =>
    splitTextblockWithParagraphDefaults(current, dispatch, {
      autoFirstLineIndent: 18,
      defaultLineSpacing: '1.15',
    }));
  assert.equal(state.doc.childCount, 2);
  assert.equal(state.doc.child(0).type, schema.nodes.heading);
  assert.equal(state.doc.child(1).type, schema.nodes.paragraph);
  assert.equal(state.doc.child(1).attrs.firstLineIndent, 18);
  assert.equal(state.doc.child(1).attrs.lineSpacing, '1.15');
  assert.equal(state.doc.child(1).attrs.textAlignment, 'left');
});

test('auto-indent split and first Backspace use explicit formatting with a manual override', () => {
  let state = EditorState.create({
    doc: schema.nodes.doc.create(null, [paragraph('Alpha')]),
    plugins: [paragraphLayoutStatePlugin()],
  });
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, 6)));
  state = applyCommand(state, splitTextblockWithFirstLineIndent);
  assert.equal(state.doc.childCount, 2);
  assert.equal(state.doc.child(1).attrs.firstLineIndent, 18);

  let customState = EditorState.create({
    doc: schema.nodes.doc.create(null, [paragraph('Custom')]),
    plugins: [paragraphLayoutStatePlugin()],
  });
  customState = customState.apply(customState.tr.setSelection(TextSelection.create(customState.doc, 7)));
  customState = applyCommand(customState, (current, dispatch) =>
    splitTextblockWithFirstLineIndent(current, dispatch, 30));
  assert.equal(customState.doc.child(1).attrs.firstLineIndent, 30, 'configured auto-indent amount is applied');

  const secondStart = state.doc.child(0).nodeSize + 1;
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, secondStart)));
  state = applyCommand(state, clearFirstLineIndentOnBackspace);
  assert.equal(state.doc.child(1).attrs.firstLineIndent, 0);

  state = state.apply(state.tr.insertText('Beta'));
  const secondEnd = state.doc.child(0).nodeSize + 1 + state.doc.child(1).content.size;
  state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, secondEnd)));
  state = applyCommand(state, splitTextblockWithFirstLineIndent);
  assert.equal(state.doc.childCount, 3);
  assert.equal(state.doc.child(2).attrs.firstLineIndent, 0, 'manual removal suppresses the next inherited auto-indent');
});

test('paste normalization strips structural indentation without eating ordinary short leading spaces', () => {
  const input = `\tAlpha\n\u00a0\u00a0Beta\n Gamma\n  Delta\n    Epsilon`;
  assert.equal(normalizePastedIndentText(input), 'Alpha\nBeta\n Gamma\n  Delta\n    Epsilon');

  const slice = new Slice(Fragment.from(paragraph('\u00a0\u00a0Pasted')), 0, 0);
  const normalized = normalizePastedIndentSlice(slice);
  assert.equal(normalized.content.firstChild.textContent, 'Pasted');
});


test('clipboard provenance preserves Rustnote layout only for trusted internal rich fragments', () => {
  const trustedClipboard = {getData: type => type === 'application/x-rustnote-editor-fragment' ? '1' : ''};
  const foreignClipboard = {getData: _type => ''};
  const forgedVersion = {getData: type => type === 'application/x-rustnote-editor-fragment' ? '999' : ''};
  assert.equal(editorClipboardDataIsTrusted(trustedClipboard), true);
  assert.equal(editorClipboardDataIsTrusted(foreignClipboard), false);
  assert.equal(editorClipboardDataIsTrusted(forgedVersion), false);

  const foreign = new Slice(Fragment.fromArray([
    paragraph('Alpha', {
      lineSpacing: '1.5',
      textAlignment: 'right',
      leftIndent: 36,
      rightIndent: 18,
      firstLineIndent: 18,
      dropCap: false,
      boundary: false,
    }),
    schema.nodes.heading.create({
      level: 2,
      lineSpacing: '2',
      textAlignment: 'justify',
      leftIndent: 18,
      rightIndent: 36,
      firstLineIndent: 0,
      dropCap: true,
    }, schema.text('Heading')),
    schema.nodes.code_block.create({language: 'rust', lineSpacing: '1.5'}, schema.text('let x = 1;')),
  ]), 0, 0);
  const sanitized = stripUntrustedEditorLayoutSlice(foreign);

  const paragraphNode = sanitized.content.child(0);
  assert.equal(paragraphNode.attrs.lineSpacing, null);
  assert.equal(paragraphNode.attrs.textAlignment, 'left');
  assert.equal(paragraphNode.attrs.leftIndent, 0);
  assert.equal(paragraphNode.attrs.rightIndent, 0);
  assert.equal(paragraphNode.attrs.firstLineIndent, 0);
  assert.equal(paragraphNode.attrs.dropCap, false);
  assert.equal(paragraphNode.textContent, 'Alpha');

  const headingNode = sanitized.content.child(1);
  assert.equal(headingNode.attrs.level, 2, 'semantic heading level must survive foreign-layout cleanup');
  assert.equal(headingNode.attrs.lineSpacing, null);
  assert.equal(headingNode.attrs.textAlignment, 'left');
  assert.equal(headingNode.attrs.leftIndent, 0);
  assert.equal(headingNode.attrs.rightIndent, 0);
  assert.equal(headingNode.attrs.firstLineIndent, 0);
  assert.equal(headingNode.attrs.dropCap, false);

  const codeNode = sanitized.content.child(2);
  assert.equal(codeNode.attrs.language, 'rust');
  assert.equal(codeNode.attrs.lineSpacing, null);
});


test('paste-as-plain-text consumes the next native paste as ProseMirror plain text', () => {
  let pasted = null;
  let focused = false;
  let prevented = false;
  const view = {
    editable: true,
    isDestroyed: false,
    pasteText(text) { pasted = text; return true; },
    focus() { focused = true; },
  };
  const plugin = plainTextPastePlugin();
  const event = {
    clipboardData: {getData: type => type === 'text/plain' ? 'Plain\ntext' : ''},
    preventDefault() { prevented = true; },
  };

  assert.equal(requestPlainTextPaste(null, null, view), false, 'shortcut must leave native paste delivery enabled');
  assert.equal(plugin.props.handlePaste(view, event), true);
  assert.equal(pasted, 'Plain\ntext');
  assert.equal(focused, true);
  assert.equal(prevented, true);

  pasted = null;
  prevented = false;
  assert.equal(plugin.props.handlePaste(view, event), false, 'normal paste is unaffected once the one-shot request is consumed');
  assert.equal(pasted, null);
  assert.equal(prevented, false);
});

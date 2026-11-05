import {schema} from "../document/schema.js";
import {Fragment} from "prosemirror-model";
import {splitBlock} from "prosemirror-commands";
import {InputRule} from "prosemirror-inputrules";
import {keymap} from "prosemirror-keymap";
import {TextSelection} from "prosemirror-state";

function textblockPositionAtSelection(state) {
  const {$from} = state.selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    const node = $from.node(depth);
    if (node.isTextblock) return {node, position: $from.before(depth)};
  }
  return null;
}

function positionHasListAncestor(state, position) {
  const probe = Math.max(0, Math.min(state.doc.content.size, position + 1));
  const resolved = state.doc.resolve(probe);
  for (let depth = resolved.depth; depth > 0; depth -= 1) {
    const node = resolved.node(depth);
    if (node.type === schema.nodes.list_item || node.type === schema.nodes.task_item) return true;
  }
  return false;
}

const EM_DASH_SEQUENCE_PREFIX = '— ';

function lineStartEmDashInputRule() {
  return new InputRule(/^\-\s$/, (state, _match, start, end) => (
    state.tr.insertText(EM_DASH_SEQUENCE_PREFIX, start, end)
  ));
}

function continueEmDashSequence(state, dispatch) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) || !selection.empty) return false;
  const paragraph = selection.$from.parent;
  if (paragraph.type !== schema.nodes.paragraph
      || !paragraph.textContent.startsWith(EM_DASH_SEQUENCE_PREFIX)
      || selection.$from.parentOffset < EM_DASH_SEQUENCE_PREFIX.length) {
    return false;
  }
  if (!dispatch) return true;
  let transaction = null;
  const applied = splitBlock(state, next => { transaction = next; });
  if (!applied || transaction === null) return false;
  const insertAt = transaction.selection.from;
  dispatch(transaction.insertText(EM_DASH_SEQUENCE_PREFIX, insertAt).scrollIntoView());
  return true;
}

function exitEmptyEmDashSequenceOnBackspace(state, dispatch) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) || !selection.empty) return false;
  const paragraph = selection.$from.parent;
  if (paragraph.type !== schema.nodes.paragraph
      || paragraph.textContent !== EM_DASH_SEQUENCE_PREFIX
      || selection.$from.parentOffset !== EM_DASH_SEQUENCE_PREFIX.length) {
    return false;
  }
  if (!dispatch) return true;
  dispatch(state.tr.delete(
    selection.from - EM_DASH_SEQUENCE_PREFIX.length,
    selection.from,
  ).scrollIntoView());
  return true;
}

function emDashSequenceKeymap() {
  return keymap({
    Enter: continueEmDashSequence,
    Backspace: exitEmptyEmDashSequenceOnBackspace,
  });
}

function createIntentionalBlankLineFromAtomicBoundary(state, dispatch) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) || !selection.empty) return false;
  const paragraph = selection.$from.parent;
  if (selection.$from.depth !== 1
      || paragraph.type !== schema.nodes.paragraph
      || paragraph.attrs.boundary !== true
      || paragraph.content.size !== 0) {
    return false;
  }
  if (!dispatch) return true;

  // A boundary paragraph is only an automatic caret landing pad. Enter makes
  // the current empty line intentional and creates the next ordinary line for
  // the caret, so boundary cleanup must no longer collapse the user's blank.
  const paragraphPosition = selection.$from.before(1);
  const blank = schema.nodes.paragraph.create();
  const continuation = schema.nodes.paragraph.create();
  let transaction = state.tr.replaceWith(
    paragraphPosition,
    paragraphPosition + paragraph.nodeSize,
    Fragment.fromArray([blank, continuation]),
  );
  transaction = transaction.setSelection(TextSelection.create(
    transaction.doc,
    paragraphPosition + blank.nodeSize + 1,
  ));
  dispatch(transaction.scrollIntoView());
  return true;
}

function atomicBoundaryKeymap() {
  return keymap({
    Enter: createIntentionalBlankLineFromAtomicBoundary,
  });
}

function firstBodyTextblockPosition(doc) {
  let position = null;
  doc.descendants((node, nodePosition) => {
    if (position !== null) return false;
    if (!node.isTextblock) return true;
    position = nodePosition;
    return false;
  });
  return position;
}

function selectionAtFirstBodyTextblockStart(state) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) || !selection.empty || selection.$from.parentOffset !== 0) return false;
  const current = textblockPositionAtSelection(state);
  return Boolean(current && current.position === firstBodyTextblockPosition(state.doc));
}

function selectionOnFirstBodyVisualLine(state, view = null) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) || !selection.empty) return false;
  const current = textblockPositionAtSelection(state);
  if (!current || current.position !== firstBodyTextblockPosition(state.doc)) return false;
  if (selection.$from.parentOffset === 0) return true;
  if (!view || typeof view.coordsAtPos !== 'function') return false;
  try {
    const caret = view.coordsAtPos(selection.from);
    const first = view.coordsAtPos(current.position + 1);
    return Math.abs(caret.top - first.top) <= 2;
  } catch {
    return false;
  }
}

function focusNoteTitleFromBody(state, _dispatch = null, view = null) {
  if (!selectionOnFirstBodyVisualLine(state, view) || typeof document === 'undefined') return false;
  const title = document.getElementById('note-title-input');
  if (!title || title.disabled || typeof title.focus !== 'function') return false;
  title.focus({preventScroll: true});
  if (typeof title.setSelectionRange === 'function') {
    const end = String(title.value || '').length;
    title.setSelectionRange(end, end);
  }
  return true;
}

function backspaceToNoteTitle(state, _dispatch = null, view = null) {
  const current = textblockPositionAtSelection(state);
  if (!current || positionHasListAncestor(state, current.position) || !selectionAtFirstBodyTextblockStart(state)) return false;
  return focusNoteTitleFromBody(state, null, view);
}

function editorBoundaryKeymap() {
  return keymap({
    ArrowUp: focusNoteTitleFromBody,
    Backspace: backspaceToNoteTitle,
  });
}

export {
  atomicBoundaryKeymap,
  backspaceToNoteTitle,
  continueEmDashSequence,
  createIntentionalBlankLineFromAtomicBoundary,
  editorBoundaryKeymap,
  emDashSequenceKeymap,
  exitEmptyEmDashSequenceOnBackspace,
  firstBodyTextblockPosition,
  focusNoteTitleFromBody,
  lineStartEmDashInputRule,
};

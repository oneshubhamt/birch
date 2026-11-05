import {ORDERED_LIST_STYLES} from "../config.js";
import {bulletListAttrs, schema} from "../document/schema.js";
import {autoJoin, chainCommands, joinTextblockBackward} from "prosemirror-commands";
import {keymap} from "prosemirror-keymap";
import {liftListItem, sinkListItem, splitListItem, wrapInList} from "prosemirror-schema-list";
import {TextSelection} from "prosemirror-state";

const LIST_NODE_TYPES = new Set(['bullet_list', 'ordered_list', 'task_list']);
const LIST_ITEM_TYPES = new Set(['list_item', 'task_item']);

function listContextAtResolved($pos) {
  for (let depth = $pos.depth; depth > 0; depth -= 1) {
    const node = $pos.node(depth);
    if (!LIST_NODE_TYPES.has(node.type.name)) continue;
    return {
      depth,
      node,
      pos: $pos.before(depth),
      parent: $pos.node(depth - 1),
      parentDepth: depth - 1,
      index: $pos.index(depth - 1),
    };
  }
  return null;
}

function commonSelectedListContext(state) {
  const {$from, $to} = state.selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    const node = $from.node(depth);
    if (!LIST_NODE_TYPES.has(node.type.name)) continue;
    const position = $from.before(depth);
    for (let otherDepth = $to.depth; otherDepth > 0; otherDepth -= 1) {
      if ($to.node(otherDepth).type === node.type && $to.before(otherDepth) === position) {
        return {
          depth,
          node,
          pos: position,
          parent: $from.node(depth - 1),
          parentDepth: depth - 1,
          index: $from.index(depth - 1),
        };
      }
    }
  }
  return null;
}

function selectedListContexts(state) {
  const {selection} = state;
  if (selection.empty) {
    const context = listContextAtResolved(selection.$from);
    return context ? [context] : [];
  }

  const common = commonSelectedListContext(state);
  if (common) return [common];

  const contexts = new Map();
  state.doc.nodesBetween(selection.from, selection.to, (node, position) => {
    if (!LIST_ITEM_TYPES.has(node.type.name)) return true;
    const probe = Math.min(position + 1, state.doc.content.size);
    const context = listContextAtResolved(state.doc.resolve(probe));
    if (context) contexts.set(context.pos, context);
    return false;
  });

  if (contexts.size === 0) {
    const context = listContextAtResolved(selection.$from);
    return context ? [context] : [];
  }
  return [...contexts.values()].sort((left, right) => left.pos - right.pos);
}

function selectedListType(state) {
  const contexts = selectedListContexts(state);
  if (contexts.length === 0) return null;
  const types = new Set(contexts.map(context => context.node.type.name));
  return types.size === 1 ? contexts[0].node.type.name : 'mixed';
}

function orderedListAttrs(order, style = schema.nodes.ordered_list.defaultAttrs.style, restart = false) {
  return {
    order: Math.max(1, Number(order) || 1),
    style: ORDERED_LIST_STYLES.includes(style) ? style : schema.nodes.ordered_list.defaultAttrs.style,
    restart: Boolean(restart),
  };
}

function adjacentOrderedContext(context, direction) {
  const index = context.index + direction;
  if (index < 0 || index >= context.parent.childCount) return null;
  const node = context.parent.child(index);
  return node.type === schema.nodes.ordered_list ? node : null;
}

const splitNativeListItem = chainCommands(
  splitListItem(schema.nodes.list_item),
  splitListItem(schema.nodes.task_item),
);

const liftNativeListItem = chainCommands(
  liftListItem(schema.nodes.list_item),
  liftListItem(schema.nodes.task_item),
);

const sinkNativeListItem = chainCommands(
  sinkListItem(schema.nodes.list_item),
  sinkListItem(schema.nodes.task_item),
);

function selectedListItemTextblockPositions(state) {
  const contexts = new Map();

  function addAncestor($pos) {
    for (let depth = $pos.depth; depth > 0; depth -= 1) {
      const node = $pos.node(depth);
      if (!LIST_ITEM_TYPES.has(node.type.name)) continue;
      const pos = $pos.before(depth);
      contexts.set(pos, {node, pos});
      break;
    }
  }

  addAncestor(state.selection.$from);
  addAncestor(state.selection.$to);
  if (!state.selection.empty) {
    state.doc.nodesBetween(state.selection.from, state.selection.to, (node, pos) => {
      if (!LIST_ITEM_TYPES.has(node.type.name)) return true;
      contexts.set(pos, {node, pos});
      return false;
    });
  }

  const positions = new Set();
  for (const {node, pos} of contexts.values()) {
    node.descendants((child, offset) => {
      if (child.isTextblock) positions.add(pos + 1 + offset);
      return true;
    });
  }
  return [...positions].sort((left, right) => left - right);
}

function orderedOuterLiftMetadata(state) {
  const context = listContextAtResolved(state.selection.$from);
  if (!context || context.node.type !== schema.nodes.ordered_list) return null;

  const {$from, $to} = state.selection;
  const range = $from.blockRange(
    $to,
    node => node.childCount > 0 && node.firstChild?.type === schema.nodes.list_item,
  );
  if (!range || range.parent !== context.node) return null;
  if ($from.node(range.depth - 1).type === schema.nodes.list_item) return null;

  return {
    containerDepth: range.depth - 1,
    hasTrailingFragment: range.endIndex < range.parent.childCount,
    trailingOrder: Number(context.node.attrs.order) + range.endIndex,
    style: context.node.attrs.style || 'auto',
  };
}

function normalizeOrderedTrailingFragment(transaction, metadata, originalPositions) {
  if (!metadata?.hasTrailingFragment || originalPositions.length === 0) return transaction;

  let anchor = null;
  let lastLiftedIndex = -1;
  for (const original of originalPositions) {
    const mapped = transaction.mapping.map(original, 1);
    const probe = Math.min(mapped + 1, transaction.doc.content.size);
    const $mapped = transaction.doc.resolve(probe);
    if ($mapped.depth <= metadata.containerDepth) continue;
    const index = $mapped.index(metadata.containerDepth);
    if (index > lastLiftedIndex) {
      lastLiftedIndex = index;
      anchor = $mapped;
    }
  }
  if (!anchor || lastLiftedIndex < 0) return transaction;

  const parent = anchor.node(metadata.containerDepth);
  const trailingIndex = lastLiftedIndex + 1;
  if (trailingIndex >= parent.childCount) return transaction;
  const trailing = parent.child(trailingIndex);
  if (trailing.type !== schema.nodes.ordered_list) return transaction;

  let position = anchor.start(metadata.containerDepth);
  for (let index = 0; index < trailingIndex; index += 1) {
    position += parent.child(index).nodeSize;
  }
  return transaction.setNodeMarkup(
    position,
    undefined,
    orderedListAttrs(metadata.trailingOrder, metadata.style, false),
    trailing.marks,
  );
}

function normalizeExitedListParagraphIndents(transaction, originalPositions) {
  const mappedPositions = [...new Set(
    originalPositions.map(position => transaction.mapping.map(position, 1)),
  )];
  let next = transaction;
  for (const mapped of mappedPositions) {
    const node = next.doc.nodeAt(mapped);
    if (!node?.isTextblock) continue;

    const probe = Math.min(mapped + 1, next.doc.content.size);
    if (listContextAtResolved(next.doc.resolve(probe))) continue;

    const attrs = node.attrs || {};
    if (!Object.prototype.hasOwnProperty.call(attrs, 'leftIndent')
        || (Number(attrs.leftIndent) === 0
          && Number(attrs.rightIndent) === 0
          && Number(attrs.firstLineIndent) === 0)) {
      continue;
    }
    next = next.setNodeMarkup(mapped, undefined, {
      ...attrs,
      leftIndent: 0,
      rightIndent: 0,
      firstLineIndent: 0,
    }, node.marks);
  }
  return next;
}

function liftSelectedListItems(state, dispatch, view) {
  if (!dispatch) return liftNativeListItem(state, undefined, view);

  const textblockPositions = selectedListItemTextblockPositions(state);
  const orderedMetadata = orderedOuterLiftMetadata(state);
  let transaction = null;
  const lifted = liftNativeListItem(state, next => { transaction = next; }, view);
  if (!lifted || transaction === null) return false;

  transaction = normalizeOrderedTrailingFragment(transaction, orderedMetadata, textblockPositions);
  transaction = normalizeExitedListParagraphIndents(transaction, textblockPositions);
  dispatch(transaction.scrollIntoView());
  return true;
}

function sinkSelectedListItems(state, dispatch, view) {
  return sinkNativeListItem(state, dispatch, view);
}

function backspaceListItemAtStart(state, dispatch, view) {
  const {selection} = state;
  if (!(selection instanceof TextSelection) || !selection.empty || selection.$from.parentOffset !== 0) {
    return false;
  }

  const {$from} = selection;
  for (let depth = $from.depth - 1; depth > 0; depth -= 1) {
    const node = $from.node(depth);
    if (!LIST_ITEM_TYPES.has(node.type.name)) continue;
    if ($from.index(depth) !== 0) return false;
    return liftSelectedListItems(state, dispatch, view);
  }
  return false;
}

function backspaceParagraphAfterList(state, dispatch, view) {
  const {selection} = state;
  if (!(selection instanceof TextSelection)
      || !selection.empty
      || selection.$from.parentOffset !== 0
      || selection.$from.parent.type !== schema.nodes.paragraph
      || selection.$from.parent.attrs.boundary === true
      || listContextAtResolved(selection.$from)) {
    return false;
  }

  const range = selection.$from.blockRange(selection.$to);
  if (!range || range.startIndex === 0) return false;
  const previous = range.parent.child(range.startIndex - 1);
  if (!LIST_NODE_TYPES.has(previous.type.name)) return false;

  // Product invariant: Backspace may remove list membership or merge text, but
  // it must never re-parent an ordinary paragraph back into a list as a new
  // list-owned block. The narrow native command joins only the textblocks.
  const command = autoJoin(joinTextblockBackward, listsCanAutoJoin);
  return command(state, dispatch, view);
}

const backspaceListPolicy = chainCommands(
  backspaceListItemAtStart,
  backspaceParagraphAfterList,
);

function listKeymap() {
  return keymap({
    Enter: splitNativeListItem,
    Backspace: backspaceListPolicy,
    Tab: sinkNativeListItem,
    'Shift-Tab': liftSelectedListItems,
  });
}

function convertListItemNode(item, targetItemType) {
  if (item.type === targetItemType) return item;
  const attrs = targetItemType === schema.nodes.task_item ? {checked: false} : null;
  return targetItemType.create(attrs, item.content, item.marks);
}

function convertedOrderedListAttrs(context) {
  const previous = adjacentOrderedContext(context, -1);
  if (previous) {
    return orderedListAttrs(
      Number(previous.attrs.order) + previous.childCount,
      previous.attrs.style || 'auto',
    );
  }
  const next = adjacentOrderedContext(context, 1);
  if (next && !next.attrs.restart) {
    return orderedListAttrs(
      Math.max(1, Number(next.attrs.order) - context.node.childCount),
      next.attrs.style || 'auto',
    );
  }
  return orderedListAttrs(1, 'auto');
}

function restoreListSelection(transaction, listPosition, anchorOffset, headOffset) {
  const anchor = listPosition + anchorOffset;
  const head = listPosition + headOffset;
  try {
    return transaction.setSelection(TextSelection.create(transaction.doc, anchor, head));
  } catch {
    return transaction.setSelection(TextSelection.near(transaction.doc.resolve(head), 1));
  }
}

function convertContainingList(state, dispatch, context, targetListType, targetItemType) {
  if (!dispatch) return true;
  const attrs = targetListType === schema.nodes.ordered_list
    ? convertedOrderedListAttrs(context)
    : targetListType === schema.nodes.bullet_list
      ? bulletListAttrs('disc')
      : null;
  const items = [];
  context.node.forEach(item => items.push(convertListItemNode(item, targetItemType)));
  const converted = targetListType.create(attrs, items, context.node.marks);
  const anchorOffset = state.selection.anchor - context.pos;
  const headOffset = state.selection.head - context.pos;
  let transaction = state.tr.replaceWith(
    context.pos,
    context.pos + context.node.nodeSize,
    converted,
  );
  transaction = restoreListSelection(transaction, context.pos, anchorOffset, headOffset);
  dispatch(transaction.scrollIntoView());
  return true;
}

function selectedBlockRange(state) {
  return state.selection.$from.blockRange(state.selection.$to);
}

function adjacentTargetLists(state, targetListType) {
  const range = selectedBlockRange(state);
  if (!range) return {range: null, previous: null, next: null};
  const previous = range.startIndex > 0 ? range.parent.child(range.startIndex - 1) : null;
  const next = range.endIndex < range.parent.childCount ? range.parent.child(range.endIndex) : null;
  return {
    range,
    previous: previous?.type === targetListType ? previous : null,
    next: next?.type === targetListType ? next : null,
  };
}

function listsCanAutoJoin(left, right) {
  if (!left || !right || left.type !== right.type) return false;
  if (left.type === schema.nodes.bullet_list) {
    return left.attrs.marker === right.attrs.marker;
  }
  if (left.type === schema.nodes.task_list) return true;
  if (left.type !== schema.nodes.ordered_list) return false;
  return (left.attrs.style || 'auto') === (right.attrs.style || 'auto')
    && right.attrs.restart !== true;
}

function wrappingListAttrs(state, targetListType) {
  const {range, previous, next} = adjacentTargetLists(state, targetListType);
  if (targetListType === schema.nodes.bullet_list) {
    return bulletListAttrs(previous?.attrs.marker || next?.attrs.marker || 'disc');
  }
  if (targetListType !== schema.nodes.ordered_list) return null;

  if (previous) {
    return orderedListAttrs(
      Number(previous.attrs.order) + previous.childCount,
      previous.attrs.style || 'auto',
      false,
    );
  }
  if (next && next.attrs.restart !== true) {
    const wrappedItemCount = Math.max(1, (range?.endIndex || 0) - (range?.startIndex || 0));
    return orderedListAttrs(
      Math.max(1, Number(next.attrs.order) - wrappedItemCount),
      next.attrs.style || 'auto',
      false,
    );
  }
  return orderedListAttrs(1, 'auto', false);
}

function wrapInListJoiningAdjacent(state, dispatch, targetListType) {
  const attrs = wrappingListAttrs(state, targetListType);
  const command = autoJoin(wrapInList(targetListType, attrs), listsCanAutoJoin);
  return command(state, dispatch);
}

function toggleList(state, dispatch, targetListType, targetItemType) {
  const context = commonSelectedListContext(state);
  if (!context) return wrapInListJoiningAdjacent(state, dispatch, targetListType);
  if (context.node.type === targetListType) return liftSelectedListItems(state, dispatch);
  return convertContainingList(state, dispatch, context, targetListType, targetItemType);
}

export {
  adjacentOrderedContext,
  commonSelectedListContext,
  liftSelectedListItems,
  listContextAtResolved,
  listKeymap,
  orderedListAttrs,
  selectedListContexts,
  selectedListType,
  sinkSelectedListItems,
  toggleList,
};

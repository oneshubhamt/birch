import {Fragment, Slice} from "prosemirror-model";
import {Plugin} from "prosemirror-state";
import {schema} from "../document/schema.js";

const EDITOR_FRAGMENT_CLIPBOARD_MIME = 'application/x-rustnote-editor-fragment';
const EDITOR_FRAGMENT_CLIPBOARD_VERSION = '1';
const PLAIN_TEXT_PASTE_REQUEST_WINDOW_MS = 1500;

let plainTextPasteRequestedUntil = 0;

function editorClipboardDataIsTrusted(clipboardData) {
  if (!clipboardData?.getData) return false;
  try {
    return clipboardData.getData(EDITOR_FRAGMENT_CLIPBOARD_MIME) === EDITOR_FRAGMENT_CLIPBOARD_VERSION;
  } catch {
    return false;
  }
}

function neutralizeUntrustedTextblockLayout(node) {
  if (!node || node.isText) return node;
  let contentChanged = false;
  const children = [];
  node.forEach(child => {
    const normalized = neutralizeUntrustedTextblockLayout(child);
    children.push(normalized);
    contentChanged = contentChanged || normalized !== child;
  });
  const content = contentChanged ? Fragment.fromArray(children) : node.content;
  let attrs = node.attrs;
  let attrsChanged = false;
  const nextAttrs = {...node.attrs};
  const reset = (key, value) => {
    if (nextAttrs[key] !== value) {
      nextAttrs[key] = value;
      attrsChanged = true;
    }
  };
  if (node.type === schema.nodes.paragraph || node.type === schema.nodes.heading) {
    reset('lineSpacing', null);
    reset('textAlignment', 'left');
    reset('leftIndent', 0);
    reset('rightIndent', 0);
    reset('firstLineIndent', 0);
    reset('dropCap', false);
    if (node.type === schema.nodes.paragraph) reset('boundary', false);
  } else if (node.type === schema.nodes.code_block) {
    reset('lineSpacing', null);
  }
  if (attrsChanged) attrs = nextAttrs;
  if (!attrsChanged && !contentChanged) return node;
  return node.type.create(attrs, content, node.marks);
}

function stripUntrustedEditorLayoutSlice(slice) {
  const children = [];
  let changed = false;
  slice.content.forEach(child => {
    const normalized = neutralizeUntrustedTextblockLayout(child);
    children.push(normalized);
    changed = changed || normalized !== child;
  });
  return changed ? new Slice(Fragment.fromArray(children), slice.openStart, slice.openEnd) : slice;
}

function writeEditorClipboard(view, event) {
  const data = event.clipboardData;
  const selection = view.state.selection;
  if (!data || selection.empty || (event.type === 'cut' && view.editable === false)) return false;
  try {
    const {dom, text} = view.serializeForClipboard(selection.content());
    data.clearData();
    data.setData(EDITOR_FRAGMENT_CLIPBOARD_MIME, EDITOR_FRAGMENT_CLIPBOARD_VERSION);
    data.setData('text/html', dom.innerHTML);
    data.setData('text/plain', text);
  } catch {
    return false;
  }
  event.preventDefault();
  if (event.type === 'cut') {
    view.dispatch(view.state.tr.deleteSelection().scrollIntoView().setMeta('uiEvent', 'cut'));
  }
  return true;
}

function requestPlainTextPaste(_state, _dispatch, view) {
  if (!view || view.editable === false || view.isDestroyed) return false;
  plainTextPasteRequestedUntil = Date.now() + PLAIN_TEXT_PASTE_REQUEST_WINDOW_MS;
  // Returning false deliberately leaves the native paste shortcut unhandled. The
  // ensuing ClipboardEvent carries the system clipboard payload without requiring
  // navigator.clipboard permissions.
  return false;
}

function consumePlainTextPasteRequest() {
  const requested = Date.now() <= plainTextPasteRequestedUntil;
  plainTextPasteRequestedUntil = 0;
  return requested;
}

function plainTextPastePlugin() {
  return new Plugin({
    props: {
      handlePaste(view, event) {
        if (!event || !consumePlainTextPasteRequest()) return false;
        let text = '';
        try {
          text = event.clipboardData?.getData('text/plain') || '';
        } catch {
          return false;
        }
        event.preventDefault();
        // Omit the ClipboardEvent intentionally. Other rich/smart-paste handlers
        // treat an event-less paste as plain content and yield to ProseMirror.
        view.pasteText(text);
        view.focus();
        return true;
      },
    },
  });
}

function editorClipboardProvenancePlugin() {
  return new Plugin({
    props: {
      handleDOMEvents: {
        copy: writeEditorClipboard,
        cut: writeEditorClipboard,
      },
      handlePaste(view, event, slice) {
        if (!event) return false;
        if (editorClipboardDataIsTrusted(event.clipboardData)) return false;
        const sanitized = stripUntrustedEditorLayoutSlice(slice);
        if (sanitized === slice) return false;
        event.preventDefault();
        const singleNode = sanitized.openStart === 0
          && sanitized.openEnd === 0
          && sanitized.content.childCount === 1
          ? sanitized.content.firstChild
          : null;
        const transaction = singleNode
          ? view.state.tr.replaceSelectionWith(singleNode, false)
          : view.state.tr.replaceSelection(sanitized);
        view.dispatch(transaction.scrollIntoView().setMeta('paste', true).setMeta('uiEvent', 'paste'));
        return true;
      },
    },
  });
}

export {
  editorClipboardDataIsTrusted,
  editorClipboardProvenancePlugin,
  plainTextPastePlugin,
  requestPlainTextPaste,
  stripUntrustedEditorLayoutSlice,
};

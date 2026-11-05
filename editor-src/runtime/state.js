


function runtimeErrorMessage(error) {
  if (error instanceof Error) {
    const detail = error.message || String(error);
    const stack = typeof error.stack === 'string' ? error.stack : '';
    return stack && !stack.startsWith(detail) ? `${detail}\n${stack}` : stack || detail;
  }
  return String(error || 'Unknown editor error');
}

function setEditorRuntimeError(error) {
  editorLastError = runtimeErrorMessage(error);
  return editorLastError;
}

let activeEditorController = null;
let editorLastError = '';

function setActiveEditorController(controller) {
  activeEditorController = controller;
}

function clearEditorRuntimeError() {
  editorLastError = '';
}
export {activeEditorController, clearEditorRuntimeError, editorLastError, setActiveEditorController, setEditorRuntimeError};

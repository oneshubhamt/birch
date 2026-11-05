import {EDITOR_BRIDGE_PROTOCOL_VERSION, EVENT_BRIDGE_ID, ROOT_ID, RUNTIME_MARKER} from "../config.js";
import {Controller} from "../runtime/controller.js";
import {normalizeAtomicBoundaryDocument} from "../editor/core.js";
import {parseEnvelope} from "../document/schema.js";
import {activeEditorController, clearEditorRuntimeError, editorLastError, setActiveEditorController, setEditorRuntimeError} from "../runtime/state.js";



function installEditor(config) {
  const root = document.getElementById(ROOT_ID);
  const eventBridge = document.getElementById(EVENT_BRIDGE_ID);
  if (!(root instanceof HTMLElement) || !(eventBridge instanceof HTMLTextAreaElement)) {
    setEditorRuntimeError('The editor host or event bridge is not mounted.');
    return false;
  }
  try {
    const previousController = activeEditorController;
    previousController?.destroy(false);
    if (activeEditorController === previousController) setActiveEditorController(null);
    const doc = normalizeAtomicBoundaryDocument(parseEnvelope(config.initial_editor_state));
    root.replaceChildren();
    root.dataset.runtimeError = 'false';
    root.removeAttribute('data-runtime-error-message');
    const controller = new Controller(
      root,
      doc,
      config.note_id,
      config.generation,
      config.editable !== false,
      config.image_import_stage_url || '',
      config.attachments || [],
      config.note_link_targets || [],
      config.attachment_import_intents || null,
      config.auto_indent_enabled === true,
      config.auto_first_line_indent,
      config.default_line_spacing,
      config.spellcheck_enabled !== false,
    );
    setActiveEditorController(controller);
    clearEditorRuntimeError();
    controller.requestPendingAttachmentRecovery();
    return true;
  } catch (error) {
    const message = setEditorRuntimeError(error);
    root.dataset.runtimeError = 'true';
    root.dataset.runtimeReady = 'false';
    root.dataset.runtimeErrorMessage = message.slice(0, 500);
    console.error('Rustnote ProseMirror editor error', error);
    return false;
  }
}

function validateEditorState(serialized) {
  try {
    normalizeAtomicBoundaryDocument(parseEnvelope(serialized));
    clearEditorRuntimeError();
    return true;
  } catch (error) {
    setEditorRuntimeError(error);
    return false;
  }
}

function disposeEditor(flush = false) {
  const controller = activeEditorController;
  if (!controller) return true;
  controller.destroy(Boolean(flush));
  if (activeEditorController === controller) setActiveEditorController(null);
  return true;
}

function editorCommandPayload(envelope) {
  if (!envelope || envelope.version !== EDITOR_BRIDGE_PROTOCOL_VERSION) return null;
  if (typeof envelope.command !== 'string') return null;
  const payload = envelope.payload;
  if (payload == null) return {};
  return typeof payload === 'object' && !Array.isArray(payload) ? payload : null;
}

function dispatchEditorCommand(envelope) {
  const payload = editorCommandPayload(envelope);
  if (payload === null) return false;
  const controller = activeEditorController;
  switch (envelope.command) {
    case 'install':
      return installEditor(payload);
    case 'read_state':
      return controller?.flush(Boolean(payload.dispatch_input)) || '';
    case 'set_editable':
      return controller?.setEditable(Boolean(payload.editable)) || false;
    case 'set_attachment_registry':
      return controller?.setAttachmentRegistry(payload.attachments) || false;
    case 'remove_attachment_from_document':
      return controller?.removeAttachmentFromDocument(payload.attachment_id) || false;
    case 'recover_attachment_imports': {
      if (!controller) return false;
      controller.setAttachmentRegistry(payload.attachments);
      return controller.recoverAttachmentPickerImports(payload.recovery) || false;
    }
    case 'set_note_link_registry':
      return controller?.setNoteLinkRegistry(payload.targets) || false;
    case 'media_compatibility_result':
      return controller?.completeMediaCompatibility(
        payload.attachment_id,
        payload.profile,
        payload.status,
        payload.message || '',
      ) || false;
    case 'complete_note_link_picker':
      return controller?.completeNoteLinkPicker(payload.request_id, payload.descriptor) || false;
    case 'fail_note_link_picker':
      return controller?.failNoteLinkPicker(payload.request_id) || false;
    case 'begin_attachment_import':
      return controller?.beginAttachmentPickerImport(payload.request_id, payload.items) || false;
    case 'update_attachment_import':
      return controller?.updateAttachmentPickerImport(payload.request_id, payload.items) || false;
    case 'complete_attachment_picker':
      return controller?.completeAttachmentPicker(payload.request_id, payload.attachments) || 0;
    case 'fail_attachment_picker':
      return controller?.failAttachmentPicker(payload.request_id) || false;
    case 'complete_pdf_export':
      return controller?.completePdfExport(payload.request_id, Boolean(payload.success), payload.message || '') || false;
    case 'complete_image_import':
      return controller?.completeImageImport(payload.request_id, payload.attachments) || 0;
    case 'fail_image_import':
      return controller?.failImageImport(payload.request_id) || false;
    case 'complete_image_edit':
      return controller?.completeImageEdit(payload.request_id, payload.descriptor) || false;
    case 'fail_image_edit':
      return controller?.failImageEdit(payload.request_id, payload.message || '') || false;
    case 'dispose':
      return disposeEditor(Boolean(payload.flush));
    case 'validate_state':
      return validateEditorState(payload.editor_state);
    case 'replace_state':
      return controller?.replaceEditorState(payload.editor_state, payload.generation) || '';
    case 'format':
      return controller?.format(payload.action) || false;
    case 'set_typography':
      return controller?.setTypography(payload.kind, payload.value) || false;
    case 'set_editor_preferences':
      return controller?.setEditorPreferences(
        Boolean(payload.auto_indent_enabled),
        payload.auto_first_line_indent,
        payload.default_line_spacing,
        Boolean(payload.spellcheck_enabled),
      ) || false;
    case 'apply_font_size_input': {
      const input = document.getElementById('editor-font-size-input');
      return Boolean(input && controller && controller.setTypography('font_size', input.value));
    }
    case 'adjust_font_size':
      return controller?.adjustFontSize(payload.delta) || false;
    case 'refresh_surface':
      if (!controller) return false;
      controller.refreshSurface();
      return true;
    case 'focus_editor':
      return controller?.focusEditor() || false;
    case 'open_insert_menu':
      return controller?.openInsertMenu() || false;
    case 'find':
      return controller?.find(payload.query, payload.requested_index) || 0;
    case 'toggle_editor_focus':
      return controller?.toggleEditorFocus() || false;
    case 'exit_focus':
      return controller?.exitFocusMode() || false;
    case 'toggle_window_fullscreen':
      return controller?.requestWindowFullscreenToggle() || false;
    case 'capture_link_selection':
      return controller?.captureLinkSelection() || '';
    case 'insert_link':
      return controller?.insertLink(payload.url, payload.label) || false;
    default:
      return false;
  }
}

window.__rustnoteEditorBridge = Object.freeze({
  version: EDITOR_BRIDGE_PROTOCOL_VERSION,
  marker: RUNTIME_MARKER,
  dispatch: envelope => dispatchEditorCommand(envelope),
  error: () => editorLastError,
  isReady: generation => {
    const root = document.getElementById(ROOT_ID);
    return Boolean(
      root?.dataset.runtimeReady === 'true'
      && activeEditorController?.generation === Number(generation),
    );
  },
});

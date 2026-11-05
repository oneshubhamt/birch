import {EDITOR_BRIDGE_PROTOCOL_VERSION, EVENT_BRIDGE_ID} from "../config.js";



function emitEditorEvent(event, payload) {
  const bridge = document.getElementById(EVENT_BRIDGE_ID);
  if (!(bridge instanceof HTMLTextAreaElement)) return false;
  bridge.value = JSON.stringify({version: EDITOR_BRIDGE_PROTOCOL_VERSION, event, payload});
  bridge.dispatchEvent(new Event('input', {bubbles: true}));
  return true;
}

function emitAttachmentCommand(command) {
  return emitEditorEvent('attachment_command', command);
}

function emitNoteLinkCommand(command) {
  return emitEditorEvent('note_link_command', command);
}

function attachmentStatus(message) {
  emitAttachmentCommand({kind: 'status', message: String(message || '').slice(0, 400)});
}
export {attachmentStatus, emitAttachmentCommand, emitEditorEvent, emitNoteLinkCommand};

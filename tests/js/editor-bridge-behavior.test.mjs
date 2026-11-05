import assert from 'node:assert/strict';
import test from 'node:test';

globalThis.__RUSTNOTE_RUNTIME_MARKER__ = 'b4-bridge-test';

class FakeElement {}
class FakeTextArea extends FakeElement {
  constructor() {
    super();
    this.value = '';
    this.events = [];
  }
  dispatchEvent(event) {
    this.events.push(event);
    return true;
  }
}
class FakeEvent {
  constructor(type, options = {}) {
    this.type = type;
    this.bubbles = Boolean(options.bubbles);
  }
}

globalThis.HTMLElement = FakeElement;
globalThis.HTMLTextAreaElement = FakeTextArea;
globalThis.Event = FakeEvent;
const bridge = new FakeTextArea();
globalThis.document = {
  getElementById(id) {
    return id === 'note-editor-event-bridge' ? bridge : null;
  },
};

const {attachmentStatus, emitAttachmentCommand, emitEditorEvent} = await import('../../editor-src/bridge/events.js');

test('editor events use the versioned bridge envelope and bubble input', () => {
  assert.equal(emitEditorEvent('document_changed', {revision: 7}), true);
  assert.deepEqual(JSON.parse(bridge.value), {
    version: 1,
    event: 'document_changed',
    payload: {revision: 7},
  });
  assert.equal(bridge.events.at(-1).type, 'input');
  assert.equal(bridge.events.at(-1).bubbles, true);
});

test('attachment status is bounded before crossing the bridge', () => {
  attachmentStatus('x'.repeat(900));
  const envelope = JSON.parse(bridge.value);
  assert.equal(envelope.event, 'attachment_command');
  assert.equal(envelope.payload.kind, 'status');
  assert.equal(envelope.payload.message.length, 400);
  assert.equal(emitAttachmentCommand({kind: 'reveal', attachment_id: 'att_b4'}), true);
});

test('missing event bridge fails closed', () => {
  globalThis.document = {getElementById() { return null; }};
  assert.equal(emitEditorEvent('document_changed', {}), false);
});

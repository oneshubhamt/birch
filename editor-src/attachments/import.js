import {nodeAttrDefault, specDomain, specLimit, truncateUtf8} from "../document/spec.js";
import {Plugin} from "prosemirror-state";
import {MAX_TRANSIENT_IMAGE_IMPORT_BYTES} from "../config.js";
import {emitAttachmentCommand} from "../bridge/events.js";
import {activeEditorController} from "../runtime/state.js";




function imageMimeType(file) {
  return typeof file?.type === 'string' ? file.type.trim().toLowerCase() : '';
}

function isImageImportFile(file) {
  if (!(file instanceof File)) return false;
  const supplied = imageMimeType(file);
  return supplied === '' || supplied === 'application/octet-stream' || supplied.startsWith('image/');
}

function imageImportFilesFromTransfer(transfer) {
  const direct = Array.from(transfer?.files || []).filter(isImageImportFile);
  if (direct.length > 0) return direct;
  const files = [];
  for (const item of Array.from(transfer?.items || [])) {
    if (item?.kind !== 'file') continue;
    const supplied = typeof item.type === 'string' ? item.type.trim().toLowerCase() : '';
    if (supplied && !supplied.startsWith('image/')) continue;
    const file = item.getAsFile?.();
    if (isImageImportFile(file)) files.push(file);
  }
  return files;
}

function normalizedImageImportStageUrl(value) {
  const source = String(value || '').trim();
  if (!source) return '';
  try {
    const url = new URL(source);
    if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1') return '';
    if (!/^\/stage\/[0-9a-f]{64}\/image-import$/.test(url.pathname)) return '';
    url.search = '';
    url.hash = '';
    return url.toString().replace(/\/$/, '');
  } catch {
    return '';
  }
}

function imageImportStageEndpoint(stageUrl, token = '') {
  const base = normalizedImageImportStageUrl(stageUrl);
  if (base) return token ? `${base}/${encodeURIComponent(token)}` : base;
  return token
    ? `/rustnoteattachment/image-import-stage/${encodeURIComponent(token)}`
    : '/rustnoteattachment/image-import-stage';
}

async function discardStagedImageImport(token, stageUrl = '') {
  const value = String(token || '');
  if (!/^[0-9a-fA-F]{32}$/.test(value)) return false;
  try {
    const response = await fetch(imageImportStageEndpoint(stageUrl, value), {
      method: 'DELETE',
      cache: 'no-store',
      credentials: 'omit',
    });
    return response.ok;
  } catch {
    // Abandoned ready stages are pruned conservatively and cleared at startup.
    return false;
  }
}

async function stageImageImportFile(file, stageUrl = '', noteId = '') {
  if (!(file instanceof File) || !isImageImportFile(file)) {
    throw new Error('Only image files can be embedded here.');
  }
  if (file.size <= 0) throw new Error(`${file.name || 'Image'} is empty.`);
  if (file.size > MAX_TRANSIENT_IMAGE_IMPORT_BYTES) {
    throw new Error(`Image ${file.name || 'file'} is larger than the 64 MB browser import limit.`);
  }
  const mimeType = imageMimeType(file) || 'application/octet-stream';
  let response;
  try {
    response = await fetch(imageImportStageEndpoint(stageUrl), {
      method: 'POST',
      headers: {
        'Content-Type': mimeType,
        'X-Rustnote-Note-Id': String(noteId || ''),
      },
      body: file,
      cache: 'no-store',
      credentials: 'omit',
    });
  } catch {
    throw new Error('Could not connect to Rustnote image staging.');
  }
  if (!response.ok) {
    let message = `Could not stage ${file.name || 'image'} (${response.status}).`;
    try {
      const failure = await response.json();
      if (failure && typeof failure.error === 'string' && failure.error.trim()) message = failure.error.trim();
    } catch { /* keep status-based message */ }
    throw new Error(message);
  }
  let detail;
  try { detail = await response.json(); } catch { throw new Error('Image staging returned an invalid response.'); }
  const token = String(detail?.token || '');
  if (!/^[0-9a-fA-F]{32}$/.test(token)) throw new Error('Image staging returned an invalid token.');
  return {
    file_name: truncateUtf8(file.name || 'pasted-image', specLimit('attachment_display_name_max_bytes')),
    mime_type: mimeType === 'application/octet-stream' ? '' : mimeType,
    stage_token: token,
  };
}

async function stageImageImportFiles(files, stageUrl = '', noteId = '') {
  const payloads = [];
  try {
    for (const file of files) payloads.push(await stageImageImportFile(file, stageUrl, noteId));
    return payloads;
  } catch (error) {
    await Promise.all(payloads.map(payload => discardStagedImageImport(payload.stage_token, stageUrl)));
    throw error;
  }
}

async function discardStagedImageImports(payloads, stageUrl = '') {
  await Promise.all((payloads || []).map(payload => discardStagedImageImport(payload?.stage_token, stageUrl)));
}

function canonicalAttachmentKind(value) {
  return specDomain('attachment_kind').includes(value) ? value : nodeAttrDefault('attachment', 'kind');
}

function attachmentImportPlaceholderHeader(fileName, kind, item = null) {
  const header = document.createElement('span');
  header.className = 'prosemirror-attachment-header';
  const icon = document.createElement('span');
  icon.className = 'prosemirror-attachment-icon';
  icon.textContent = kind === 'file' ? 'FILE' : kind.toUpperCase();
  const details = document.createElement('span');
  details.className = 'prosemirror-attachment-details';
  const name = document.createElement('strong');
  name.className = 'prosemirror-attachment-name';
  name.textContent = fileName;
  const meta = document.createElement('span');
  meta.className = 'prosemirror-attachment-meta';
  meta.textContent = String(item?.status || 'Importing…');
  details.append(name, meta);
  header.append(icon, details);
  return header;
}

function attachmentImportPlaceholderActions(pdf = false, item = null) {
  const actions = document.createElement('span');
  actions.className = pdf
    ? 'prosemirror-attachment-actions prosemirror-pdf-controls'
    : 'prosemirror-attachment-actions';
  const status = document.createElement('span');
  status.className = 'prosemirror-attachment-import-status prosemirror-attachment-action-button';
  status.textContent = String(item?.status || 'Importing…');
  actions.append(status);
  const ingestId = String(item?.ingest_id || '');
  if (ingestId) {
    const open = document.createElement('button');
    open.type = 'button';
    open.className = 'prosemirror-attachment-action-button';
    open.textContent = 'Open';
    open.title = 'Open a temporary copy while Rustnote verifies the attachment';
    open.addEventListener('mousedown', event => event.preventDefault());
    open.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      emitAttachmentCommand({kind: 'open_pending_attachment', ingest_id: ingestId});
    });
    actions.append(open);
  }
  return actions;
}

function attachmentImportPlaceholderCard(fileName, kind, item = null) {
  const card = document.createElement('span');
  card.classList.add('prosemirror-attachment-import-card', 'prosemirror-attachment-node');
  card.dataset.kind = kind;

  if (kind === 'image') {
    card.classList.add('prosemirror-image-node');
    const shell = document.createElement('span');
    shell.className = 'prosemirror-image-shell prosemirror-attachment-import-image-shell';
    const frame = document.createElement('span');
    frame.className = 'prosemirror-image-frame prosemirror-attachment-import-image-frame';
    const missing = document.createElement('span');
    missing.className = 'prosemirror-image-missing prosemirror-attachment-import-image-missing';
    const name = document.createElement('strong');
    name.className = 'prosemirror-attachment-import-image-name';
    name.textContent = fileName;
    const status = document.createElement('span');
    status.textContent = String(item?.status || 'Importing image…');
    missing.append(name, status);
    frame.append(missing);
    shell.append(frame);
    card.append(shell);
    if (String(item?.ingest_id || '')) {
      card.append(attachmentImportPlaceholderActions(false, item));
    }
    return card;
  }

  if (kind === 'audio' || kind === 'video') {
    card.classList.add('prosemirror-media-node', `prosemirror-${kind}-node`);
    const frame = document.createElement('span');
    frame.className = 'prosemirror-media-frame prosemirror-attachment-import-media-frame';
    const missing = document.createElement('span');
    missing.className = 'prosemirror-media-missing';
    missing.textContent = String(item?.status || (kind === 'video' ? 'Importing video…' : 'Importing audio…'));
    frame.append(missing);
    card.append(
      attachmentImportPlaceholderHeader(fileName, kind, item),
      frame,
      attachmentImportPlaceholderActions(false, item),
    );
    return card;
  }

  if (kind === 'pdf') {
    card.classList.add('prosemirror-pdf-node');
    card.append(
      attachmentImportPlaceholderHeader(fileName, kind, item),
      attachmentImportPlaceholderActions(true, item),
    );
    return card;
  }

  card.classList.add('prosemirror-file-node');
  card.append(
    attachmentImportPlaceholderHeader(fileName, kind, item),
    attachmentImportPlaceholderActions(false, item),
  );
  return card;
}

function attachmentImportPlaceholderDom(items) {
  const container = document.createElement('div');
  container.className = 'prosemirror-attachment-import-placeholder';
  container.contentEditable = 'false';
  container.setAttribute('role', 'status');
  container.setAttribute('aria-live', 'polite');

  const normalized = Array.isArray(items) && items.length > 0
    ? items.slice(0, specLimit('attachment_items_max'))
    : [{file_name: 'Attachment', kind: 'file'}];
  for (const item of normalized) {
    const fileName = String(item?.file_name || 'Attachment').slice(0, 160);
    const kind = canonicalAttachmentKind(item?.kind);
    container.appendChild(attachmentImportPlaceholderCard(fileName, kind, item));
  }
  if (Array.isArray(items) && items.length > normalized.length) {
    const more = document.createElement('span');
    more.className = 'prosemirror-attachment-import-more';
    more.textContent = `+${items.length - normalized.length} more`;
    container.appendChild(more);
  }
  return container;
}

function attachmentImportPlugin() {
  return new Plugin({
    props: {
      handlePaste(view, event) {
        if (!event) return false;
        const files = imageImportFilesFromTransfer(event.clipboardData);
        if (files.length === 0) return false;
        const controller = activeEditorController;
        if (!controller?.editable) return false;
        event.preventDefault();
        controller.queueImageImport(files, {mode: 'insert'});
        return true;
      },
      handleDrop(view, event, _slice, moved) {
        if (moved) return false;
        const files = imageImportFilesFromTransfer(event.dataTransfer);
        if (files.length === 0) return false;
        const controller = activeEditorController;
        if (!controller?.editable) return false;
        event.preventDefault();
        const position = view.posAtCoords({left: event.clientX, top: event.clientY})?.pos;
        controller.queueImageImport(files, {
          mode: 'insert',
          position: Number.isFinite(position) ? position : null,
        });
        return true;
      },
    },
  });
}

function mediaTimeLabelStatic(seconds) {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const total = Math.floor(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = String(total % 60).padStart(2, '0');
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, '0')}:${secs}` : `${minutes}:${secs}`;
}
export {attachmentImportPlaceholderDom, attachmentImportPlugin, canonicalAttachmentKind, discardStagedImageImports, isImageImportFile, mediaTimeLabelStatic, stageImageImportFiles};

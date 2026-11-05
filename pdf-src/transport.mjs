import * as pdfjsLib from 'rustnote-pdfjs-display';
import {pdfTransportPlan} from './transport_policy.mjs';
import {
  DEFAULT_PDF_TRANSPORT_BLOCK_BYTES,
  MAX_CONCURRENT_PDF_BLOCK_FETCHES,
  MAX_PDF_TRANSPORT_REQUEST_BYTES,
  PDF_TRANSPORT_CACHE_BUDGET_BYTES,
  PDF_TRANSPORT_READ_AHEAD_DELAY_MS,
} from './config.mjs';

const {PDFDataRangeTransport} = pdfjsLib;

export async function pdfDocumentMetadata(url) {
  const response = await fetch(url, {
    method: 'GET',
    cache: 'no-store',
    credentials: 'omit',
  });
  if (!response.ok) throw new Error(`PDF metadata request failed (${response.status})`);
  const payload = await response.json();
  const metadata = {
    length: Number(payload?.length),
    wholeFileMaxBytes: Number(payload?.whole_file_max_bytes),
    blockBytes: Number(payload?.block_bytes),
    maxBlockBytes: Number(payload?.max_block_bytes),
    readAheadBlocks: Number(payload?.read_ahead_blocks),
  };
  if (!Number.isSafeInteger(metadata.length) || metadata.length <= 0
      || !Number.isSafeInteger(metadata.wholeFileMaxBytes) || metadata.wholeFileMaxBytes <= 0
      || !Number.isSafeInteger(metadata.blockBytes) || metadata.blockBytes <= 0
      || !Number.isSafeInteger(metadata.maxBlockBytes) || metadata.maxBlockBytes < metadata.blockBytes
      || !Number.isSafeInteger(metadata.readAheadBlocks) || metadata.readAheadBlocks < 0) {
    throw new Error('PDF transport metadata is invalid');
  }
  return metadata;
}

async function fetchPdfWholeDocument(url, totalLength, signal = undefined) {
  const separator = url.includes('?') ? '&' : '?';
  const response = await fetch(`${url}${separator}full=1`, {
    method: 'GET',
    cache: 'no-store',
    credentials: 'omit',
    signal,
  });
  if (!response.ok) throw new Error(`PDF whole-file request failed (${response.status})`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.byteLength !== totalLength) throw new Error('PDF whole-file response length mismatch');
  return bytes;
}

async function fetchPdfBlock(url, offset, length, totalLength, signal = undefined) {
  if (!Number.isSafeInteger(offset) || offset < 0 || offset >= totalLength
      || !Number.isSafeInteger(length) || length <= 0 || offset + length > totalLength) {
    throw new Error(`Invalid PDF block request: ${offset}+${length}`);
  }
  const separator = url.includes('?') ? '&' : '?';
  const response = await fetch(`${url}${separator}offset=${offset}&length=${length}`, {
    method: 'GET',
    cache: 'no-store',
    credentials: 'omit',
    signal,
  });
  if (!response.ok) throw new Error(`PDF block request failed (${response.status})`);
  const block = new Uint8Array(await response.arrayBuffer());
  if (block.byteLength !== length) throw new Error(`PDF block response length mismatch at ${offset}`);
  return block;
}

export async function preparePdfDocumentTransport(
  url,
  metadata,
  onRangeFailure,
  signal = undefined,
) {
  const plan = pdfTransportPlan(metadata);
  if (plan.mode === 'whole-file') {
    const data = await fetchPdfWholeDocument(url, plan.length, signal);
    return Object.freeze({
      mode: plan.mode,
      detail: `whole-file ${plan.length} bytes`,
      range: null,
      documentOptions: Object.freeze({data}),
    });
  }

  const initialData = await fetchPdfBlock(
    url,
    0,
    plan.initialLength,
    plan.length,
    signal,
  );
  const range = new RustnotePdfRangeTransport(
    url,
    plan.length,
    initialData,
    {blockBytes: metadata.blockBytes, readAheadBlocks: metadata.readAheadBlocks},
    onRangeFailure,
  );
  return Object.freeze({
    mode: plan.mode,
    detail: `blocks ${metadata.blockBytes} bytes`,
    range,
    documentOptions: Object.freeze({
      range,
      disableRange: false,
      // A custom range transport has no progressive byte stream. Keeping
      // streaming disabled lets PDF.js use range auto-fetch instead of
      // waiting for progressive data that Rustnote intentionally does not
      // provide for very large local files.
      disableStream: true,
      disableAutoFetch: false,
      rangeChunkSize: metadata.blockBytes,
    }),
  });
}

export class RustnotePdfRangeTransport extends PDFDataRangeTransport {
  constructor(url, length, initialData, options, onFailure) {
    super(length, initialData);
    this.url = url;
    this.onFailure = onFailure;
    this.blockBytes = options.blockBytes || DEFAULT_PDF_TRANSPORT_BLOCK_BYTES;
    this.readAheadBlocks = options.readAheadBlocks;
    this.aborted = false;
    this.failed = false;
    this.controllers = new Set();
    this.activeFetches = 0;
    this.fetchWaiters = [];
    this.blockCache = new Map();
    this.blockCacheBytes = 0;
    this.pendingBlocks = new Map();
    this.lastLogicalBegin = null;
    this.readDirection = 1;
    this.readAheadTimer = 0;
    this.cacheBlock(0, initialData);
  }

  requestDataRange(begin, end) {
    const length = end - begin;
    if (!Number.isSafeInteger(begin) || !Number.isSafeInteger(end)
      || begin < 0 || end > this.length || length <= 0
      || length > MAX_PDF_TRANSPORT_REQUEST_BYTES) {
      this.fail(new Error(`Rustnote rejected an invalid PDF range: ${begin}-${end}`));
      return;
    }
    if (this.aborted || this.failed) return;
    this.cancelScheduledReadAhead();
    if (this.lastLogicalBegin != null && begin !== this.lastLogicalBegin) {
      this.readDirection = begin > this.lastLogicalBegin ? 1 : -1;
    }
    this.lastLogicalBegin = begin;
    void this.loadRange(begin, end).catch(error => this.fail(error));
  }

  async acquireFetchSlot() {
    if (this.activeFetches < MAX_CONCURRENT_PDF_BLOCK_FETCHES) {
      this.activeFetches += 1;
      return;
    }
    await new Promise(resolve => this.fetchWaiters.push(resolve));
    this.activeFetches += 1;
  }

  releaseFetchSlot() {
    this.activeFetches = Math.max(0, this.activeFetches - 1);
    this.fetchWaiters.shift()?.();
  }

  cacheBlock(offset, bytes) {
    if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0) return;
    const existing = this.blockCache.get(offset);
    if (existing) this.blockCacheBytes -= existing.byteLength;
    this.blockCache.delete(offset);
    this.blockCache.set(offset, bytes);
    this.blockCacheBytes += bytes.byteLength;
    while (this.blockCacheBytes > PDF_TRANSPORT_CACHE_BUDGET_BYTES && this.blockCache.size > 1) {
      const oldestOffset = this.blockCache.keys().next().value;
      if (oldestOffset === offset) break;
      const oldest = this.blockCache.get(oldestOffset);
      this.blockCache.delete(oldestOffset);
      this.blockCacheBytes -= oldest?.byteLength ?? 0;
    }
  }

  cachedBlock(offset) {
    const block = this.blockCache.get(offset);
    if (!block) return null;
    this.blockCache.delete(offset);
    this.blockCache.set(offset, block);
    return block;
  }

  async blockAt(offset) {
    const cached = this.cachedBlock(offset);
    if (cached) return cached;
    const pending = this.pendingBlocks.get(offset);
    if (pending) return pending;
    const length = Math.min(this.blockBytes, this.length - offset);
    const controller = new AbortController();
    this.controllers.add(controller);
    const promise = (async () => {
      await this.acquireFetchSlot();
      try {
        if (this.aborted || this.failed) throw new DOMException('PDF transport stopped', 'AbortError');
        const block = await fetchPdfBlock(
          this.url,
          offset,
          length,
          this.length,
          controller.signal,
        );
        this.cacheBlock(offset, block);
        return block;
      } finally {
        this.releaseFetchSlot();
        this.controllers.delete(controller);
        this.pendingBlocks.delete(offset);
      }
    })();
    this.pendingBlocks.set(offset, promise);
    return promise;
  }

  async loadRange(begin, end) {
    if (this.aborted || this.failed) return;
    const bytes = new Uint8Array(end - begin);
    const firstBlock = Math.floor(begin / this.blockBytes) * this.blockBytes;
    const blockOffsets = [];
    for (let offset = firstBlock; offset < end; offset += this.blockBytes) blockOffsets.push(offset);
    const blocks = await Promise.all(blockOffsets.map(offset => this.blockAt(offset)));
    if (this.aborted || this.failed) return;
    for (let index = 0; index < blockOffsets.length; index += 1) {
      const blockOffset = blockOffsets[index];
      const block = blocks[index];
      const copyBegin = Math.max(begin, blockOffset);
      const copyEnd = Math.min(end, blockOffset + block.byteLength);
      if (copyEnd > copyBegin) {
        bytes.set(
          block.subarray(copyBegin - blockOffset, copyEnd - blockOffset),
          copyBegin - begin,
        );
      }
    }
    this.onDataRange(begin, bytes);
    this.scheduleReadAhead(begin, end);
  }

  scheduleReadAhead(begin, end) {
    if (this.readAheadBlocks <= 0 || this.aborted || this.failed) return;
    const generationBegin = begin;
    this.readAheadTimer = setTimeout(() => {
      this.readAheadTimer = 0;
      if (this.aborted || this.failed || this.lastLogicalBegin !== generationBegin) return;
      const offsets = [];
      if (this.readDirection > 0) {
        let offset = Math.ceil(end / this.blockBytes) * this.blockBytes;
        for (let index = 0; index < this.readAheadBlocks && offset < this.length; index += 1) {
          offsets.push(offset);
          offset += this.blockBytes;
        }
      } else {
        let offset = Math.floor(begin / this.blockBytes) * this.blockBytes - this.blockBytes;
        for (let index = 0; index < this.readAheadBlocks && offset >= 0; index += 1) {
          offsets.push(offset);
          offset -= this.blockBytes;
        }
      }
      for (const offset of offsets) {
        if (this.blockCache.has(offset) || this.pendingBlocks.has(offset)) continue;
        void this.blockAt(offset).catch(error => {
          if (error?.name !== 'AbortError') console.warn('Rustnote PDF read-ahead failed', error);
        });
      }
    }, PDF_TRANSPORT_READ_AHEAD_DELAY_MS);
  }

  cancelScheduledReadAhead() {
    if (this.readAheadTimer) {
      clearTimeout(this.readAheadTimer);
      this.readAheadTimer = 0;
    }
  }

  fail(error) {
    if (this.aborted || this.failed) return;
    this.failed = true;
    this.cancelScheduledReadAhead();
    for (const controller of this.controllers) controller.abort();
    this.controllers.clear();
    while (this.fetchWaiters.length > 0) this.fetchWaiters.shift()?.();
    this.onFailure(error);
  }

  abort() {
    this.aborted = true;
    this.cancelScheduledReadAhead();
    for (const controller of this.controllers) controller.abort();
    this.controllers.clear();
    while (this.fetchWaiters.length > 0) this.fetchWaiters.shift()?.();
    this.blockCache.clear();
    this.blockCacheBytes = 0;
    this.pendingBlocks.clear();
  }
}

function disposeCanvas(canvas) {
  if (!canvas) return;
  canvas.width = 1;
  canvas.height = 1;
}

export function canvasBackingBytes(canvas, bytesPerPixel = 4) {
  if (!canvas || canvas.width < 1 || canvas.height < 1) return 0;
  return Math.max(0, canvas.width * canvas.height * bytesPerPixel);
}

/**
 * Byte-accounted LRU cache for optional PDF preview rasters.
 *
 * The viewer owns which pages are protected and reports current page-canvas
 * pressure; this class owns cache mutation, accounting, adaptive budgeting,
 * and deterministic eviction/disposal.
 */
export class PreviewRasterCache {
  constructor({
    hardLimitBytes,
    minimumBudgetBytes,
    canvasMinimumOperatingBytes,
    canvasHardLimitBytes,
  }) {
    this.hardLimitBytes = Math.max(0, Number(hardLimitBytes) || 0);
    this.minimumBudgetBytes = Math.max(0, Math.min(this.hardLimitBytes, Number(minimumBudgetBytes) || 0));
    this.canvasMinimumOperatingBytes = Math.max(0, Number(canvasMinimumOperatingBytes) || 0);
    this.canvasHardLimitBytes = Math.max(this.canvasMinimumOperatingBytes, Number(canvasHardLimitBytes) || 0);
    this.entries = new Map();
    this.bytes = 0;
    this.budgetBytes = this.hardLimitBytes;
  }

  get size() {
    return this.entries.size;
  }

  has(pageNumber) {
    return this.entries.has(pageNumber);
  }

  get(pageNumber) {
    const entry = this.entries.get(pageNumber);
    if (!entry) return null;
    this.entries.delete(pageNumber);
    this.entries.set(pageNumber, entry);
    return entry;
  }

  clear() {
    for (const entry of this.entries.values()) disposeCanvas(entry.canvas);
    this.entries.clear();
    this.bytes = 0;
    this.budgetBytes = this.hardLimitBytes;
  }

  store(pageNumber, {canvas, baseWidth, baseHeight}) {
    if (!canvas || canvas.width < 1 || canvas.height < 1) return null;
    const previous = this.entries.get(pageNumber);
    if (previous) {
      this.bytes = Math.max(0, this.bytes - previous.bytes);
      disposeCanvas(previous.canvas);
      this.entries.delete(pageNumber);
    }
    const entry = {
      canvas,
      baseWidth,
      baseHeight,
      bytes: canvasBackingBytes(canvas),
    };
    this.entries.set(pageNumber, entry);
    this.bytes += entry.bytes;
    return entry;
  }

  updateAdaptiveBudget({retainedCanvasBytes, auxiliaryRasterBytes}) {
    const retained = Math.max(0, Number(retainedCanvasBytes) || 0);
    const auxiliary = Math.max(0, Number(auxiliaryRasterBytes) || 0);
    const pressure = Math.max(0, retained - this.canvasMinimumOperatingBytes);
    const pressureRange = Math.max(1, this.canvasHardLimitBytes - this.canvasMinimumOperatingBytes);
    const pressureRatio = Math.max(0, Math.min(1, pressure / pressureRange));
    const adaptiveBudget = Math.round(
      this.hardLimitBytes - (this.hardLimitBytes - this.minimumBudgetBytes) * pressureRatio,
    );
    const hardRemaining = Math.max(0, this.canvasHardLimitBytes - retained - auxiliary);
    this.budgetBytes = Math.min(adaptiveBudget, hardRemaining);
    return this.budgetBytes;
  }

  evictToBudget({
    retainedCanvasBytes,
    auxiliaryRasterBytes,
    protectedPages = [],
    forcedBudget = null,
  }) {
    const retained = Math.max(0, Number(retainedCanvasBytes) || 0);
    const auxiliary = Math.max(0, Number(auxiliaryRasterBytes) || 0);
    if (forcedBudget == null) {
      this.updateAdaptiveBudget({retainedCanvasBytes: retained, auxiliaryRasterBytes: auxiliary});
    } else {
      this.budgetBytes = Math.max(0, Math.min(this.hardLimitBytes, Number(forcedBudget) || 0));
    }

    const protectedSet = protectedPages instanceof Set ? protectedPages : new Set(protectedPages);
    const strictHardPressure = retained + auxiliary + this.bytes > this.canvasHardLimitBytes;
    let attempts = this.entries.size * 2 + 1;
    while (this.bytes > this.budgetBytes && this.entries.size > 0 && attempts > 0) {
      attempts -= 1;
      const oldestPage = this.entries.keys().next().value;
      const entry = this.entries.get(oldestPage);
      this.entries.delete(oldestPage);
      if (!strictHardPressure && protectedSet.has(oldestPage) && this.entries.size > 0) {
        this.entries.set(oldestPage, entry);
        continue;
      }
      this.bytes = Math.max(0, this.bytes - (entry?.bytes ?? 0));
      disposeCanvas(entry?.canvas);
    }
    return this.bytes;
  }
}

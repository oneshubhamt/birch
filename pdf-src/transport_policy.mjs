export function pdfTransportPlan(metadata) {
  const length = Number(metadata?.length);
  const wholeFileMaxBytes = Number(metadata?.wholeFileMaxBytes);
  const blockBytes = Number(metadata?.blockBytes);
  if (!Number.isSafeInteger(length) || length <= 0
      || !Number.isSafeInteger(wholeFileMaxBytes) || wholeFileMaxBytes <= 0
      || !Number.isSafeInteger(blockBytes) || blockBytes <= 0) {
    throw new Error('PDF transport metadata is invalid');
  }
  if (length <= wholeFileMaxBytes) {
    return Object.freeze({mode: 'whole-file', length});
  }
  return Object.freeze({
    mode: 'range',
    length,
    initialLength: Math.min(blockBytes, length),
  });
}

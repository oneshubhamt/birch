const TRANSPORT_ERROR_PATTERNS = [
  /^PDF metadata request failed\b/,
  /^PDF whole-file request failed\b/,
  /^PDF block request failed\b/,
  /^PDF whole-file response length mismatch\b/,
  /^PDF block response length mismatch\b/,
  /^PDF transport metadata is invalid\b/,
  /^Invalid PDF block request\b/,
  /^Rustnote rejected an invalid PDF range\b/,
];

export function classifyPdfPreviewError(error) {
  const name = String(error?.name ?? '');
  const detail = String(error?.message ?? '');

  if (name === 'PasswordException') {
    return Object.freeze({
      category: 'password',
      message: 'Password-protected PDFs should be opened externally.',
    });
  }
  if (name === 'InvalidPDFException') {
    return Object.freeze({
      category: 'document',
      message: 'This PDF could not be read. It may be damaged or unsupported. Use Open externally.',
    });
  }
  if (name === 'MissingPDFException') {
    return Object.freeze({
      category: 'unavailable',
      message: 'The PDF attachment is unavailable. Try reopening the note or use Open externally.',
    });
  }
  if (name === 'UnexpectedResponseException'
      || TRANSPORT_ERROR_PATTERNS.some(pattern => pattern.test(detail))) {
    return Object.freeze({
      category: 'transport',
      message: 'PDF preview data could not be loaded. Try reopening the preview or use Open externally.',
    });
  }
  return Object.freeze({
    category: 'viewer',
    message: 'Rustnote PDF preview failed. Use Open externally.',
  });
}

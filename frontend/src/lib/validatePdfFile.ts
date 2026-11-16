export type PdfFileError = 'too_large' | 'not_pdf';

export interface PdfFileMeta {
  name: string;
  size: number;
  type: string;
}

/**
 * Client-side file check. Returns `too_large` when size exceeds maxBytes,
 * `not_pdf` when the name lacks a `.pdf` extension (case-insensitive) AND the
 * MIME type is not `application/pdf`, otherwise `null`.
 */
export function validatePdfFile(meta: PdfFileMeta, maxBytes: number): PdfFileError | null {
  if (meta.size > maxBytes) return 'too_large';
  const hasPdfExtension = meta.name.toLowerCase().endsWith('.pdf');
  if (!hasPdfExtension && meta.type !== 'application/pdf') return 'not_pdf';
  return null;
}

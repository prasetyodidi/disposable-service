export interface PdfOptionsInput {
  from: string;
  to: string;
  pageBreaks: boolean;
  strip: boolean;
}

/** Wire format of the `options` multipart part. */
export interface PdfRequestOptions {
  page_from: number;
  page_to?: number;
  page_breaks: boolean;
  strip_headers_footers: boolean;
}

export type PdfOptionsResult =
  | { ok: true; options: PdfRequestOptions }
  | { ok: false; field: 'page_from' | 'page_to'; message: string };

const POSITIVE_INT = /^[1-9][0-9]*$/;

/**
 * Validates the page range fields. Empty string means default
 * (page_from = 1, page_to omitted = last page).
 */
export function parsePdfOptions(input: PdfOptionsInput): PdfOptionsResult {
  const { from, to, pageBreaks, strip } = input;

  if (from !== '' && !POSITIVE_INT.test(from)) {
    return { ok: false, field: 'page_from', message: 'Halaman awal harus bilangan bulat positif.' };
  }
  if (to !== '' && !POSITIVE_INT.test(to)) {
    return { ok: false, field: 'page_to', message: 'Halaman akhir harus bilangan bulat positif.' };
  }

  const pageFrom = from === '' ? 1 : Number(from);
  const pageTo = to === '' ? undefined : Number(to);

  if (pageTo !== undefined && pageFrom > pageTo) {
    return {
      ok: false,
      field: 'page_from',
      message: 'Halaman awal tidak boleh lebih besar dari halaman akhir.',
    };
  }

  const options: PdfRequestOptions = {
    page_from: pageFrom,
    page_breaks: pageBreaks,
    strip_headers_footers: strip,
  };
  if (pageTo !== undefined) options.page_to = pageTo;
  return { ok: true, options };
}

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { parsePdfOptions } from './parsePdfOptions';

const VALID = /^[1-9][0-9]*$/;

// Isian valid (bilangan bulat positif, <= 15 digit agar tepat sebagai Number), kosong, atau string acak/mendekati valid.
const fieldArb = fc.oneof(
  fc.integer({ min: 1, max: 400 }).map(String),
  fc.integer({ min: 1, max: 999_999_999_999_999 }).map(String),
  fc.constant(''),
  fc.constantFrom('0', '00', '01', '-1', '1.5', '1e3', ' 1', '1 ', '+1', 'abc', '１', '0x10'),
  fc.string({ maxLength: 8 }),
  fc.stringMatching(/^[0-9]{1,6}$/),
);

// Feature: pdf-to-markdown, Property 35: Validasi opsi di klien
describe('parsePdfOptions', () => {
  it('Property 35: menerima iff tiap isian tidak kosong cocok ^[1-9][0-9]*$ dan from <= to; galat menyebut isian bermasalah', () => {
    // **Validates: Requirements 13.5, 13.6**
    fc.assert(
      fc.property(fieldArb, fieldArb, fc.boolean(), fc.boolean(), (from, to, pageBreaks, strip) => {
        const r = parsePdfOptions({ from, to, pageBreaks, strip });

        const fromBad = from !== '' && !VALID.test(from);
        const toBad = to !== '' && !VALID.test(to);
        const rangeBad =
          !fromBad && !toBad && to !== '' && Number(from === '' ? '1' : from) > Number(to);

        if (fromBad) {
          expect(r.ok).toBe(false);
          if (!r.ok) expect(r.field).toBe('page_from');
        } else if (toBad) {
          expect(r.ok).toBe(false);
          if (!r.ok) expect(r.field).toBe('page_to');
        } else if (rangeBad) {
          expect(r.ok).toBe(false);
          if (!r.ok) expect(r.field).toBe('page_from');
        } else {
          expect(r.ok).toBe(true);
          if (r.ok) {
            expect(r.options.page_from).toBe(from === '' ? 1 : Number(from));
            if (to === '') expect('page_to' in r.options).toBe(false);
            else expect(r.options.page_to).toBe(Number(to));
            expect(r.options.page_breaks).toBe(pageBreaks);
            expect(r.options.strip_headers_footers).toBe(strip);
          }
        }
        if (!r.ok) expect(r.message).toBeTruthy();
      }),
      { numRuns: 200 },
    );
  });

  it('isian kosong menghasilkan nilai bawaan', () => {
    // **Validates: Requirements 13.5**
    const r = parsePdfOptions({ from: '', to: '', pageBreaks: false, strip: false });
    expect(r).toEqual({
      ok: true,
      options: { page_from: 1, page_breaks: false, strip_headers_footers: false },
    });
  });

  it('from > to menolak pada page_from; from = to diterima', () => {
    // **Validates: Requirements 13.6**
    const bad = parsePdfOptions({ from: '5', to: '3', pageBreaks: false, strip: false });
    expect(bad.ok).toBe(false);
    if (!bad.ok) expect(bad.field).toBe('page_from');
    expect(parsePdfOptions({ from: '3', to: '3', pageBreaks: true, strip: true }).ok).toBe(true);
  });
});

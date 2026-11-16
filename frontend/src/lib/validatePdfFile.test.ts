import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { validatePdfFile } from './validatePdfFile';

// Feature: pdf-to-markdown, Property 34: Validasi file di klien
describe('validatePdfFile', () => {
  const nameArb = fc.oneof(
    fc.string({ maxLength: 20 }),
    fc.string({ maxLength: 20 }).map((s) => `${s}.pdf`),
    fc.string({ maxLength: 20 }).map((s) => `${s}.PdF`),
    fc.string({ maxLength: 20 }).map((s) => `${s}.pdf.txt`),
    fc.string({ maxLength: 20 }).map((s) => `${s}.PDF`),
  );
  const typeArb = fc.oneof(
    fc.constant('application/pdf'),
    fc.constant(''),
    fc.constant('text/plain'),
    fc.string({ maxLength: 20 }),
  );

  it('Property 34: too_large / not_pdf / null sesuai aturan', () => {
    // **Validates: Requirements 13.4**
    fc.assert(
      fc.property(
        nameArb,
        typeArb,
        fc.integer({ min: 0, max: 1000 }),
        fc.integer({ min: 0, max: 1000 }),
        (name, type, size, maxBytes) => {
          const r = validatePdfFile({ name, type, size }, maxBytes);
          const isPdf = name.toLowerCase().endsWith('.pdf') || type === 'application/pdf';
          if (size > maxBytes) expect(r).toBe('too_large');
          else if (!isPdf) expect(r).toBe('not_pdf');
          else expect(r).toBeNull();
        },
      ),
      { numRuns: 200 },
    );
  });

  it('batas persis: size == maxBytes diterima, maxBytes + 1 ditolak', () => {
    // **Validates: Requirements 13.4**
    fc.assert(
      fc.property(fc.integer({ min: 0, max: 100000 }), (maxBytes) => {
        const base = { name: 'a.pdf', type: 'application/pdf' };
        expect(validatePdfFile({ ...base, size: maxBytes }, maxBytes)).toBeNull();
        expect(validatePdfFile({ ...base, size: maxBytes + 1 }, maxBytes)).toBe('too_large');
      }),
      { numRuns: 100 },
    );
  });
});

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { validateRedactorInput, REDACTOR_MAX_CHARS } from './validateRedactorInput';

// Feature: privacy-micro-apps-and-burn-workspace, Property 25: Validasi masukan redactor di klien
describe('validateRedactorInput', () => {
  it('Property 25: menolak iff panjang > 1.048.576; teks tidak berubah', () => {
    // **Validates: Requirements 7.5**
    // Panjang dibangkitkan di sekitar batas, lalu string dibuat dengan repeat agar cepat.
    const lengthArb = fc.oneof(
      fc.integer({ min: 0, max: 200 }),
      fc.integer({ min: REDACTOR_MAX_CHARS - 3, max: REDACTOR_MAX_CHARS + 3 }),
      fc.integer({ min: 0, max: REDACTOR_MAX_CHARS + 1000 }),
    );
    fc.assert(
      fc.property(lengthArb, fc.constantFrom('a', 'é', ' ', '\n'), (len, ch) => {
        const text = ch.repeat(len);
        const r = validateRedactorInput(text);
        expect(r.ok).toBe(text.length <= REDACTOR_MAX_CHARS);
        expect(r.text).toBe(text);
        if (!r.ok) expect(r.error).toBeTruthy();
      }),
      { numRuns: 100 },
    );
  });

  it('string acak biasa selalu diterima dan dikembalikan apa adanya', () => {
    // **Validates: Requirements 7.5**
    fc.assert(
      fc.property(fc.string({ maxLength: 500 }), (text) => {
        const r = validateRedactorInput(text);
        expect(r.ok).toBe(true);
        expect(r.text).toBe(text);
      }),
      { numRuns: 100 },
    );
  });
});

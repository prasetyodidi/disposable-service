import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { pickFirstFile } from './pickFirstFile';

// Feature: privacy-micro-apps-and-burn-workspace, Property 9: Hanya file pertama yang diterima
describe('pickFirstFile', () => {
  it('Property 9: hanya file pertama diterima; multiple true iff n > 1', () => {
    // **Validates: Requirements 4.4**
    fc.assert(
      fc.property(fc.array(fc.string(), { minLength: 1, maxLength: 20 }), (files) => {
        const r = pickFirstFile(files);
        expect(r.file).toBe(files[0]);
        expect(r.multiple).toBe(files.length > 1);
      }),
      { numRuns: 100 },
    );
  });

  it('daftar kosong atau null menghasilkan file null', () => {
    expect(pickFirstFile([])).toEqual({ file: null, multiple: false });
    expect(pickFirstFile(null)).toEqual({ file: null, multiple: false });
  });
});

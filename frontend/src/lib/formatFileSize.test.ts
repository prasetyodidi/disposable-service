import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { formatFileSize } from './formatFileSize';

const UNIT_BYTES: Record<string, number> = { B: 1, KB: 1024, MB: 1024 * 1024 };

describe('formatFileSize', () => {
  // Feature: privacy-micro-apps-and-burn-workspace, Property 8: Format ukuran file
  // **Validates: Requirements 4.3**
  it('Property 8: satuan tepat B/KB/MB, maks 2 desimal, round-trip dalam toleransi pembulatan', () => {
    fc.assert(
      fc.property(
        fc.oneof(
          fc.integer({ min: 0, max: 1023 }),
          fc.integer({ min: 1024, max: 1024 * 1024 }),
          fc.integer({ min: 0, max: 100 * 1024 * 1024 }),
          fc.integer({ min: 0, max: Number.MAX_SAFE_INTEGER }),
        ),
        (bytes) => {
          const out = formatFileSize(bytes);
          const m = /^(\d+(?:\.\d{1,2})?) (B|KB|MB)$/.exec(out);
          expect(m).not.toBeNull();
          const value = parseFloat(m![1]);
          const unit = m![2];
          const factor = UNIT_BYTES[unit];
          // Satuan B tidak punya desimal
          if (unit === 'B') expect(m![1]).not.toContain('.');
          // Toleransi pembulatan: setengah satuan terkecil (0.005) dalam unit terpilih
          const tolerance = unit === 'B' ? 0.5 : 0.005 * factor + 1e-6 * factor;
          expect(Math.abs(value * factor - bytes)).toBeLessThanOrEqual(tolerance);
        },
      ),
      { numRuns: 100 },
    );
  });

  it('contoh spesifik', () => {
    expect(formatFileSize(0)).toBe('0 B');
    expect(formatFileSize(1023)).toBe('1023 B');
    expect(formatFileSize(1024)).toBe('1 KB');
    expect(formatFileSize(1536)).toBe('1.5 KB');
    expect(formatFileSize(1024 * 1024)).toBe('1 MB');
  });
});

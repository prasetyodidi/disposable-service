import { describe, it } from 'vitest';
import fc from 'fast-check';
import { shouldShowZeroBytes } from './shouldShowZeroBytes';

// Feature: privacy-micro-apps-and-burn-workspace, Property 32: Indikator "0 bytes remaining" mengikuti verified
describe('Property 32: Indikator "0 bytes remaining" mengikuti verified', () => {
  it('**Validates: Requirements 11.7, 11.8** true iff verified && remaining_bytes === 0', () => {
    fc.assert(
      fc.property(
        fc.boolean(),
        fc.oneof(fc.constant(0), fc.nat()),
        (verified, remaining_bytes) => {
          const expected = verified && remaining_bytes === 0;
          return shouldShowZeroBytes({ verified, remaining_bytes }) === expected;
        },
      ),
      { numRuns: 100 },
    );
  });

  it('**Validates: Requirements 11.8** never true when verified is false', () => {
    fc.assert(
      fc.property(fc.nat(), (remaining_bytes) => {
        return shouldShowZeroBytes({ verified: false, remaining_bytes }) === false;
      }),
      { numRuns: 100 },
    );
  });
});

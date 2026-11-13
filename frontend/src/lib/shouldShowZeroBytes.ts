import type { AuditReport } from '../types/privacy';

/** Indikator "0 bytes remaining" hanya bila verified = true dan remaining_bytes = 0. */
export function shouldShowZeroBytes(
  report: Pick<AuditReport, 'verified' | 'remaining_bytes'>,
): boolean {
  return report.verified === true && report.remaining_bytes === 0;
}

const KB = 1024;
const MB = 1024 * 1024;

/**
 * Format ukuran byte menjadi "N B", "N KB", atau "N MB" (maks 2 digit desimal).
 * Nilai tidak valid (negatif/NaN) diperlakukan sebagai 0.
 */
export function formatFileSize(bytes: number): string {
  const size = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  const round2 = (n: number) => String(Math.round(n * 100) / 100);
  if (size < KB) return `${Math.round(size)} B`;
  if (size < MB) {
    const kb = Math.round((size / KB) * 100) / 100;
    // Pembulatan bisa mencapai 1024 KB; naikkan ke MB agar tetap konsisten.
    if (kb < KB) return `${round2(kb)} KB`;
  }
  return `${round2(size / MB)} MB`;
}

export const REDACTOR_MAX_CHARS = 1_048_576;

export interface RedactorValidation {
  ok: boolean;
  /** Teks masukan selalu dikembalikan apa adanya (tidak dipotong/diubah). */
  text: string;
  error?: string;
}

/** Tolak masukan jika dan hanya jika panjangnya > 1.048.576 karakter. */
export function validateRedactorInput(text: string): RedactorValidation {
  if (text.length > REDACTOR_MAX_CHARS) {
    return {
      ok: false,
      text,
      error: 'Batas ukuran terlampaui: teks maksimal 1.048.576 karakter.',
    };
  }
  return { ok: true, text };
}

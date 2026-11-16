const UNPROCESSABLE: Record<string, string> = {
  corrupt_pdf: 'File PDF rusak atau tidak dapat dibaca.',
  encrypted_pdf: 'File PDF dilindungi kata sandi atau terenkripsi, sehingga tidak dapat diproses.',
  no_text_layer:
    'PDF tidak memiliki lapisan teks: dokumen tampak berupa hasil pindai dan OCR belum didukung.',
  too_many_pages: 'PDF melebihi jumlah halaman maksimum yang didukung.',
  output_too_large: 'Hasil konversi melebihi ukuran keluaran maksimum.',
};

export function pdfErrorMessage(status: number | 'network', code?: string): string {
  if (status === 'network') {
    return 'Tidak dapat terhubung ke server. Periksa koneksi Anda lalu coba lagi.';
  }
  switch (status) {
    case 401:
      return 'Sesi Anda berakhir. Silakan masuk kembali.';
    case 413:
      return 'Ukuran file melebihi batas maksimum yang diizinkan.';
    case 415:
      return 'File bukan PDF yang valid.';
    case 422:
      return (code && UNPROCESSABLE[code]) || 'PDF tidak dapat dikonversi ke Markdown.';
    case 503:
      return 'Layanan konversi sedang tidak tersedia. Coba lagi nanti.';
    case 504:
      return 'Konversi melebihi batas waktu. Coba dengan dokumen yang lebih kecil atau rentang halaman yang lebih sempit.';
    default:
      return 'Terjadi kesalahan saat mengonversi PDF. Silakan coba lagi.';
  }
}

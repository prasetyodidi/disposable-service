export interface PdfConversionResult {
  markdown: string;
  pages: number;
  headings: number;
  warnings: string[];
  /** UUID; only present when `?download=true` and conversion succeeded. */
  download_token?: string;
}

export interface PdfLimits {
  max_bytes: number;
  max_pages: number;
}

export interface PdfApiError {
  success: false;
  code: string;
  message: string;
}

export interface PdfMcpResult {
  markdown?: string;
  preview?: string;
  download_url?: string;
  pages: number;
  headings: number;
  warnings: string[];
  truncated: boolean;
}

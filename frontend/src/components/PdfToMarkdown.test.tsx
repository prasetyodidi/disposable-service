// @vitest-environment jsdom
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { PdfToMarkdown } from './PdfToMarkdown';
import { pdfErrorMessage } from '../lib/pdfErrorMessage';

// PageShell (Navbar, tema, dsb.) tidak relevan untuk perilaku komponen ini.
vi.mock('./PageShell', () => ({
  PageShell: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

const MAX_BYTES = 1024 * 1024;
const LIMITS_URL = '/api/apps/pdf-to-markdown/limits';
const CONVERT_URL = '/api/apps/pdf-to-markdown';

const MARKDOWN = '# Judul\n\nIsi paragraf <script>alert(1)</script>\n';
const RESULT = {
  markdown: MARKDOWN,
  pages: 7,
  headings: 3,
  warnings: ['peringatan pertama', 'peringatan kedua', 'peringatan ketiga'],
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

function pdfFile(name = 'dok.pdf', bytes = 100, type = 'application/pdf'): File {
  return new File([new Uint8Array(bytes)], name, { type });
}

type Handler = () => Promise<Response> | Response;

/** Pasang fetch palsu: /limits selalu sukses, konversi memakai `convert`. */
function stubFetch(convert: Handler = () => json(RESULT), limits: Handler = () => json({ max_bytes: MAX_BYTES })) {
  const fetchMock = vi.fn((url: string) => {
    if (url === LIMITS_URL) return Promise.resolve(limits());
    return Promise.resolve(convert());
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

const convertCalls = (m: ReturnType<typeof vi.fn>) => m.mock.calls.filter((c) => c[0] === CONVERT_URL);

async function renderReady() {
  const utils = render(<PdfToMarkdown />);
  // Tunggu batas dimuat: teks petunjuk memuat ukuran maksimum.
  await waitFor(() => expect(screen.getByText(/maksimum 1 MB/)).toBeTruthy());
  return utils;
}

function selectFiles(files: File[]) {
  const input = screen.getByLabelText('Pilih file PDF') as HTMLInputElement;
  fireEvent.change(input, { target: { files } });
}

const convertButton = () => screen.getByRole('button', { name: /Konversi ke Markdown|Mengonversi/ }) as HTMLButtonElement;

async function convertSuccessfully() {
  selectFiles([pdfFile()]);
  fireEvent.click(convertButton());
  await waitFor(() => expect(screen.getByTestId('markdown-preview')).toBeTruthy());
}

describe('PdfToMarkdown', () => {
  let anchorClick: ReturnType<typeof vi.spyOn>;
  let hrefSetter: ReturnType<typeof vi.fn<(v: string) => void>>;
  const originalLocation = window.location;

  beforeEach(() => {
    anchorClick = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    hrefSetter = vi.fn();
    const fakeLocation = {} as Location;
    Object.defineProperty(fakeLocation, 'href', { get: () => 'http://localhost/pdf-to-markdown', set: (v: string) => { hrefSetter(v); } });
    Object.defineProperty(window, 'location', { configurable: true, value: fakeLocation });
  });

  afterEach(() => {
    cleanup();
    Object.defineProperty(window, 'location', { configurable: true, value: originalLocation });
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('accepts only the first file when several are selected (Req 13.3)', async () => {
    stubFetch();
    await renderReady();
    selectFiles([pdfFile('pertama.pdf'), pdfFile('kedua.pdf')]);
    expect(screen.getByText('pertama.pdf')).toBeTruthy();
    expect(screen.queryByText('kedua.pdf')).toBeNull();
    expect(screen.getByRole('status').textContent).toMatch(/satu file/i);
  });

  it('rejects a non-PDF file without selecting it (Req 13.4)', async () => {
    const fetchMock = stubFetch();
    await renderReady();
    selectFiles([pdfFile('catatan.txt', 10, 'text/plain')]);
    expect(screen.getByRole('alert').textContent).toMatch(/bukan PDF/i);
    expect(screen.queryByText('catatan.txt')).toBeNull();
    expect(convertButton().disabled).toBe(true);
    expect((screen.getByLabelText('Pilih file PDF') as HTMLInputElement).disabled).toBe(false);
    expect(convertCalls(fetchMock)).toHaveLength(0);
  });

  it('rejects a file above the server limit without calling fetch for conversion (Req 13.4)', async () => {
    const fetchMock = stubFetch();
    await renderReady();
    selectFiles([pdfFile('besar.pdf', MAX_BYTES + 1)]);
    expect(screen.getByRole('alert').textContent).toMatch(/melebihi batas maksimum \(1 MB\)/);
    expect(screen.queryByText('besar.pdf')).toBeNull();
    expect(convertButton().disabled).toBe(true);
    expect((screen.getByLabelText('Pilih file PDF') as HTMLInputElement).disabled).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(1); // hanya /limits
    expect(convertCalls(fetchMock)).toHaveLength(0);
  });

  it('shows the limit received from /limits (Req 13.4)', async () => {
    stubFetch(undefined, () => json({ max_bytes: 5 * 1024 * 1024 }));
    render(<PdfToMarkdown />);
    await waitFor(() => expect(screen.getByText(/maksimum 5 MB/)).toBeTruthy());
  });

  it('shows an error without a guessed number when /limits fails, and does not convert', async () => {
    const fetchMock = stubFetch(undefined, () => new Response('x', { status: 500 }));
    render(<PdfToMarkdown />);
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/Batas ukuran file tidak dapat dimuat/));
    expect(screen.queryByText(/maksimum/)).toBeNull();
    expect(screen.getByRole('alert').textContent).not.toMatch(/\d/);
    selectFiles([pdfFile()]);
    expect(screen.queryByText('dok.pdf')).toBeNull();
    expect(convertCalls(fetchMock)).toHaveLength(0);
  });

  it('disables the convert button and shows a progress indicator while running (Req 13.7)', async () => {
    let release: (r: Response) => void = () => {};
    stubFetch(() => new Promise<Response>((res) => (release = res)));
    await renderReady();
    selectFiles([pdfFile()]);
    expect(convertButton().disabled).toBe(false);
    fireEvent.click(convertButton());
    await waitFor(() => expect(screen.getByText('Mengonversi...')).toBeTruthy());
    expect(convertButton().disabled).toBe(true);
    await act(async () => {
      release(json(RESULT));
    });
    await waitFor(() => expect(screen.getByTestId('markdown-preview')).toBeTruthy());
    expect(convertButton().disabled).toBe(false);
  });

  it('shows pages, headings and warnings in order, defaulting to preview mode, with a raw mode (Req 13.8, 13.9)', async () => {
    stubFetch();
    await renderReady();
    await convertSuccessfully();

    expect(screen.getByText('Halaman:').textContent).toContain('7');
    expect(screen.getByText('Heading:').textContent).toContain('3');
    const items = screen.getAllByRole('listitem').map((li) => li.textContent);
    expect(items).toEqual(RESULT.warnings);

    expect(screen.getByRole('button', { name: 'Pratinjau' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.queryByTestId('markdown-raw')).toBeNull();
    // Pratinjau: heading dirender, HTML ditampilkan sebagai teks (Req 13.18).
    expect(screen.getByRole('heading', { name: 'Judul' })).toBeTruthy();
    expect(screen.getByTestId('markdown-preview').querySelector('script')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Markdown mentah' }));
    expect(screen.getByTestId('markdown-raw').textContent).toBe(MARKDOWN);
    expect(screen.queryByTestId('markdown-preview')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Pratinjau' }));
    expect(screen.getByTestId('markdown-preview')).toBeTruthy();
  });

  it('copies the markdown and shows a confirmation for 2 seconds (Req 13.10)', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    stubFetch();
    await renderReady();
    await convertSuccessfully();

    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Salin/ }));
    });
    expect(writeText).toHaveBeenCalledWith(MARKDOWN);
    expect(screen.getByRole('button', { name: /Tersalin/ })).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(2_000);
    });
    expect(screen.getByRole('button', { name: /Salin/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Tersalin/ })).toBeNull();
  });

  it('shows a failure message and keeps the result when copying fails (Req 13.10)', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'));
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    stubFetch();
    await renderReady();
    await convertSuccessfully();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Salin/ }));
    });
    expect(screen.getByRole('alert').textContent).toMatch(/Gagal menyalin/);
    expect(screen.queryByRole('button', { name: /Tersalin/ })).toBeNull();
    expect(screen.getByTestId('markdown-preview')).toBeTruthy();
  });

  it('downloads document.md with content identical to the markdown (Req 13.11)', async () => {
    let captured: Blob | null = null;
    (URL as any).createObjectURL = vi.fn((b: Blob) => {
      captured = b;
      return 'blob:mock';
    });
    (URL as any).revokeObjectURL = vi.fn();
    let downloadName = '';
    anchorClick.mockImplementation(function (this: HTMLAnchorElement) {
      downloadName = this.download;
    });
    stubFetch();
    await renderReady();
    await convertSuccessfully();

    fireEvent.click(screen.getByRole('button', { name: /Unduh \.md/ }));
    expect(anchorClick).toHaveBeenCalledTimes(1);
    expect(downloadName).toBe('document.md');
    expect(captured).not.toBeNull();
    const bytes = await new Promise<Uint8Array>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(new Uint8Array(reader.result as ArrayBuffer));
      reader.onerror = () => reject(reader.error);
      reader.readAsArrayBuffer(captured as unknown as Blob);
    });
    expect(Array.from(bytes)).toEqual(Array.from(new TextEncoder().encode(MARKDOWN)));
  });

  const ERROR_CASES: Array<[string, number, string | undefined]> = [
    ['413', 413, undefined],
    ['415', 415, undefined],
    ['422', 422, 'corrupt_pdf'],
    ['503', 503, undefined],
    ['504', 504, undefined],
  ];

  it.each(ERROR_CASES)('shows a distinct message for HTTP %s, keeps the file and re-enables the button (Req 13.12)', async (_l, status, code) => {
    stubFetch(() => json(code ? { code } : {}, status));
    await renderReady();
    selectFiles([pdfFile()]);
    fireEvent.click(convertButton());
    const expected = pdfErrorMessage(status, code);
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe(expected));
    expect(screen.getByText('dok.pdf')).toBeTruthy();
    expect(convertButton().disabled).toBe(false);
    expect(screen.queryByTestId('markdown-preview')).toBeNull();
  });

  it('shows a network error message and keeps the file (Req 13.12)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string) =>
        url === LIMITS_URL ? Promise.resolve(json({ max_bytes: MAX_BYTES })) : Promise.reject(new TypeError('net')),
      ),
    );
    await renderReady();
    selectFiles([pdfFile()]);
    fireEvent.click(convertButton());
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe(pdfErrorMessage('network')));
    expect(screen.getByText('dok.pdf')).toBeTruthy();
    expect(convertButton().disabled).toBe(false);
  });

  it('uses a different message for each error status and for network errors (Req 13.12)', () => {
    const messages = [
      pdfErrorMessage(401),
      pdfErrorMessage(413),
      pdfErrorMessage(415),
      pdfErrorMessage(422, 'corrupt_pdf'),
      pdfErrorMessage(503),
      pdfErrorMessage(504),
      pdfErrorMessage('network'),
    ];
    expect(new Set(messages).size).toBe(messages.length);
  });

  it('clears a previous result when a later conversion fails (Req 13.12)', async () => {
    let fail = false;
    stubFetch(() => (fail ? json({}, 503) : json(RESULT)));
    await renderReady();
    await convertSuccessfully();
    fail = true;
    fireEvent.click(convertButton());
    await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy());
    expect(screen.queryByTestId('markdown-preview')).toBeNull();
    expect(screen.queryByText('Halaman:')).toBeNull();
  });

  it('shows the scanned-document / OCR message for no_text_layer (Req 13.13)', async () => {
    stubFetch(() => json({ code: 'no_text_layer' }, 422));
    await renderReady();
    selectFiles([pdfFile()]);
    fireEvent.click(convertButton());
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/pindai/));
    expect(screen.getByRole('alert').textContent).toMatch(/OCR belum didukung/);
    expect(screen.getByRole('alert').textContent).not.toBe(pdfErrorMessage(422, 'corrupt_pdf'));
  });

  it('redirects to /login on HTTP 401 (Req 13.12, 13.14)', async () => {
    stubFetch(() => json({}, 401));
    await renderReady();
    selectFiles([pdfFile()]);
    fireEvent.click(convertButton());
    await waitFor(() => expect(hrefSetter).toHaveBeenCalledWith('/login'));
  });

  it('aborts the in-flight request and leaves nothing behind on unmount (Req 13.15)', async () => {
    let signal: AbortSignal | undefined;
    const fetchMock = vi.fn((url: string, init?: RequestInit) => {
      if (url === LIMITS_URL) return Promise.resolve(json({ max_bytes: MAX_BYTES }));
      signal = init?.signal ?? undefined;
      return new Promise<Response>(() => {});
    });
    vi.stubGlobal('fetch', fetchMock);
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    const { unmount } = await renderReady();
    selectFiles([pdfFile('rahasia.pdf')]);
    fireEvent.click(convertButton());
    await waitFor(() => expect(signal).toBeDefined());
    expect(signal!.aborted).toBe(false);

    unmount();
    expect(signal!.aborted).toBe(true);
    expect(screen.queryByText('rahasia.pdf')).toBeNull();
    expect(document.body.textContent).not.toContain('rahasia.pdf');
    expect(errorSpy).not.toHaveBeenCalled();

    // Pemasangan ulang mulai dari keadaan kosong.
    render(<PdfToMarkdown />);
    expect(screen.queryByText('rahasia.pdf')).toBeNull();
    expect(screen.queryByTestId('markdown-preview')).toBeNull();
  });

  it('does not write to localStorage, sessionStorage or cookies (Req 13.16)', async () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    const removeItem = vi.spyOn(Storage.prototype, 'removeItem');
    const cookieSet = vi.spyOn(Document.prototype, 'cookie', 'set');
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
    (URL as any).createObjectURL = vi.fn(() => 'blob:mock');
    (URL as any).revokeObjectURL = vi.fn();
    stubFetch();

    const { unmount } = await renderReady();
    await convertSuccessfully();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Salin/ }));
    });
    fireEvent.click(screen.getByRole('button', { name: /Unduh \.md/ }));
    unmount();

    expect(setItem).not.toHaveBeenCalled();
    expect(removeItem).not.toHaveBeenCalled();
    expect(cookieSet).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  });
});

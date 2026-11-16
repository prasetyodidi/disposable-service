// @vitest-environment jsdom
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import fc from 'fast-check';
import { PdfToMarkdown } from './PdfToMarkdown';

// Navbar dan kerangka halaman tidak relevan untuk tampilan warnings.
vi.mock('./PageShell', () => ({
  PageShell: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

function jsonResponse(body: unknown): Response {
  return { ok: true, status: 200, json: async () => body } as unknown as Response;
}

const specialStrings = fc.oneof(
  fc.string(),
  fc.constantFrom('<b>x</b>', '<script>alert(1)</script>', '& &amp; "q" \'s\'', '  spasi  ', '', '- [x] `kode` **tebal**'),
  fc.array(fc.constantFrom('<', '>', '&', '"', "'", '\\', '*', '`', ' ', 'a', '\u00e9', '\u4e2d'), { maxLength: 20 }).map((a) =>
    a.join(''),
  ),
);

async function renderResult(warnings: string[]) {
  const fetchMock = vi.fn(async (url: string) => {
    if (url.endsWith('/limits')) return jsonResponse({ max_bytes: 1_000_000, max_pages: 300 });
    return jsonResponse({ markdown: 'Halo', pages: 1, headings: 0, warnings });
  });
  vi.stubGlobal('fetch', fetchMock);
  render(<PdfToMarkdown />);
  await waitFor(() => expect(screen.getByText(/maksimum/)).toBeTruthy());
  const input = screen.getByLabelText('Pilih file PDF') as HTMLInputElement;
  const file = new File([new Uint8Array(10)], 'a.pdf', { type: 'application/pdf' });
  fireEvent.change(input, { target: { files: [file] } });
  fireEvent.click(screen.getByRole('button', { name: /Konversi ke Markdown/ }));
  await waitFor(() => expect(screen.getByTestId('markdown-preview')).toBeTruthy());
}

describe('PdfToMarkdown warnings (property)', () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  // Feature: pdf-to-markdown, Property 38: Warnings ditampilkan sesuai urutan
  // **Validates: Requirements 13.9**
  it('renders exactly one <li> per warning in order, and no list when empty', async () => {
    await fc.assert(
      fc.asyncProperty(fc.array(specialStrings, { maxLength: 8 }), async (warnings) => {
        await renderResult(warnings);
        const list = screen.queryByLabelText('Peringatan');
        if (warnings.length === 0) {
          expect(list).toBeNull();
        } else {
          expect(list).not.toBeNull();
          const items = Array.from(list!.querySelectorAll('li')).map((li) => li.textContent);
          expect(items).toEqual(warnings);
          expect(list!.children.length).toBe(warnings.length);
        }
        cleanup();
        vi.unstubAllGlobals();
      }),
      { numRuns: 100 },
    );
  }, 120_000);
});

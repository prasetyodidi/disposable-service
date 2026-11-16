// @vitest-environment jsdom
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import fc from 'fast-check';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { PdfToMarkdown } from '../components/PdfToMarkdown';

// Navbar tidak relevan untuk pratinjau.
vi.mock('../components/Navbar', () => ({ Navbar: () => null }));

// Elemen yang boleh ada: h1-h6, p, ul, ol, li, pre, code, hr, strong, em, ditambah pembungkus
// kontainer (div/span) yang dipakai komponen untuk heading ARIA dan penanda daftar.
const ALLOWED = new Set([
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'p', 'ul', 'ol', 'li', 'pre', 'code', 'hr', 'strong', 'em',
  'div', 'span',
]);
const FORBIDDEN = ['script', 'img', 'iframe', 'a', 'style'];

// Fragmen HTML berbahaya. Tanpa karakter yang bermakna di Markdown pada awal/akhir (`*`, `\`, spasi tepi).
const fragmentArb = fc.constantFrom(
  '<script>alert(1)</script>',
  '<img src=x onerror=alert(1)>',
  '<iframe src="https://evil.example"></iframe>',
  '<a href="javascript:alert(1)">klik</a>',
  '<style>body{display:none}</style>',
  '<div onclick="x()">halo</div>',
  '<svg onload=alert(1)>',
  '&lt;b&gt;tebal&lt;/b&gt;',
  '&amp;#x3C;script&amp;#x3E;',
  '<b onmouseover="x">y</b>',
);

// Baris Markdown acak (tanpa newline) yang boleh membentuk heading, daftar, kode, gaya, dll.
const noiseArb = fc.oneof(
  fc.string({ maxLength: 30 }),
  fc.constantFrom('# Judul', '- butir', '1. satu', '```', '---', '**tebal** dan *miring*', '    kode'),
);

// Dokumen: paragraf-paragraf dipisah baris kosong; fragmen berdiri sendiri dalam satu paragraf.
const docArb = fc
  .array(
    fc.oneof(
      fragmentArb.map((f) => ({ frag: f as string | null, text: f })),
      noiseArb.map((t) => ({ frag: null as string | null, text: t })),
    ),
    { minLength: 1, maxLength: 8 },
  )
  .map((parts) => ({
    markdown: parts.map((p) => p.text).join('\n\n'),
    fragments: parts.flatMap((p) => (p.frag ? [p.frag] : [])),
  }));

// Feature: pdf-to-markdown, Property 36: Pratinjau Markdown tidak pernah menyuntikkan HTML
describe('Pratinjau Markdown (PdfToMarkdown)', () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('Property 36: hanya elemen yang diizinkan, tanpa atribut on*, dan teks fragmen muncul di textContent', async () => {
    // **Validates: Requirements 13.18**
    let nextMarkdown = '';
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string, init?: RequestInit) => {
        const body =
          init?.method === 'POST'
            ? { markdown: nextMarkdown, pages: 1, headings: 0, warnings: [] }
            : { max_bytes: 1_000_000, max_pages: 300 };
        return new Response(JSON.stringify(body), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }),
    );

    render(React.createElement(PdfToMarkdown));
    await screen.findByText(/maksimum/);
    const input = screen.getByLabelText('Pilih file PDF') as HTMLInputElement;
    const pdf = new File([new Uint8Array(8)], 'a.pdf', { type: 'application/pdf' });
    fireEvent.change(input, { target: { files: [pdf] } });
    const button = await screen.findByRole('button', { name: /Konversi ke Markdown/ });

    await fc.assert(
      fc.asyncProperty(docArb, async ({ markdown, fragments }) => {
        nextMarkdown = markdown;
        await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
        fireEvent.click(button);
        const preview = await screen.findByTestId('markdown-preview');
        await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));

        const all = [preview, ...Array.from(preview.querySelectorAll('*'))];
        for (const el of all) {
          const tag = el.tagName.toLowerCase();
          if (el !== preview) expect(ALLOWED.has(tag), `elemen tak diizinkan: ${tag}`).toBe(true);
          expect(FORBIDDEN).not.toContain(tag);
          for (const attr of Array.from(el.attributes)) {
            expect(attr.name.toLowerCase().startsWith('on'), `atribut ${attr.name}`).toBe(false);
          }
        }
        for (const f of fragments) {
          expect(preview.textContent ?? '').toContain(f);
        }
      }),
      { numRuns: 100 },
    );
  }, 120_000);
});

import { describe, it, expect, vi, afterEach } from 'vitest';
import fc from 'fast-check';
import { markdownToBlob, saveAs } from './downloadText';

async function readUtf8(blob: Blob): Promise<string> {
  // ignoreBOM: true mempertahankan BOM di awal teks (Blob.text() membuangnya).
  return new TextDecoder('utf-8', { ignoreBOM: true, fatal: true }).decode(
    await blob.arrayBuffer(),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('downloadText', () => {
  // Feature: pdf-to-markdown, Property 37: Berkas unduhan identik dengan hasil
  // **Validates: Requirements 13.11**
  it('Property 37: isi Blob UTF-8 sama persis dengan md dan nama berkas document.md', async () => {
    const names: string[] = [];
    const a = {
      href: '',
      download: '',
      style: { display: '' },
      click() {
        names.push(this.download);
      },
    };
    vi.stubGlobal('document', {
      createElement: () => a,
      body: { appendChild: () => undefined, removeChild: () => undefined },
    });
    vi.stubGlobal('URL', {
      createObjectURL: () => 'blob:test',
      revokeObjectURL: () => undefined,
    });

    await fc.assert(
      fc.asyncProperty(
        fc.oneof(
          fc.string({ unit: 'grapheme', maxLength: 500 }),
          fc.string({ unit: 'binary', maxLength: 200 }).map((s) => s.toWellFormed()),
          fc.constant(''),
          fc.string({ maxLength: 50 }).map((s) => `\uFEFF${s}\r\n\0`),
        ),
        async (md) => {
          const blob = markdownToBlob(md);
          expect(blob.type).toBe('text/markdown;charset=utf-8');
          expect(await readUtf8(blob)).toBe(md);

          saveAs(blob, 'document.md');
          expect(names[names.length - 1]).toBe('document.md');
        },
      ),
      { numRuns: 100 },
    );
  });
});

// @vitest-environment jsdom
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MetadataScrubber } from './MetadataScrubber';

// Navbar tidak relevan untuk perilaku Scrubber.
vi.mock('./Navbar', () => ({ Navbar: () => null }));

function b64url(obj: unknown): string {
  return btoa(JSON.stringify(obj)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function makeFile(name: string, bytes: number): File {
  return new File([new Uint8Array(bytes)], name, { type: 'image/jpeg' });
}

function selectFiles(files: File[]) {
  const input = screen.getByLabelText('Select file to scrub') as HTMLInputElement;
  fireEvent.change(input, { target: { files } });
}

describe('MetadataScrubber', () => {
  let clickSpy: ReturnType<typeof vi.spyOn>;
  const createUrl = vi.fn(() => 'blob:mock');
  const revokeUrl = vi.fn();

  beforeEach(() => {
    (URL as any).createObjectURL = createUrl;
    (URL as any).revokeObjectURL = revokeUrl;
    clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    createUrl.mockClear();
    revokeUrl.mockClear();
  });

  it('shows name and size before upload (Req 4.3)', () => {
    render(<MetadataScrubber />);
    expect((screen.getByRole('button', { name: /Scrub & Download/ }) as HTMLButtonElement).disabled).toBe(true);
    selectFiles([makeFile('photo.jpg', 2048)]);
    expect(screen.getByText('photo.jpg')).toBeTruthy();
    expect(screen.getByText('2 KB')).toBeTruthy();
    expect((screen.getByRole('button', { name: /Scrub & Download/ }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('uses only the first file and shows a notice (Req 4.4)', () => {
    render(<MetadataScrubber />);
    selectFiles([makeFile('first.jpg', 10), makeFile('second.png', 20)]);
    expect(screen.getByText('first.jpg')).toBeTruthy();
    expect(screen.queryByText('second.png')).toBeNull();
    expect(screen.getByRole('status').textContent).toMatch(/first file/i);
  });

  it('shows the report and triggers a download on success (Req 4.5)', async () => {
    const header = b64url({ file_kind: 'jpeg', removed: [{ category: 'gps', count: 3 }] });
    const fetchMock = vi.fn().mockResolvedValue(
      new Response('clean', {
        status: 200,
        headers: {
          'content-disposition': 'attachment; filename="clean.jpg"',
          'x-scrub-report': header,
        },
      }),
    );
    vi.stubGlobal('fetch', fetchMock);

    render(<MetadataScrubber />);
    selectFiles([makeFile('photo.jpg', 100)]);
    fireEvent.click(screen.getByRole('button', { name: /Scrub & Download/ }));

    await waitFor(() => expect(screen.getByText('Scrub Report')).toBeTruthy());
    expect(screen.getByText('gps')).toBeTruthy();
    expect(screen.getByText('3')).toBeTruthy();
    expect(screen.getByText(/Clean file downloaded \(JPEG\)/)).toBeTruthy();
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/apps/scrubber?mode=direct',
      expect.objectContaining({ method: 'POST' }),
    );
    expect(createUrl).toHaveBeenCalledTimes(1);
    expect(clickSpy).toHaveBeenCalledTimes(1);
    expect(revokeUrl).toHaveBeenCalledWith('blob:mock');
  });

  it('keeps the selected file and re-enables controls on error (Req 4.6)', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('network')));

    render(<MetadataScrubber />);
    selectFiles([makeFile('photo.jpg', 100)]);
    fireEvent.click(screen.getByRole('button', { name: /Scrub & Download/ }));

    await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy());
    expect(screen.getByText('photo.jpg')).toBeTruthy();
    expect((screen.getByRole('button', { name: /Scrub & Download/ }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByLabelText('Select file to scrub') as HTMLInputElement).disabled).toBe(false);
    expect(clickSpy).not.toHaveBeenCalled();
  });

  it('shows server error message and keeps the file on non-OK response (Req 4.6)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ message: 'Unsupported file type' }), {
          status: 415,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );

    render(<MetadataScrubber />);
    selectFiles([makeFile('doc.pdf', 100)]);
    fireEvent.click(screen.getByRole('button', { name: /Scrub & Download/ }));

    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Unsupported file type'));
    expect(screen.getByText('doc.pdf')).toBeTruthy();
    expect((screen.getByRole('button', { name: /Scrub & Download/ }) as HTMLButtonElement).disabled).toBe(false);
  });
});

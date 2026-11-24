// @vitest-environment jsdom
// Validates: Requirements 7.2, 7.3, 7.5, 7.6, 7.7, 7.8, 7.9
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

// Navbar menyentuh ThemeToggle (localStorage); di luar cakupan tes ini.
vi.mock('./Navbar', () => ({ Navbar: () => null }));

import { RedactorPage } from './RedactorPage';

const MAX = 1_048_576;

function mockFetchOk(body: unknown) {
  const fn = vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => body });
  vi.stubGlobal('fetch', fn);
  return fn;
}

function setClipboard(writeText: (t: string) => Promise<void>) {
  Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
}

async function openRedactorAndRedact(text = 'secret a@b.co') {
  fireEvent.click(screen.getByRole('tab', { name: 'Log & .env Redactor' }));
  fireEvent.change(screen.getByLabelText('Text, log or .env'), { target: { value: text } });
  await act(async () => {
    fireEvent.click(screen.getByRole('button', { name: /^Redact$/ }));
  });
}

describe('RedactorPage', () => {
  let storageSet: ReturnType<typeof vi.spyOn>;
  let storageGet: ReturnType<typeof vi.spyOn>;
  let cookieSet: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    storageSet = vi.spyOn(Storage.prototype, 'setItem');
    storageGet = vi.spyOn(Storage.prototype, 'getItem');
    cookieSet = vi.spyOn(Document.prototype, 'cookie', 'set');
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('renders exactly two tabs with JWT Inspector selected by default', () => {
    render(<RedactorPage />);
    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual(['JWT Inspector', 'Log & .env Redactor']);
    expect(tabs[0].getAttribute('aria-selected')).toBe('true');
    expect(tabs[1].getAttribute('aria-selected')).toBe('false');
    expect(document.getElementById('panel-jwt')!.hidden).toBe(false);
    expect(document.getElementById('panel-redactor')!.hidden).toBe(true);
  });

  it('switches tabs without a page reload', () => {
    render(<RedactorPage />);
    fireEvent.click(screen.getByRole('tab', { name: 'Log & .env Redactor' }));
    expect(screen.getByRole('tab', { name: 'Log & .env Redactor' }).getAttribute('aria-selected')).toBe('true');
    expect(document.getElementById('panel-jwt')!.hidden).toBe(true);
    expect(document.getElementById('panel-redactor')!.hidden).toBe(false);
    fireEvent.click(screen.getByRole('tab', { name: 'JWT Inspector' }));
    expect(document.getElementById('panel-jwt')!.hidden).toBe(false);
  });

  it('clears sensitive state on pagehide', () => {
    render(<RedactorPage />);
    const token = screen.getByLabelText('JWT token') as HTMLTextAreaElement;
    fireEvent.change(token, { target: { value: 'abc.def.ghi' } });
    expect(token.value).toBe('abc.def.ghi');
    act(() => {
      window.dispatchEvent(new Event('pagehide'));
    });
    expect((screen.getByLabelText('JWT token') as HTMLTextAreaElement).value).toBe('');
  });

  it('clears state and aborts in-flight requests on unmount', async () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      'fetch',
      vi.fn((_url: string, init: RequestInit) => {
        signal = init.signal as AbortSignal;
        return new Promise(() => {});
      }),
    );
    const { unmount } = render(<RedactorPage />);
    fireEvent.change(screen.getByLabelText('JWT token'), { target: { value: 'a.b.c' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Inspect token/ }));
    });
    expect(signal?.aborted).toBe(false);
    unmount();
    expect(signal?.aborted).toBe(true);

    render(<RedactorPage />);
    expect((screen.getByLabelText('JWT token') as HTMLTextAreaElement).value).toBe('');
  });

  it('never touches localStorage, sessionStorage or document.cookie', async () => {
    mockFetchOk({ text: 'x', counts: {} });
    const { unmount } = render(<RedactorPage />);
    fireEvent.change(screen.getByLabelText('JWT token'), { target: { value: 'a.b.c' } });
    await openRedactorAndRedact();
    act(() => {
      window.dispatchEvent(new Event('beforeunload'));
    });
    unmount();
    expect(storageSet).not.toHaveBeenCalled();
    expect(storageGet).not.toHaveBeenCalled();
    expect(cookieSet).not.toHaveBeenCalled();
  });

  it('copies the redacted result and shows a confirmation', async () => {
    mockFetchOk({ text: '[REDACTED]', counts: { email: 1 } });
    const writeText = vi.fn().mockResolvedValue(undefined);
    setClipboard(writeText);
    render(<RedactorPage />);
    await openRedactorAndRedact();
    await screen.findByTestId('redactor-output');
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Copy/ }));
    });
    expect(writeText).toHaveBeenCalledWith('[REDACTED]');
    expect(await screen.findByText('Copied to clipboard')).toBeTruthy();
  });

  it('shows an error when the clipboard write fails', async () => {
    mockFetchOk({ text: '[REDACTED]', counts: {} });
    setClipboard(vi.fn().mockRejectedValue(new Error('denied')));
    render(<RedactorPage />);
    await openRedactorAndRedact();
    await screen.findByTestId('redactor-output');
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Copy/ }));
    });
    expect((await screen.findByText(/Copy failed/)).getAttribute('role')).toBe('alert');
    expect(screen.queryByText('Copied to clipboard')).toBeNull();
  });

  it('rejects input over 1,048,576 characters without sending and keeps the text', async () => {
    const fetchMock = mockFetchOk({ text: '', counts: {} });
    render(<RedactorPage />);
    fireEvent.click(screen.getByRole('tab', { name: 'Log & .env Redactor' }));
    const big = 'a'.repeat(MAX + 1);
    const area = screen.getByLabelText('Text, log or .env') as HTMLTextAreaElement;
    fireEvent.change(area, { target: { value: big } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /^Redact$/ }));
    });
    expect(fetchMock).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy());
    expect(area.value.length).toBe(MAX + 1);
  });

  it('accepts input of exactly 1,048,576 characters', async () => {
    const fetchMock = mockFetchOk({ text: 'ok', counts: {} });
    render(<RedactorPage />);
    await openRedactorAndRedact('a'.repeat(MAX));
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

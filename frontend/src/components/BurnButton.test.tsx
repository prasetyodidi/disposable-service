// @vitest-environment jsdom
import React from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { BurnButton } from './BurnButton';
import type { AuditReport } from '../types/privacy';

// Validates: Requirements 9.2, 9.3, 9.4, 9.6, 9.7, 9.8, 11.7, 11.8, 11.9, 11.10

class MockEventSource {
  static CLOSED = 2;
  readyState = 0;
  onerror: (() => void) | null = null;
  constructor(public url: string) {}
  addEventListener() {}
  removeEventListener() {}
  close() {
    this.readyState = 2;
  }
}

function makeReport(overrides: Partial<AuditReport> = {}): AuditReport {
  return {
    verified: true,
    incomplete: false,
    before: [{ category: 'sessions', entries: 2, bytes: 100 }],
    after: [{ category: 'sessions', entries: 0, bytes: 0 }],
    total_bytes_before: 100,
    remaining_bytes: 0,
    remaining_categories: [],
    removed_items: 2,
    failed_items: [],
    duration_ms: 12,
    timestamp_utc: '2025-01-01T00:00:00Z',
    warnings: [],
    ...overrides,
  };
}

function okResponse(report: AuditReport) {
  return { ok: true, status: 200, json: async () => report } as Response;
}

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('EventSource', MockEventSource);
  fetchMock = vi.fn();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const burnBtn = () => screen.getByRole('button', { name: /Burn Workspace/ });
const flush = (ms = 0) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

describe('BurnButton confirmation dialog', () => {
  it('Setuju sends exactly one POST /api/burn', async () => {
    fetchMock.mockResolvedValue(okResponse(makeReport()));
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    expect(screen.getByRole('alertdialog')).toBeTruthy();
    expect(fetchMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Setuju' }));
    await flush();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/burn');
    expect(fetchMock.mock.calls[0][1].method).toBe('POST');
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect((burnBtn() as HTMLButtonElement).disabled).toBe(true);
  });

  it('Batal sends no request and closes the dialog', async () => {
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.click(screen.getByRole('button', { name: 'Batal' }));
    await flush(1000);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('Escape sends no request and closes the dialog', async () => {
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.keyDown(window, { key: 'Escape' });
    await flush(1000);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
});

describe('BurnButton Demo mode', () => {
  it('counts 3-2-1 then sends exactly one POST', async () => {
    fetchMock.mockResolvedValue(okResponse(makeReport()));
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.click(screen.getByLabelText(/Demo mode/));
    fireEvent.click(screen.getByRole('button', { name: 'Setuju' }));

    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.getByText(/Burn dalam 3/)).toBeTruthy();
    await flush(1000);
    expect(screen.getByText(/Burn dalam 2/)).toBeTruthy();
    await flush(1000);
    expect(screen.getByText(/Burn dalam 1/)).toBeTruthy();
    expect(fetchMock).not.toHaveBeenCalled();
    await flush(1000);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/burn');
  });

  it('Batal during countdown sends nothing', async () => {
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.click(screen.getByLabelText(/Demo mode/));
    fireEvent.click(screen.getByRole('button', { name: 'Setuju' }));
    await flush(1000);
    fireEvent.click(screen.getByRole('button', { name: 'Batal' }));
    await flush(10_000);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByText(/Burn dalam/)).toBeNull();
    expect((burnBtn() as HTMLButtonElement).disabled).toBe(false);
  });
});

describe('BurnButton failure handling', () => {
  it('re-enables the button and shows an error when the request fails', async () => {
    fetchMock.mockRejectedValue(new Error('network'));
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.click(screen.getByRole('button', { name: 'Setuju' }));
    await flush();
    expect(screen.getByRole('alert').textContent).toMatch(/Burn gagal/);
    expect((burnBtn() as HTMLButtonElement).disabled).toBe(false);
  });

  it('re-enables the button when the server responds non-OK', async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 500, json: async () => ({}) } as Response);
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.click(screen.getByRole('button', { name: 'Setuju' }));
    await flush();
    expect(screen.getByRole('alert')).toBeTruthy();
    expect((burnBtn() as HTMLButtonElement).disabled).toBe(false);
  });

  it('aborts after 10s timeout and re-enables the button', async () => {
    fetchMock.mockImplementation(
      (_url: string, init: RequestInit) =>
        new Promise((_resolve, reject) => {
          init.signal?.addEventListener('abort', () => reject(new Error('aborted')));
        }),
    );
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.click(screen.getByRole('button', { name: 'Setuju' }));
    await flush(9_000);
    expect((burnBtn() as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole('alert')).toBeNull();
    await flush(1_500);
    expect(screen.getByRole('alert').textContent).toMatch(/Burn gagal/);
    expect((burnBtn() as HTMLButtonElement).disabled).toBe(false);
  });
});

describe('BurnButton audit screen', () => {
  async function burnWith(report: AuditReport) {
    fetchMock.mockResolvedValue(okResponse(report));
    render(<BurnButton />);
    fireEvent.click(burnBtn());
    fireEvent.click(screen.getByRole('button', { name: 'Setuju' }));
    await flush();
    // animasi hitung mundur 3 detik lalu audit
    await flush(3_000);
  }

  it('verified=true and 0 remaining bytes shows "0 bytes remaining"', async () => {
    await burnWith(makeReport());
    expect(screen.getByText('0 bytes remaining')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('verified=false does not show "0 bytes remaining" and shows a warning', async () => {
    await burnWith(makeReport({ verified: false, remaining_bytes: 0, incomplete: true }));
    expect(screen.queryByText('0 bytes remaining')).toBeNull();
    expect(screen.getByRole('alert').textContent).toMatch(/tidak berhasil membuktikan/);
  });

  it('verified=true with remaining bytes shows remaining data, not zero', async () => {
    await burnWith(
      makeReport({
        remaining_bytes: 42,
        remaining_categories: [{ category: 'downloads', entries: 1, bytes: 42 }],
      }),
    );
    expect(screen.queryByText('0 bytes remaining')).toBeNull();
    expect(screen.getByRole('alert').textContent).toMatch(/42 byte/);
  });

  it('states the Managed_Storage verification scope', async () => {
    await burnWith(makeReport());
    expect(screen.getByText(/Managed_Storage/, { selector: 'p' }).textContent).toMatch(
      /bukan memori proses server secara keseluruhan/,
    );
  });
});

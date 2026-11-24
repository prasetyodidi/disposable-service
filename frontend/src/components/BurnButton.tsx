import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Flame } from 'lucide-react';
import type { AuditReport } from '../types/privacy';
import { BurnConfirmDialog } from './BurnConfirmDialog';
import { BurnOverlay } from './BurnOverlay';

const BURN_TIMEOUT_MS = 10_000;
const DEMO_COUNTDOWN_SECONDS = 3;

export const BurnButton: React.FC = () => {
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [countdown, setCountdown] = useState<number | null>(null);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<AuditReport | null>(null);
  const inFlight = useRef(false);

  const sendBurn = useCallback(async () => {
    if (inFlight.current) return; // tepat satu POST per aksi
    inFlight.current = true;
    setSending(true);
    setError(null);
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), BURN_TIMEOUT_MS);
    try {
      const res = await fetch('/api/burn', { method: 'POST', signal: controller.signal });
      if (!res.ok) throw new Error(`status ${res.status}`);
      const data = (await res.json()) as AuditReport;
      setReport((prev) => prev ?? data);
      // Tombol tetap nonaktif: server sedang berhenti.
    } catch {
      setError('Burn gagal. Workspace tidak diubah, silakan coba lagi.');
      inFlight.current = false;
      setSending(false);
    } finally {
      window.clearTimeout(timer);
    }
  }, []);

  const handleClick = () => {
    if (sending || countdown !== null) return;
    setError(null);
    setConfirmOpen(true);
  };

  const confirm = useCallback(
    (demo: boolean) => {
      setConfirmOpen(false);
      if (demo) setCountdown(DEMO_COUNTDOWN_SECONDS);
      else void sendBurn();
    },
    [sendBurn],
  );

  const cancelConfirm = useCallback(() => setConfirmOpen(false), []);

  // Hitung mundur Demo mode (dipilih di dialog konfirmasi): 3, 2, 1, lalu kirim saat mencapai 0.
  useEffect(() => {
    if (countdown === null) return;
    if (countdown === 0) {
      setCountdown(null);
      void sendBurn();
      return;
    }
    const id = window.setTimeout(() => setCountdown((c) => (c === null ? null : c - 1)), 1000);
    return () => window.clearTimeout(id);
  }, [countdown, sendBurn]);

  const handleReport = useCallback((r: AuditReport) => setReport((prev) => prev ?? r), []);

  return (
    <>
      <div className="flex items-center gap-2">
        {countdown !== null ? (
          <div className="flex items-center gap-2" role="status" aria-live="assertive">
            <span className="text-sm font-bold text-airbnb-error whitespace-nowrap">Burn dalam {countdown}…</span>
            <button
              type="button"
              onClick={() => setCountdown(null)}
              className="px-3 py-1.5 rounded-sm text-xs font-semibold border border-hairline-soft dark:border-dark-border text-ink dark:text-dark-ink bg-white dark:bg-dark-surface"
            >
              Batal
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={handleClick}
            disabled={sending}
            aria-label="Burn Workspace: hapus seluruh data dan matikan container"
            className="inline-flex items-center justify-center gap-1.5 h-10 min-w-10 px-3 rounded-sm text-sm font-bold text-white bg-airbnb-error hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-opacity"
          >
            <Flame size={16} aria-hidden="true" />
            <span className="hidden sm:inline whitespace-nowrap">Burn Workspace</span>
          </button>
        )}
      </div>

      {error && (
        <div
          role="alert"
          className="fixed top-4 right-4 z-[90] max-w-xs bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-sm shadow-airbnb"
        >
          {error}
        </div>
      )}

      {confirmOpen && <BurnConfirmDialog onConfirm={confirm} onCancel={cancelConfirm} />}
      <BurnOverlay report={report} onReport={handleReport} />
    </>
  );
};

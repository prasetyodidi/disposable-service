import React, { useEffect, useRef, useState } from 'react';
import type { AuditReport } from '../types/privacy';
import { AuditScreen } from './AuditScreen';

/** Total durasi animasi hitung mundur (detik); harus di antara 1 dan 10. */
export const BURN_ANIMATION_SECONDS = 3;

interface BurnOverlayProps {
  /** Laporan dari POST /api/burn (atau dari event SSE yang diteruskan ke atas). */
  report: AuditReport | null;
  /** Dipanggil saat event `burn` diterima lewat EventSource (burn dipicu MCP). */
  onReport: (report: AuditReport) => void;
}

export const BurnOverlay: React.FC<BurnOverlayProps> = ({ report, onReport }) => {
  const [remaining, setRemaining] = useState(BURN_ANIMATION_SECONDS);
  const [done, setDone] = useState(false);
  const onReportRef = useRef(onReport);
  onReportRef.current = onReport;

  // Berlangganan burn yang dipicu MCP.
  useEffect(() => {
    if (typeof EventSource === 'undefined') return;
    const es = new EventSource('/api/burn/events');
    const handler = (ev: MessageEvent) => {
      try {
        onReportRef.current(JSON.parse(ev.data) as AuditReport);
      } catch {
        /* abaikan event rusak */
      }
    };
    es.addEventListener('burn', handler as EventListener);
    // Server mati setelah burn; hentikan reconnect otomatis saat koneksi putus.
    es.onerror = () => {
      if (es.readyState === EventSource.CLOSED) es.close();
    };
    return () => {
      es.removeEventListener('burn', handler as EventListener);
      es.close();
    };
  }, []);

  // Animasi hitung mundur dimulai begitu laporan ada.
  useEffect(() => {
    if (!report) return;
    setRemaining(BURN_ANIMATION_SECONDS);
    setDone(false);
    const id = window.setInterval(() => {
      setRemaining((r) => {
        if (r <= 1) {
          window.clearInterval(id);
          setDone(true);
          return 0;
        }
        return r - 1;
      });
    }, 1000);
    return () => window.clearInterval(id);
  }, [report]);

  if (!report) return null;
  if (done) return <AuditScreen report={report} />;

  return (
    <div
      className="fixed inset-0 z-[110] flex flex-col items-center justify-center bg-ink text-white"
      role="status"
      aria-live="polite"
    >
      <p className="text-sm uppercase tracking-widest mb-4">Menghancurkan Workspace</p>
      <p className="text-8xl font-bold text-airbnb-error animate-pulse" aria-label={`${remaining} detik`}>
        {remaining}
      </p>
    </div>
  );
};

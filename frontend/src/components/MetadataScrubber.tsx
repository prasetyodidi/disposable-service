import React, { useEffect, useRef, useState } from 'react';
import { Upload, Download, CheckCircle2, AlertCircle, FileText, Trash2, ShieldCheck } from 'lucide-react';
import { PageShell } from './PageShell';
import { formatFileSize } from '../lib/formatFileSize';
import { pickFirstFile } from '../lib/pickFirstFile';
import type { ScrubReport } from '../types/privacy';
import { PRIMARY_BUTTON } from '../lib/ui';

const TIMEOUT_MS = 60_000;
const GENERIC_ERROR = 'Scrubbing failed. The server did not respond or the connection was lost.';

/** Decode header X-Scrub-Report (JSON, base64url) menjadi ScrubReport. */
function decodeScrubReport(header: string | null): ScrubReport | null {
  if (!header) return null;
  try {
    const b64 = header.replace(/-/g, '+').replace(/_/g, '/');
    const padded = b64 + '='.repeat((4 - (b64.length % 4)) % 4);
    const bin = atob(padded);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    return JSON.parse(new TextDecoder().decode(bytes)) as ScrubReport;
  } catch {
    return null;
  }
}

function filenameFromDisposition(disposition: string | null, fallback: string): string {
  const match = disposition?.match(/filename="?([^";]+)"?/);
  return match?.[1] || fallback;
}

export const MetadataScrubber: React.FC = () => {
  const [file, setFile] = useState<File | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ScrubReport | null>(null);
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => () => abortRef.current?.abort(), []);

  const selectFiles = (files: FileList | null) => {
    const { file: first, multiple } = pickFirstFile<File>(files);
    if (!first) return;
    setFile(first);
    setReport(null);
    setError(null);
    setNotice(multiple ? 'Only one file is processed per scrub. The first file was selected.' : null);
  };

  const handleScrub = async () => {
    if (!file || busy) return;
    setBusy(true);
    setError(null);
    setReport(null);

    const controller = new AbortController();
    abortRef.current = controller;
    const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);

    try {
      const formData = new FormData();
      formData.append('file', file);
      const res = await fetch('/api/apps/scrubber?mode=direct', {
        method: 'POST',
        body: formData,
        signal: controller.signal,
      });

      if (!res.ok) {
        if (res.status === 401) {
          window.location.href = '/login';
          return;
        }
        const errJson = await res.json().catch(() => null);
        const msg = errJson?.message || errJson?.error;
        setError(typeof msg === 'string' && msg ? msg : GENERIC_ERROR);
        return;
      }

      const blob = await res.blob();
      const name = filenameFromDisposition(res.headers.get('content-disposition'), file.name);
      setReport(decodeScrubReport(res.headers.get('x-scrub-report')));

      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = name;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch {
      // Timeout (abort) atau koneksi terputus: pesan umum; file tetap dipilih.
      setError(GENERIC_ERROR);
    } finally {
      clearTimeout(timer);
      setBusy(false);
    }
  };

  return (
    <PageShell currentPath="/scrubber">
      <div className="max-w-3xl">
        <div className="mb-8">
          <div className="flex items-center gap-2 text-xs font-semibold text-rausch uppercase tracking-wider mb-1">
            <ShieldCheck size={14} />
            <span>Privacy Utility</span>
          </div>
          <h1 className="text-3xl font-bold text-ink dark:text-dark-ink tracking-tight mb-2">Metadata Scrubber</h1>
          <p className="text-sm text-muted dark:text-dark-muted">
            Strip GPS, device serials and author info from JPEG, PNG and PDF files. Processed in memory only.
          </p>
        </div>

        {error && (
          <div
            role="alert"
            className="bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-sm mb-6 flex items-start gap-2.5"
          >
            <AlertCircle size={18} className="shrink-0 mt-0.5" />
            <div className="flex-1 text-xs">{error}</div>
          </div>
        )}

        {notice && (
          <div
            role="status"
            className="bg-surface-strong dark:bg-dark-surface border border-hairline dark:border-dark-border text-ink dark:text-dark-ink px-4 py-3 rounded-sm text-sm mb-6"
          >
            {notice}
          </div>
        )}

        <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline">
          <div
            onDragOver={(e) => {
              e.preventDefault();
              setDragging(true);
            }}
            onDragLeave={() => setDragging(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragging(false);
              if (!busy) selectFiles(e.dataTransfer.files);
            }}
            onClick={() => !busy && inputRef.current?.click()}
            className={`border-2 border-dashed rounded-sm p-10 text-center transition-colors bg-surface-soft/40 dark:bg-dark-surface/40 ${
              busy ? 'cursor-not-allowed opacity-60' : 'cursor-pointer'
            } ${
              dragging
                ? 'border-ink dark:border-dark-ink'
                : 'border-hairline dark:border-dark-border hover:border-ink dark:hover:border-dark-ink'
            }`}
          >
            <input
              type="file"
              ref={inputRef}
              multiple
              disabled={busy}
              accept=".jpg,.jpeg,.png,.pdf,image/jpeg,image/png,application/pdf"
              onChange={(e) => {
                selectFiles(e.target.files);
                e.target.value = '';
              }}
              className="hidden"
              aria-label="Select file to scrub"
            />

            {file ? (
              <div className="space-y-3">
                <div className="w-12 h-12 rounded-full bg-rose-50 dark:bg-rose-950/40 text-rausch mx-auto flex items-center justify-center">
                  <FileText size={24} />
                </div>
                <div>
                  <p className="font-semibold text-ink dark:text-dark-ink text-sm break-all">{file.name}</p>
                  <p className="text-xs text-muted dark:text-dark-muted">{formatFileSize(file.size)}</p>
                </div>
                <button
                  type="button"
                  disabled={busy}
                  onClick={(e) => {
                    e.stopPropagation();
                    setFile(null);
                    setReport(null);
                    setNotice(null);
                  }}
                  className="inline-flex items-center gap-1 text-xs text-airbnb-error hover:underline cursor-pointer disabled:opacity-60"
                >
                  <Trash2 size={13} />
                  <span>Remove file</span>
                </button>
              </div>
            ) : (
              <div className="space-y-3">
                <div className="w-12 h-12 rounded-full bg-surface-strong dark:bg-dark-surface text-muted dark:text-dark-muted mx-auto flex items-center justify-center">
                  <Upload size={22} />
                </div>
                <div>
                  <p className="font-semibold text-ink dark:text-dark-ink text-sm">Click to upload or drag & drop</p>
                  <p className="text-xs text-muted dark:text-dark-muted mt-1">JPEG, PNG or PDF, up to 25 MB</p>
                </div>
              </div>
            )}
          </div>

          <div className="mt-6 pt-4 border-t border-hairline-soft dark:border-dark-hairline flex items-center justify-between">
            <span className="text-xs text-muted dark:text-dark-muted">Strict Zero-Footprint: nothing is stored</span>
            <button
              type="button"
              onClick={handleScrub}
              disabled={!file || busy}
              className={PRIMARY_BUTTON}
            >
              <Download size={16} />
              <span>{busy ? 'Scrubbing...' : 'Scrub & Download'}</span>
            </button>
          </div>
        </div>

        {report && (
          <div className="mt-6 bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline">
            <div className="flex items-center gap-2 mb-4 text-emerald-800 dark:text-emerald-400 text-sm font-semibold">
              <CheckCircle2 size={18} />
              <span>Clean file downloaded ({report.file_kind.toUpperCase()})</span>
            </div>
            <h2 className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-3">
              Scrub Report
            </h2>
            {report.removed.length === 0 ? (
              <p className="text-xs text-muted dark:text-dark-muted">No sensitive metadata was found.</p>
            ) : (
              <ul className="space-y-1.5 text-xs">
                {report.removed.map((r) => (
                  <li
                    key={r.category}
                    className="flex justify-between py-1 border-b border-hairline-soft/60 dark:border-dark-hairline/60"
                  >
                    <span className="font-mono text-ink dark:text-dark-ink">{r.category}</span>
                    <span className="font-mono font-semibold text-ink dark:text-dark-ink">{r.count}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </div>
      </PageShell>
  );
};

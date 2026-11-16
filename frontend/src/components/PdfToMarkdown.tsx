import React, { useEffect, useRef, useState } from 'react';
import {
  Upload,
  FileText,
  Trash2,
  AlertCircle,
  FileDown,
  Copy,
  Check,
  Loader2,
  FileType2,
} from 'lucide-react';
import { PageShell } from './PageShell';
import { formatFileSize } from '../lib/formatFileSize';
import { pickFirstFile } from '../lib/pickFirstFile';
import { PRIMARY_BUTTON } from '../lib/ui';
import { validatePdfFile } from '../lib/validatePdfFile';
import { parsePdfOptions } from '../lib/parsePdfOptions';
import { pdfErrorMessage } from '../lib/pdfErrorMessage';
import { parseMarkdownPreview, type Inline, type PreviewBlock } from '../lib/markdownPreview';
import { markdownToBlob, saveAs } from '../lib/downloadText';
import type { PdfConversionResult, PdfLimits } from '../types/pdf';

const TIMEOUT_MS = 60_000;
const COPY_CONFIRM_MS = 2_000;
const LIMITS_ERROR = 'Batas ukuran file tidak dapat dimuat dari server. Muat ulang halaman lalu coba lagi.';

type ViewMode = 'preview' | 'raw';

function renderInlines(inlines: Inline[]): React.ReactNode {
  return inlines.map((s, i) => {
    let node: React.ReactNode = s.text;
    if (s.italic) node = <em>{node}</em>;
    if (s.bold) node = <strong>{node}</strong>;
    return <React.Fragment key={i}>{node}</React.Fragment>;
  });
}

const HEADING_CLASS: Record<number, string> = {
  1: 'text-2xl font-bold',
  2: 'text-xl font-bold',
  3: 'text-lg font-semibold',
  4: 'text-base font-semibold',
  5: 'text-sm font-semibold',
  6: 'text-sm font-semibold text-muted dark:text-dark-muted',
};

/** Render blok pratinjau sebagai node teks React (tanpa HTML mentah). */
function renderBlock(block: PreviewBlock, key: number): React.ReactNode {
  switch (block.kind) {
    case 'heading':
      return (
        <div key={key} role="heading" aria-level={block.level} className={`${HEADING_CLASS[block.level]} mt-4 mb-2`}>
          {renderInlines(block.inlines)}
        </div>
      );
    case 'paragraph':
      return (
        <p key={key} className="mb-3 text-sm leading-relaxed">
          {renderInlines(block.inlines)}
        </p>
      );
    case 'list':
      return (
        <ul key={key} className="mb-3 text-sm space-y-1 list-none">
          {block.items.map((item, i) => (
            <li key={i} style={{ marginLeft: `${item.level * 1.25}rem` }} className="flex gap-2">
              <span className="shrink-0 text-muted dark:text-dark-muted">
                {block.ordered ? `${item.n ?? i + 1}.` : '•'}
              </span>
              <span>{renderInlines(item.inlines)}</span>
            </li>
          ))}
        </ul>
      );
    case 'code':
      return (
        <pre
          key={key}
          className="mb-3 p-3 rounded-sm bg-surface-strong dark:bg-dark-surface text-xs font-mono overflow-x-auto whitespace-pre"
        >
          {block.text}
        </pre>
      );
    case 'hr':
      return <hr key={key} className="my-4 border-hairline dark:border-dark-border" />;
  }
}

export const PdfToMarkdown: React.FC = () => {
  const [limits, setLimits] = useState<PdfLimits | null>(null);
  const [limitsError, setLimitsError] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [pageBreaks, setPageBreaks] = useState(false);
  const [strip, setStrip] = useState(true);
  const [optionError, setOptionError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<PdfConversionResult | null>(null);
  const [mode, setMode] = useState<ViewMode>('preview');
  const [dragging, setDragging] = useState(false);
  const [copyState, setCopyState] = useState<'idle' | 'done' | 'failed'>('idle');

  const inputRef = useRef<HTMLInputElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Muat batas dari server saat mount.
  useEffect(() => {
    const controller = new AbortController();
    (async () => {
      try {
        const res = await fetch('/api/apps/pdf-to-markdown/limits', { signal: controller.signal });
        if (res.status === 401) {
          window.location.href = '/login';
          return;
        }
        if (!res.ok) throw new Error('limits');
        const data = (await res.json()) as PdfLimits;
        if (typeof data?.max_bytes !== 'number') throw new Error('limits');
        setLimits(data);
      } catch {
        if (!controller.signal.aborted) setLimitsError(LIMITS_ERROR);
      }
    })();
    return () => controller.abort();
  }, []);

  // Cleanup saat unmount: batalkan permintaan, kosongkan state.
  useEffect(
    () => () => {
      abortRef.current?.abort();
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
      setFile(null);
      setResult(null);
      setError(null);
      setNotice(null);
    },
    [],
  );

  const selectFiles = (files: FileList | null) => {
    const { file: first, multiple } = pickFirstFile<File>(files);
    if (!first) return;
    setNotice(multiple ? 'Hanya satu file yang diproses. File pertama dipilih.' : null);
    setError(null);
    if (!limits) {
      setFileError(limitsError ?? LIMITS_ERROR);
      return;
    }
    const problem = validatePdfFile({ name: first.name, size: first.size, type: first.type }, limits.max_bytes);
    if (problem) {
      setFileError(
        problem === 'too_large'
          ? `Ukuran file melebihi batas maksimum (${formatFileSize(limits.max_bytes)}).`
          : 'File bukan PDF. Pilih file dengan ekstensi .pdf.',
      );
      return;
    }
    setFileError(null);
    setFile(first);
    setResult(null);
  };

  const handleConvert = async () => {
    if (!file || busy) return;
    const parsed = parsePdfOptions({ from: from.trim(), to: to.trim(), pageBreaks, strip });
    if (!parsed.ok) {
      setOptionError(parsed.message);
      return;
    }
    setOptionError(null);
    setBusy(true);
    setError(null);
    setResult(null);
    setCopyState('idle');

    const controller = new AbortController();
    abortRef.current = controller;
    const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);

    try {
      const formData = new FormData();
      formData.append('file', file);
      formData.append('options', JSON.stringify(parsed.options));
      const res = await fetch('/api/apps/pdf-to-markdown', {
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
        const code = typeof errJson?.code === 'string' ? errJson.code : undefined;
        setError(pdfErrorMessage(res.status, code));
        return;
      }

      const data = (await res.json()) as PdfConversionResult;
      setResult(data);
      setMode('preview');
    } catch {
      if (!controller.signal.aborted || abortRef.current === controller) {
        setError(pdfErrorMessage('network'));
      }
    } finally {
      clearTimeout(timer);
      setBusy(false);
    }
  };

  const handleCopy = async () => {
    if (!result) return;
    if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    try {
      await navigator.clipboard.writeText(result.markdown);
      setCopyState('done');
    } catch {
      setCopyState('failed');
    }
    copyTimerRef.current = setTimeout(() => setCopyState('idle'), COPY_CONFIRM_MS);
  };

  const handleDownload = () => {
    if (!result) return;
    saveAs(markdownToBlob(result.markdown), 'document.md');
  };

  const inputClass =
    'h-10 w-28 px-3 rounded-sm border border-hairline dark:border-dark-border bg-white dark:bg-dark-surface text-sm text-ink dark:text-dark-ink disabled:opacity-60';

  return (
    <PageShell currentPath="/pdf-to-markdown">
      <div className="max-w-3xl">
        <div className="mb-8">
          <div className="flex items-center gap-2 text-xs font-semibold text-rausch uppercase tracking-wider mb-1">
            <FileType2 size={14} />
            <span>Document Utility</span>
          </div>
          <h1 className="text-3xl font-bold text-ink dark:text-dark-ink tracking-tight mb-2">PDF to Markdown</h1>
          <p className="text-sm text-muted dark:text-dark-muted">
            Ubah PDF berlapis teks menjadi Markdown. Diproses di server tanpa menyimpan apa pun.
          </p>
        </div>

        {limitsError && (
          <div
            role="alert"
            className="bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-xs mb-6 flex items-start gap-2.5"
          >
            <AlertCircle size={18} className="shrink-0 mt-0.5" />
            <div className="flex-1">{limitsError}</div>
          </div>
        )}

        {(fileError || optionError || error) && (
          <div
            role="alert"
            className="bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-xs mb-6 flex items-start gap-2.5"
          >
            <AlertCircle size={18} className="shrink-0 mt-0.5" />
            <div className="flex-1">{fileError || optionError || error}</div>
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
              accept=".pdf,application/pdf"
              onChange={(e) => {
                selectFiles(e.target.files);
                e.target.value = '';
              }}
              className="hidden"
              aria-label="Pilih file PDF"
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
                    setResult(null);
                    setNotice(null);
                    setError(null);
                  }}
                  className="inline-flex items-center gap-1 text-xs text-airbnb-error hover:underline cursor-pointer disabled:opacity-60"
                >
                  <Trash2 size={13} />
                  <span>Hapus file</span>
                </button>
              </div>
            ) : (
              <div className="space-y-3">
                <div className="w-12 h-12 rounded-full bg-surface-strong dark:bg-dark-surface text-muted dark:text-dark-muted mx-auto flex items-center justify-center">
                  <Upload size={22} />
                </div>
                <div>
                  <p className="font-semibold text-ink dark:text-dark-ink text-sm">
                    Klik untuk unggah atau seret &amp; lepas
                  </p>
                  <p className="text-xs text-muted dark:text-dark-muted mt-1">
                    PDF{limits ? `, maksimum ${formatFileSize(limits.max_bytes)}` : ''}
                  </p>
                </div>
              </div>
            )}
          </div>

          <div className="mt-6 flex flex-wrap items-end gap-4">
            <label className="text-xs text-muted dark:text-dark-muted flex flex-col gap-1">
              <span>Halaman awal</span>
              <input
                type="text"
                inputMode="numeric"
                value={from}
                disabled={busy}
                onChange={(e) => setFrom(e.target.value)}
                placeholder="1"
                aria-label="page_from"
                className={inputClass}
              />
            </label>
            <label className="text-xs text-muted dark:text-dark-muted flex flex-col gap-1">
              <span>Halaman akhir</span>
              <input
                type="text"
                inputMode="numeric"
                value={to}
                disabled={busy}
                onChange={(e) => setTo(e.target.value)}
                placeholder="terakhir"
                aria-label="page_to"
                className={inputClass}
              />
            </label>
            <label className="flex items-center gap-2 text-sm text-ink dark:text-dark-ink h-10">
              <input
                type="checkbox"
                checked={pageBreaks}
                disabled={busy}
                onChange={(e) => setPageBreaks(e.target.checked)}
              />
              <span>page_breaks</span>
            </label>
            <label className="flex items-center gap-2 text-sm text-ink dark:text-dark-ink h-10">
              <input
                type="checkbox"
                checked={strip}
                disabled={busy}
                onChange={(e) => setStrip(e.target.checked)}
              />
              <span>strip_headers_footers</span>
            </label>
          </div>

          <div className="mt-6 pt-4 border-t border-hairline-soft dark:border-dark-hairline flex items-center justify-between">
            <span className="text-xs text-muted dark:text-dark-muted">Strict Zero-Footprint: nothing is stored</span>
            <button type="button" onClick={handleConvert} disabled={!file || busy} className={PRIMARY_BUTTON}>
              {busy ? <Loader2 size={16} className="animate-spin" /> : <FileDown size={16} />}
              <span>{busy ? 'Mengonversi...' : 'Konversi ke Markdown'}</span>
            </button>
          </div>
        </div>

        {result && (
          <div className="mt-6 bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline">
            <div className="flex flex-wrap items-center justify-between gap-3 mb-4">
              <div className="text-xs text-muted dark:text-dark-muted flex gap-4">
                <span>
                  Halaman: <strong className="text-ink dark:text-dark-ink">{result.pages}</strong>
                </span>
                <span>
                  Heading: <strong className="text-ink dark:text-dark-ink">{result.headings}</strong>
                </span>
              </div>
              <div className="flex items-center gap-2">
                <div role="group" aria-label="Mode tampilan" className="flex text-xs">
                  <button
                    type="button"
                    aria-pressed={mode === 'preview'}
                    onClick={() => setMode('preview')}
                    className={`px-3 py-1.5 rounded-l-sm border border-hairline dark:border-dark-border cursor-pointer ${
                      mode === 'preview'
                        ? 'bg-ink text-white dark:bg-dark-ink dark:text-dark-canvas'
                        : 'text-ink dark:text-dark-ink'
                    }`}
                  >
                    Pratinjau
                  </button>
                  <button
                    type="button"
                    aria-pressed={mode === 'raw'}
                    onClick={() => setMode('raw')}
                    className={`px-3 py-1.5 rounded-r-sm border border-l-0 border-hairline dark:border-dark-border cursor-pointer ${
                      mode === 'raw'
                        ? 'bg-ink text-white dark:bg-dark-ink dark:text-dark-canvas'
                        : 'text-ink dark:text-dark-ink'
                    }`}
                  >
                    Markdown mentah
                  </button>
                </div>
                <button
                  type="button"
                  onClick={handleCopy}
                  className="inline-flex items-center gap-1 px-3 py-1.5 text-xs rounded-sm border border-hairline dark:border-dark-border text-ink dark:text-dark-ink cursor-pointer"
                >
                  {copyState === 'done' ? <Check size={13} /> : <Copy size={13} />}
                  <span>{copyState === 'done' ? 'Tersalin' : 'Salin'}</span>
                </button>
                <button
                  type="button"
                  onClick={handleDownload}
                  className="inline-flex items-center gap-1 px-3 py-1.5 text-xs rounded-sm border border-hairline dark:border-dark-border text-ink dark:text-dark-ink cursor-pointer"
                >
                  <FileDown size={13} />
                  <span>Unduh .md</span>
                </button>
              </div>
            </div>

            {copyState === 'failed' && (
              <p role="alert" className="text-xs text-airbnb-error mb-3">
                Gagal menyalin ke clipboard. Hasil tetap tersedia di layar.
              </p>
            )}

            {result.warnings.length > 0 && (
              <ul
                aria-label="Peringatan"
                className="mb-4 px-4 py-3 rounded-sm bg-surface-strong dark:bg-dark-surface text-xs text-ink dark:text-dark-ink list-disc list-inside space-y-1"
              >
                {result.warnings.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            )}

            {mode === 'preview' ? (
              <div data-testid="markdown-preview" className="text-ink dark:text-dark-ink">
                {parseMarkdownPreview(result.markdown).map(renderBlock)}
              </div>
            ) : (
              <pre
                data-testid="markdown-raw"
                className="p-3 rounded-sm bg-surface-strong dark:bg-dark-surface text-xs font-mono text-ink dark:text-dark-ink overflow-x-auto whitespace-pre-wrap break-words"
              >
                {result.markdown}
              </pre>
            )}
          </div>
        )}
      </div>
    </PageShell>
  );
};

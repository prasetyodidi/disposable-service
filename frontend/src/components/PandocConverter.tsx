import React, { useState, useRef, useEffect } from 'react';
import {
  FileText,
  Upload,
  Download,
  CheckCircle2,
  AlertCircle,
  FileCode,
  Sparkles,
  Sliders,
  Trash2,
} from 'lucide-react';
import { PageShell } from './PageShell';
import type { PandocPresetInfo, PandocPresetValues } from '../types/pandoc';
import { PRIMARY_BUTTON } from '../lib/ui';

const SAMPLE_MARKDOWN = `# Disposable Workspace: Confidential Report

**Date:** ${new Date().toLocaleDateString()}  
**Status:** Highly Confidential  
**Classification:** Ephemeral Zero-Footprint

---

## 1. Executive Summary

This document demonstrates the **Markdown to PDF** conversion pipeline executed within an isolated, on-demand disposable container.

### Key Highlights:
- **Instant Processing:** LaTeX (\`pdflatex\`) typesetting rendered on-the-fly.
- **Strict Privacy:** Zero temporary files retained after streaming.
- **Deterministic Output:** Standardized typography, margins, and geometric spacing.

## 2. Performance & Security Metrics

| Parameter | Specification | Verification |
| :--- | :--- | :--- |
| **PDF Engine** | \`pdflatex\` | Standard Debian Base |
| **Paper Size** | A4 | Exact Dimensions |
| **Link Colors** | Navy Blue | Validated |
| **Disk Retention** | 0 Bytes | RAII Guard Verified |

\`\`\`rust
// Sample backend execution guard in Rust
struct TempDirGuard {
    path: PathBuf,
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
\`\`\`

> *"Data security is strongest when there is nothing left to steal."*
`;

export const PandocConverter: React.FC = () => {
  const [mode, setMode] = useState<'paste' | 'upload'>('paste');
  const [markdown, setMarkdown] = useState<string>('');
  const [file, setFile] = useState<File | null>(null);
  const [converting, setConverting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [presetInfo, setPresetInfo] = useState<PandocPresetInfo | null>(null);
  const [preset, setPreset] = useState<PandocPresetValues | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch('/api/apps/pandoc/preset')
      .then((r) => (r.ok ? r.json() : null))
      .then((info: PandocPresetInfo | null) => {
        if (cancelled || !info) return;
        setPresetInfo(info);
        setPreset(info.defaults);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const presetDirty =
    presetInfo !== null && preset !== null && JSON.stringify(preset) !== JSON.stringify(presetInfo.defaults);

  const updatePreset = (patch: Partial<PandocPresetValues>) => {
    setPreset((prev) => (prev ? { ...prev, ...patch } : prev));
  };

  const handleLoadSample = () => {
    setMarkdown(SAMPLE_MARKDOWN);
    setError(null);
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files[0]) {
      const selected = e.target.files[0];
      if (!selected.name.endsWith('.md') && !selected.name.endsWith('.markdown') && !selected.name.endsWith('.txt')) {
        setError('Please select a markdown file (.md or .txt)');
        return;
      }
      setFile(selected);
      setError(null);
    }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer.files && e.dataTransfer.files[0]) {
      const selected = e.dataTransfer.files[0];
      setFile(selected);
      setError(null);
    }
  };

  const handleConvert = async () => {
    setError(null);
    setSuccessMsg(null);

    if (mode === 'paste' && !markdown.trim()) {
      setError('Please enter or paste Markdown text first.');
      return;
    }

    if (mode === 'upload' && !file) {
      setError('Please upload a Markdown file first.');
      return;
    }

    setConverting(true);

    try {
      let response: Response;

      if (mode === 'paste') {
        response = await fetch('/api/apps/pandoc/convert', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            markdown,
            filename: 'document.pdf',
            preset: preset ?? undefined,
          }),
        });
      } else {
        const formData = new FormData();
        formData.append('file', file as File);
        if (preset) formData.append('preset', JSON.stringify(preset));
        response = await fetch('/api/apps/pandoc/convert-upload', {
          method: 'POST',
          body: formData,
        });
      }

      if (!response.ok) {
        if (response.status === 401) {
          window.location.href = '/login';
          return;
        }
        const errJson = await response.json().catch(() => null);
        throw new Error(errJson?.message || `Conversion error (Status: ${response.status})`);
      }

      // Stream received as PDF Blob
      const blob = await response.blob();
      const disposition = response.headers.get('content-disposition');
      let filename = 'document.pdf';
      if (disposition && disposition.includes('filename=')) {
        const match = disposition.match(/filename="?([^"]+)"?/);
        if (match && match[1]) filename = match[1];
      }

      // Create download trigger
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      window.URL.revokeObjectURL(url);

      setSuccessMsg('PDF generated and downloaded. Temporary files wiped from container.');
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : 'Unknown error during conversion';
      setError(message);
    } finally {
      setConverting(false);
    }
  };

  return (
    <PageShell currentPath="/pandoc">
        <div className="mb-8">
          <div className="flex items-center gap-2 text-xs font-semibold text-rausch uppercase tracking-wider mb-1">
            <Sparkles size={14} />
            <span>Service Utility</span>
          </div>
          <h1 className="text-3xl font-bold text-ink dark:text-dark-ink tracking-tight mb-2">
            Markdown to PDF Converter
          </h1>
          <p className="text-sm text-muted dark:text-dark-muted">
            Typeset and compile markdown to PDF via Pandoc and LaTeX pdflatex engine.
          </p>
        </div>

        {error && (
          <div className="bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-sm mb-6 flex items-start gap-2.5">
            <AlertCircle size={18} className="shrink-0 mt-0.5" />
            <div className="flex-1 overflow-x-auto font-mono text-xs">{error}</div>
          </div>
        )}

        {successMsg && (
          <div className="bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 text-emerald-800 dark:text-emerald-400 px-4 py-3 rounded-sm text-sm mb-6 flex items-center gap-2.5">
            <CheckCircle2 size={18} className="text-emerald-600 dark:text-emerald-400 shrink-0" />
            <span>{successMsg}</span>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
          {/* Main Input Area (2 Cols) */}
          <div className="lg:col-span-2 space-y-6">
            <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline">
              {/* Tab Selector */}
              <div className="flex items-center justify-between mb-5 pb-4 border-b border-hairline-soft dark:border-dark-hairline">
                <div className="inline-flex bg-surface-strong dark:bg-dark-surface p-1 rounded-full gap-1">
                  <button
                    onClick={() => {
                      setMode('paste');
                      setError(null);
                    }}
                    className={`px-4 py-1.5 rounded-full text-xs font-semibold transition-all cursor-pointer ${
                      mode === 'paste'
                        ? 'bg-white dark:bg-dark-card text-ink dark:text-dark-ink shadow-xs'
                        : 'text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink'
                    }`}
                  >
                    Paste Raw Markdown
                  </button>
                  <button
                    onClick={() => {
                      setMode('upload');
                      setError(null);
                    }}
                    className={`px-4 py-1.5 rounded-full text-xs font-semibold transition-all cursor-pointer ${
                      mode === 'upload'
                        ? 'bg-white dark:bg-dark-card text-ink dark:text-dark-ink shadow-xs'
                        : 'text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink'
                    }`}
                  >
                    Upload File (.md)
                  </button>
                </div>

                {mode === 'paste' && (
                  <button
                    onClick={handleLoadSample}
                    className="text-xs font-medium text-rausch hover:text-rausch-active hover:underline flex items-center gap-1 cursor-pointer"
                  >
                    <FileCode size={14} />
                    <span>Load Sample Markdown</span>
                  </button>
                )}
              </div>

              {/* Mode: Paste */}
              {mode === 'paste' && (
                <div>
                  <textarea
                    rows={16}
                    value={markdown}
                    onChange={(e) => setMarkdown(e.target.value)}
                    placeholder="# Type or paste your Markdown content here...&#10;&#10;Supports headers, tables, code fences, blockquotes, lists, and LaTeX math formulas."
                    className="w-full p-4 text-sm font-mono bg-white dark:bg-dark-surface border border-hairline dark:border-dark-border rounded-sm text-ink dark:text-dark-ink placeholder:text-muted dark:placeholder:text-dark-muted-soft focus:border-ink dark:focus:border-dark-ink focus:ring-0 outline-none transition-all resize-y leading-relaxed"
                  />
                  <div className="flex justify-between items-center mt-2 text-xs text-muted dark:text-dark-muted">
                    <span>Markdown input format</span>
                    <span>
                      {markdown.length} characters • {markdown.split('\n').length} lines
                    </span>
                  </div>
                </div>
              )}

              {/* Mode: Upload */}
              {mode === 'upload' && (
                <div
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={handleDrop}
                  onClick={() => fileInputRef.current?.click()}
                  className="border-2 border-dashed border-hairline dark:border-dark-border hover:border-ink dark:hover:border-dark-ink rounded-sm p-10 text-center cursor-pointer transition-colors bg-surface-soft/40 dark:bg-dark-surface/40"
                >
                  <input
                    type="file"
                    ref={fileInputRef}
                    onChange={handleFileChange}
                    accept=".md,.markdown,.txt"
                    className="hidden"
                  />

                  {file ? (
                    <div className="space-y-3">
                      <div className="w-12 h-12 rounded-full bg-rose-50 dark:bg-rose-950/40 text-rausch mx-auto flex items-center justify-center">
                        <FileText size={24} />
                      </div>
                      <div>
                        <p className="font-semibold text-ink dark:text-dark-ink text-sm">{file.name}</p>
                        <p className="text-xs text-muted dark:text-dark-muted">{(file.size / 1024).toFixed(1)} KB</p>
                      </div>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setFile(null);
                        }}
                        className="inline-flex items-center gap-1 text-xs text-red-600 dark:text-red-400 hover:underline cursor-pointer"
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
                        <p className="font-semibold text-ink dark:text-dark-ink text-sm">
                          Click to upload or drag & drop
                        </p>
                        <p className="text-xs text-muted dark:text-dark-muted mt-1">
                          Supported files: Markdown (.md, .markdown, .txt)
                        </p>
                      </div>
                    </div>
                  )}
                </div>
              )}

              {/* Action Button */}
              <div className="mt-6 pt-4 border-t border-hairline-soft dark:border-dark-hairline flex items-center justify-between">
                <span className="text-xs text-muted dark:text-dark-muted">
                  Strict Zero-Footprint: Auto-deleted post-download
                </span>

                <button
                  onClick={handleConvert}
                  disabled={converting}
                  className={PRIMARY_BUTTON}
                >
                  <Download size={16} />
                  <span>{converting ? 'Typesetting PDF...' : 'Convert & Download PDF'}</span>
                </button>
              </div>
            </div>
          </div>

          {/* Sidebar / Parameters Card (1 Col) */}
          <div className="space-y-6">
            <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline">
              <div className="flex items-center justify-between gap-2 mb-4 pb-3 border-b border-hairline-soft dark:border-dark-hairline">
                <div className="flex items-center gap-2">
                  <Sliders size={18} className="text-rausch" />
                  <h3 className="text-sm font-bold text-ink dark:text-dark-ink uppercase tracking-wider">
                    Pandoc Preset Specs
                  </h3>
                </div>
                {presetDirty && presetInfo && (
                  <button
                    type="button"
                    onClick={() => setPreset(presetInfo.defaults)}
                    className="text-xs font-semibold text-rausch hover:underline"
                  >
                    Reset
                  </button>
                )}
              </div>
              {!preset || !presetInfo ? (
                <p className="text-xs text-muted dark:text-dark-muted">Loading preset…</p>
              ) : (
                <div className="space-y-3 text-xs">
                  <div className="flex justify-between py-1 border-b border-hairline-soft/60 dark:border-dark-hairline/60">
                    <span className="text-muted dark:text-dark-muted">PDF Engine:</span>
                    <span className="font-mono font-semibold text-ink dark:text-dark-ink">{presetInfo.engine}</span>
                  </div>
                  <label className="flex items-center justify-between gap-3">
                    <span className="text-muted dark:text-dark-muted">Paper Size:</span>
                    <select
                      aria-label="Paper size"
                      value={preset.paper_size}
                      onChange={(e) => updatePreset({ paper_size: e.target.value })}
                      className="w-32 rounded-sm border border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-surface px-2 py-1 font-mono text-ink dark:text-dark-ink"
                    >
                      {presetInfo.paper_sizes.map((p) => (
                        <option key={p} value={p}>
                          {p.toUpperCase()}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="flex items-center justify-between gap-3">
                    <span className="text-muted dark:text-dark-muted">Margin top/bottom:</span>
                    <input
                      aria-label="Margin top and bottom"
                      value={preset.margin_y}
                      onChange={(e) => updatePreset({ margin_y: e.target.value })}
                      className="w-32 rounded-sm border border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-surface px-2 py-1 font-mono text-ink dark:text-dark-ink"
                    />
                  </label>
                  <label className="flex items-center justify-between gap-3">
                    <span className="text-muted dark:text-dark-muted">Margin left/right:</span>
                    <input
                      aria-label="Margin left and right"
                      value={preset.margin_x}
                      onChange={(e) => updatePreset({ margin_x: e.target.value })}
                      className="w-32 rounded-sm border border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-surface px-2 py-1 font-mono text-ink dark:text-dark-ink"
                    />
                  </label>
                  <label className="flex items-center justify-between gap-3">
                    <span className="text-muted dark:text-dark-muted">Font Size:</span>
                    <input
                      aria-label="Font size"
                      value={preset.font_size}
                      onChange={(e) => updatePreset({ font_size: e.target.value })}
                      className="w-32 rounded-sm border border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-surface px-2 py-1 font-mono text-ink dark:text-dark-ink"
                    />
                  </label>
                  <label className="flex items-center justify-between gap-3">
                    <span className="text-muted dark:text-dark-muted">Line Height:</span>
                    <input
                      aria-label="Line height"
                      type="number"
                      step="0.05"
                      min="1"
                      max="2.5"
                      value={preset.line_height}
                      onChange={(e) => updatePreset({ line_height: Number(e.target.value) })}
                      className="w-32 rounded-sm border border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-surface px-2 py-1 font-mono text-ink dark:text-dark-ink"
                    />
                  </label>
                  <label className="flex items-center justify-between gap-3">
                    <span className="text-muted dark:text-dark-muted">Link Color:</span>
                    <select
                      aria-label="Link color"
                      value={preset.link_color}
                      onChange={(e) => updatePreset({ link_color: e.target.value })}
                      className="w-32 rounded-sm border border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-surface px-2 py-1 font-mono text-ink dark:text-dark-ink"
                    >
                      {presetInfo.link_colors.map((c) => (
                        <option key={c} value={c}>
                          {c}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="flex items-center justify-between gap-3">
                    <span className="text-muted dark:text-dark-muted">Font Family:</span>
                    <input
                      aria-label="Font family"
                      placeholder="Engine default"
                      value={preset.main_font ?? ''}
                      onChange={(e) => updatePreset({ main_font: e.target.value || null })}
                      className="w-32 rounded-sm border border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-surface px-2 py-1 font-mono text-ink dark:text-dark-ink"
                    />
                  </label>
                </div>
              )}
              <div className="mt-6 p-3 bg-surface-soft dark:bg-dark-surface rounded-sm text-xs text-muted dark:text-dark-muted leading-relaxed">
                {presetInfo?.notes.map((n) => (
                  <p key={n}>{n}</p>
                ))}
                <p className="mt-1">Defaults follow Section 4.3.2 of the PRD. Changes apply to the next conversion only.</p>
              </div>
            </div>
          </div>
        </div>
      </PageShell>
  );
};

import React, { useState, useRef } from 'react';
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
import { Navbar } from './Navbar';

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
          }),
        });
      } else {
        const formData = new FormData();
        formData.append('file', file as File);
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
    <div className="min-h-screen bg-surface-soft flex flex-col">
      <Navbar currentPath="/pandoc" />

      <main className="flex-1 max-w-6xl w-full mx-auto px-6 py-10">
        <div className="mb-8">
          <div className="flex items-center gap-2 text-xs font-semibold text-rausch uppercase tracking-wider mb-1">
            <Sparkles size={14} />
            <span>Service Utility</span>
          </div>
          <h1 className="text-3xl font-bold text-ink tracking-tight mb-2">
            Markdown to PDF Converter
          </h1>
          <p className="text-sm text-muted">
            Typeset and compile markdown to PDF via Pandoc and LaTeX pdflatex engine.
          </p>
        </div>

        {error && (
          <div className="bg-red-50 border border-red-200 text-airbnb-error px-4 py-3 rounded-sm text-sm mb-6 flex items-start gap-2.5">
            <AlertCircle size={18} className="shrink-0 mt-0.5" />
            <div className="flex-1 overflow-x-auto font-mono text-xs">{error}</div>
          </div>
        )}

        {successMsg && (
          <div className="bg-emerald-50 border border-emerald-200 text-emerald-800 px-4 py-3 rounded-sm text-sm mb-6 flex items-center gap-2.5">
            <CheckCircle2 size={18} className="text-emerald-600 shrink-0" />
            <span>{successMsg}</span>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
          {/* Main Input Area (2 Cols) */}
          <div className="lg:col-span-2 space-y-6">
            <div className="bg-white rounded-md shadow-airbnb p-6 border border-hairline-soft">
              {/* Tab Selector */}
              <div className="flex items-center justify-between mb-5 pb-4 border-b border-hairline-soft">
                <div className="inline-flex bg-surface-strong p-1 rounded-full gap-1">
                  <button
                    onClick={() => {
                      setMode('paste');
                      setError(null);
                    }}
                    className={`px-4 py-1.5 rounded-full text-xs font-semibold transition-all cursor-pointer ${
                      mode === 'paste'
                        ? 'bg-white text-ink shadow-xs'
                        : 'text-muted hover:text-ink'
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
                        ? 'bg-white text-ink shadow-xs'
                        : 'text-muted hover:text-ink'
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
                    className="w-full p-4 text-sm font-mono bg-white border border-hairline rounded-sm text-ink placeholder:text-muted focus:border-ink focus:ring-0 outline-none transition-all resize-y leading-relaxed"
                  />
                  <div className="flex justify-between items-center mt-2 text-xs text-muted">
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
                  className="border-2 border-dashed border-hairline hover:border-ink rounded-sm p-10 text-center cursor-pointer transition-colors bg-surface-soft/40"
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
                      <div className="w-12 h-12 rounded-full bg-rose-50 text-rausch mx-auto flex items-center justify-center">
                        <FileText size={24} />
                      </div>
                      <div>
                        <p className="font-semibold text-ink text-sm">{file.name}</p>
                        <p className="text-xs text-muted">{(file.size / 1024).toFixed(1)} KB</p>
                      </div>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setFile(null);
                        }}
                        className="inline-flex items-center gap-1 text-xs text-red-600 hover:underline cursor-pointer"
                      >
                        <Trash2 size={13} />
                        <span>Remove file</span>
                      </button>
                    </div>
                  ) : (
                    <div className="space-y-3">
                      <div className="w-12 h-12 rounded-full bg-surface-strong text-muted mx-auto flex items-center justify-center">
                        <Upload size={22} />
                      </div>
                      <div>
                        <p className="font-semibold text-ink text-sm">
                          Click to upload or drag & drop
                        </p>
                        <p className="text-xs text-muted mt-1">
                          Supported files: Markdown (.md, .markdown, .txt)
                        </p>
                      </div>
                    </div>
                  )}
                </div>
              )}

              {/* Action Button */}
              <div className="mt-6 pt-4 border-t border-hairline-soft flex items-center justify-between">
                <span className="text-xs text-muted">
                  Strict Zero-Footprint: Auto-deleted post-download
                </span>

                <button
                  onClick={handleConvert}
                  disabled={converting}
                  className="h-12 px-6 bg-rausch hover:bg-rausch-active disabled:bg-rausch-disabled text-white font-semibold text-sm rounded-sm flex items-center gap-2 cursor-pointer transition-all shadow-xs"
                >
                  <Download size={16} />
                  <span>{converting ? 'Typesetting PDF...' : 'Convert & Download PDF'}</span>
                </button>
              </div>
            </div>
          </div>

          {/* Sidebar / Parameters Card (1 Col) */}
          <div className="space-y-6">
            <div className="bg-white rounded-md shadow-airbnb p-6 border border-hairline-soft">
              <div className="flex items-center gap-2 mb-4 pb-3 border-b border-hairline-soft">
                <Sliders size={18} className="text-rausch" />
                <h3 className="text-sm font-bold text-ink uppercase tracking-wider">
                  Pandoc Preset Specs
                </h3>
              </div>

              <div className="space-y-3.5 text-xs">
                <div className="flex justify-between py-1 border-b border-hairline-soft/60">
                  <span className="text-muted">PDF Engine:</span>
                  <span className="font-mono font-semibold text-ink">pdflatex</span>
                </div>
                <div className="flex justify-between py-1 border-b border-hairline-soft/60">
                  <span className="text-muted">Paper Size:</span>
                  <span className="font-mono font-semibold text-ink">A4</span>
                </div>
                <div className="flex justify-between py-1 border-b border-hairline-soft/60">
                  <span className="text-muted">Page Margins:</span>
                  <span className="font-mono text-ink">top/bottom 1.8cm, left/right 2cm</span>
                </div>
                <div className="flex justify-between py-1 border-b border-hairline-soft/60">
                  <span className="text-muted">Typography Font:</span>
                  <span className="font-mono text-ink">Helvetica Neue / Standard</span>
                </div>
                <div className="flex justify-between py-1 border-b border-hairline-soft/60">
                  <span className="text-muted">Base Font Size:</span>
                  <span className="font-mono text-ink">10.5pt</span>
                </div>
                <div className="flex justify-between py-1 border-b border-hairline-soft/60">
                  <span className="text-muted">Line Height:</span>
                  <span className="font-mono text-ink">1.25</span>
                </div>
                <div className="flex justify-between py-1 border-b border-hairline-soft/60">
                  <span className="text-muted">Colorlinks:</span>
                  <span className="font-mono text-ink font-semibold">Navy Blue (true)</span>
                </div>
              </div>

              <div className="mt-6 p-3 bg-surface-soft rounded-sm text-xs text-muted leading-relaxed">
                💡 Configuration matches Section 4.3.2 of the PRD exactly for consistent executive publication standards.
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
};

import React from 'react';
import { FileText, ArrowRight, ShieldCheck, Cpu, HardDrive } from 'lucide-react';
import { Navbar } from './Navbar';

export const DashboardView: React.FC = () => {
  return (
    <div className="min-h-screen bg-surface-soft flex flex-col">
      <Navbar currentPath="/" />

      <main className="flex-1 max-w-6xl w-full mx-auto px-6 py-10">
        {/* Hero banner */}
        <div className="mb-10">
          <div className="inline-flex items-center gap-2 bg-emerald-50 text-emerald-700 text-xs font-semibold px-3 py-1 rounded-full mb-3 border border-emerald-200">
            <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
            Ephemeral Instance Active
          </div>
          <h1 className="text-3xl font-bold text-ink tracking-tight mb-2">
            Workspace Services
          </h1>
          <p className="text-muted text-base max-w-2xl">
            Choose an on-demand internal utility. All data processing occurs strictly in memory and temporary sandboxes with instant destruction upon completion.
          </p>
        </div>

        {/* Services Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {/* Pandoc Service Card */}
          <div className="bg-white rounded-md shadow-airbnb hover:shadow-airbnb-hover transition-all duration-200 p-6 flex flex-col justify-between border border-hairline-soft group">
            <div>
              <div className="flex items-start justify-between mb-4">
                <div className="w-12 h-12 rounded-sm bg-rose-50 text-rausch flex items-center justify-center">
                  <FileText size={24} />
                </div>
                <span className="bg-surface-strong text-muted text-[11px] font-semibold px-2.5 py-1 rounded-full">
                  App 1 • MVP
                </span>
              </div>

              <h2 className="text-lg font-bold text-ink mb-2 group-hover:text-rausch transition-colors">
                Markdown to PDF Converter
              </h2>
              <p className="text-sm text-body leading-relaxed mb-6">
                Converts sensitive Markdown texts and documents to publication-grade PDFs powered by <strong>Pandoc</strong> and <strong>LaTeX (pdflatex)</strong>.
              </p>
            </div>

            <div>
              <div className="pt-4 border-t border-hairline-soft mb-5 flex items-center justify-between text-xs text-muted">
                <span>Engine: pdflatex</span>
                <span>Retention: 0 seconds</span>
              </div>

              <a
                href="/pandoc"
                className="w-full h-11 bg-rausch hover:bg-rausch-active text-white text-sm font-semibold rounded-sm flex items-center justify-center gap-2 transition-colors no-underline cursor-pointer shadow-xs"
              >
                <span>Launch Converter</span>
                <ArrowRight size={16} />
              </a>
            </div>
          </div>

          {/* Instance Health & Security Card */}
          <div className="bg-white rounded-md shadow-airbnb p-6 flex flex-col justify-between border border-hairline-soft">
            <div>
              <div className="flex items-start justify-between mb-4">
                <div className="w-12 h-12 rounded-sm bg-blue-50 text-blue-600 flex items-center justify-center">
                  <ShieldCheck size={24} />
                </div>
                <span className="bg-emerald-50 text-emerald-700 text-[11px] font-semibold px-2.5 py-1 rounded-full border border-emerald-200">
                  Protected
                </span>
              </div>

              <h2 className="text-lg font-bold text-ink mb-2">
                Isolation & Privacy Policy
              </h2>
              <p className="text-sm text-body leading-relaxed mb-4">
                No database or persistent disk writes are enabled. The single password is held in memory and all active JWT sessions automatically expire in 1 hour.
              </p>
            </div>

            <div className="pt-4 border-t border-hairline-soft space-y-2 text-xs text-muted">
              <div className="flex items-center gap-2">
                <Cpu size={14} className="text-muted-soft" />
                <span>Backend: High-performance Rust Axum</span>
              </div>
              <div className="flex items-center gap-2">
                <HardDrive size={14} className="text-muted-soft" />
                <span>Disk Storage: Ephemeral RAM /tmp only</span>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
};

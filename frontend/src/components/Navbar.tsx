import React, { useState } from 'react';
import { Layers, Shield, LogOut } from 'lucide-react';
import { ActiveSessionsModal } from './ActiveSessionsModal';

interface NavbarProps {
  currentPath?: string;
  onLogout?: () => void;
}

export const Navbar: React.FC<NavbarProps> = ({ currentPath = '/', onLogout }) => {
  const [sessionsOpen, setSessionsOpen] = useState(false);

  const handleLogout = async () => {
    try {
      await fetch('/api/auth/logout', { method: 'POST' });
    } catch {
      // ignore
    }
    if (onLogout) {
      onLogout();
    } else {
      window.location.href = '/login';
    }
  };

  return (
    <>
      <header className="h-20 bg-white border-b border-hairline-soft px-6 md:px-10 flex items-center justify-between sticky top-0 z-50">
        <a href="/" className="flex items-center gap-3 no-underline text-rausch font-bold text-lg">
          <div className="w-9 h-9 rounded-full bg-rausch text-white flex items-center justify-center shadow-xs">
            <Layers size={20} />
          </div>
          <span className="text-ink tracking-tight font-semibold">Disposable Workspace</span>
          <span className="hidden sm:inline-block bg-surface-strong text-muted text-xs font-semibold px-2.5 py-0.5 rounded-full">
            Zero-Footprint
          </span>
        </a>

        <nav className="flex items-center gap-8 h-full">
          <a
            href="/"
            className={`flex items-center h-full px-1 text-[15px] font-medium transition-colors relative ${
              currentPath === '/'
                ? 'text-ink font-semibold after:content-[""] after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-ink'
                : 'text-muted hover:text-ink'
            }`}
          >
            Dashboard
          </a>
          <a
            href="/pandoc"
            className={`flex items-center h-full px-1 text-[15px] font-medium transition-colors relative ${
              currentPath === '/pandoc'
                ? 'text-ink font-semibold after:content-[""] after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-ink'
                : 'text-muted hover:text-ink'
            }`}
          >
            Pandoc PDF
          </a>
        </nav>

        <div className="flex items-center gap-2 sm:gap-3">
          <button
            className="flex items-center gap-1.5 px-3 py-2 text-sm font-medium text-ink rounded-full hover:bg-surface-strong transition-colors cursor-pointer"
            onClick={() => setSessionsOpen(true)}
            title="Inspect Active Sessions"
          >
            <Shield size={16} className="text-muted" />
            <span className="hidden sm:inline">Active Sessions</span>
          </button>

          <button
            className="flex items-center gap-1.5 h-10 px-3.5 text-sm font-semibold text-ink bg-white border border-hairline hover:border-ink hover:bg-surface-soft rounded-sm transition-all cursor-pointer"
            onClick={handleLogout}
          >
            <LogOut size={15} />
            <span>Sign Out</span>
          </button>
        </div>
      </header>

      <ActiveSessionsModal
        isOpen={sessionsOpen}
        onClose={() => setSessionsOpen(false)}
        onLogout={handleLogout}
      />
    </>
  );
};

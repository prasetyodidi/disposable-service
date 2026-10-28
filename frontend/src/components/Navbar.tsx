import React, { useState } from 'react';
import { Layers, Shield, LogOut } from 'lucide-react';
import { ActiveSessionsModal } from './ActiveSessionsModal';
import { ThemeToggle } from './ThemeToggle';

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
      <header className="h-20 bg-white dark:bg-dark-card border-b border-hairline-soft dark:border-dark-hairline px-6 md:px-10 flex items-center justify-between sticky top-0 z-50 transition-colors">
        <a href="/" className="flex items-center gap-3 no-underline text-rausch font-bold text-lg">
          <div className="w-9 h-9 rounded-full bg-rausch text-white flex items-center justify-center shadow-xs">
            <Layers size={20} />
          </div>
          <span className="text-ink dark:text-dark-ink tracking-tight font-semibold">Disposable Workspace</span>
          <span className="hidden sm:inline-block bg-surface-strong dark:bg-dark-surface text-muted dark:text-dark-muted text-xs font-semibold px-2.5 py-0.5 rounded-full">
            Zero-Footprint
          </span>
        </a>

        <nav className="flex items-center gap-8 h-full">
          <a
            href="/"
            className={`flex items-center h-full px-1 text-[15px] font-medium transition-colors relative ${
              currentPath === '/'
                ? 'text-ink dark:text-dark-ink font-semibold after:content-[""] after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-ink dark:after:bg-dark-ink'
                : 'text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink'
            }`}
          >
            Dashboard
          </a>
          <a
            href="/pandoc"
            className={`flex items-center h-full px-1 text-[15px] font-medium transition-colors relative ${
              currentPath === '/pandoc'
                ? 'text-ink dark:text-dark-ink font-semibold after:content-[""] after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-ink dark:after:bg-dark-ink'
                : 'text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink'
            }`}
          >
            Pandoc PDF
          </a>
          <a
            href="/monitor"
            className={`flex items-center h-full px-1 text-[15px] font-medium transition-colors relative ${
              currentPath === '/monitor'
                ? 'text-ink dark:text-dark-ink font-semibold after:content-[""] after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-ink dark:after:bg-dark-ink'
                : 'text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink'
            }`}
          >
            Host Monitor
          </a>
        </nav>

        <div className="flex items-center gap-2 sm:gap-3">
          <ThemeToggle />

          <button
            className="flex items-center gap-1.5 px-3 py-2 text-sm font-medium text-ink dark:text-dark-ink rounded-full hover:bg-surface-strong dark:hover:bg-dark-surface transition-colors cursor-pointer"
            onClick={() => setSessionsOpen(true)}
            title="Inspect Active Sessions"
          >
            <Shield size={16} className="text-muted dark:text-dark-muted" />
            <span className="hidden sm:inline">Active Sessions</span>
          </button>

          <button
            className="flex items-center gap-1.5 h-10 px-3.5 text-sm font-semibold text-ink dark:text-dark-ink bg-white dark:bg-dark-surface border border-hairline dark:border-dark-border hover:border-ink dark:hover:border-dark-ink hover:bg-surface-soft dark:hover:bg-dark-surface-hover rounded-sm transition-all cursor-pointer"
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

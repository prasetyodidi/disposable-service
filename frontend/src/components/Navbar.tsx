import React, { useEffect, useRef, useState } from 'react';
import { Layers, Shield, LogOut, Menu, X, UserRound } from 'lucide-react';
import { ActiveSessionsModal } from './ActiveSessionsModal';
import { ThemeToggle } from './ThemeToggle';
import { BurnButton } from './BurnButton';

interface NavbarProps {
  currentPath?: string;
  onLogout?: () => void;
}

const NAV_LINKS = [
  { href: '/', label: 'Dashboard' },
  { href: '/pandoc', label: 'Pandoc PDF' },
  { href: '/monitor', label: 'Host Monitor' },
  { href: '/scrubber', label: 'Metadata Scrubber' },
  { href: '/redactor', label: 'Secret Redactor' },
] as const;

const iconButton =
  'inline-flex items-center justify-center gap-1.5 h-10 min-w-10 px-2.5 rounded-sm text-sm font-medium text-ink dark:text-dark-ink hover:bg-surface-strong dark:hover:bg-dark-surface transition-colors cursor-pointer';

export const Navbar: React.FC<NavbarProps> = ({ currentPath = '/', onLogout }) => {
  const [sessionsOpen, setSessionsOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [accountOpen, setAccountOpen] = useState(false);
  const accountRef = useRef<HTMLDivElement>(null);

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

  // Close the account menu on outside click or Escape.
  useEffect(() => {
    if (!accountOpen) return;
    const onPointer = (e: MouseEvent) => {
      if (accountRef.current && !accountRef.current.contains(e.target as Node)) {
        setAccountOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setAccountOpen(false);
    };
    document.addEventListener('mousedown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [accountOpen]);

  const linkClass = (href: string) =>
    currentPath === href
      ? 'text-ink dark:text-dark-ink font-semibold'
      : 'text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink';

  return (
    <>
      <header className="sticky top-0 z-50 bg-white dark:bg-dark-card border-b border-hairline-soft dark:border-dark-hairline transition-colors">
        <div className="h-16 px-4 md:px-8 flex items-center justify-between gap-4">
          <a
            href="/"
            className="flex items-center gap-2.5 no-underline shrink-0"
            aria-label="Disposable Workspace, home"
          >
            <span className="w-8 h-8 rounded-full bg-rausch text-white flex items-center justify-center">
              <Layers size={18} />
            </span>
            <span className="text-ink dark:text-dark-ink tracking-tight font-semibold text-base hidden sm:inline">
              Disposable Workspace
            </span>
          </a>

          <nav aria-label="Primary" className="hidden lg:flex items-center gap-6 h-full">
            {NAV_LINKS.map((l) => (
              <a
                key={l.href}
                href={l.href}
                aria-current={currentPath === l.href ? 'page' : undefined}
                className={`relative flex items-center h-full text-sm font-medium whitespace-nowrap transition-colors ${linkClass(l.href)} ${
                  currentPath === l.href
                    ? 'after:content-[""] after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-ink dark:after:bg-dark-ink'
                    : ''
                }`}
              >
                {l.label}
              </a>
            ))}
          </nav>

          <div className="flex items-center gap-1.5 sm:gap-2">
            <ThemeToggle />
            <BurnButton />
            <div className="relative" ref={accountRef}>
              <button
                type="button"
                className={iconButton}
                aria-haspopup="menu"
                aria-expanded={accountOpen}
                aria-label="Account menu"
                onClick={() => setAccountOpen((o) => !o)}
              >
                <UserRound size={18} />
              </button>
              {accountOpen && (
                <div
                  role="menu"
                  className="absolute right-0 mt-2 w-52 bg-white dark:bg-dark-card border border-hairline-soft dark:border-dark-hairline rounded-sm shadow-airbnb py-1 z-50"
                >
                  <button
                    type="button"
                    role="menuitem"
                    className="w-full flex items-center gap-2 px-3 h-10 text-sm text-ink dark:text-dark-ink hover:bg-surface-soft dark:hover:bg-dark-surface-hover text-left cursor-pointer"
                    onClick={() => {
                      setAccountOpen(false);
                      setSessionsOpen(true);
                    }}
                  >
                    <Shield size={16} className="text-muted dark:text-dark-muted" />
                    Active Sessions
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    className="w-full flex items-center gap-2 px-3 h-10 text-sm text-ink dark:text-dark-ink hover:bg-surface-soft dark:hover:bg-dark-surface-hover text-left cursor-pointer"
                    onClick={handleLogout}
                  >
                    <LogOut size={16} className="text-muted dark:text-dark-muted" />
                    Sign Out
                  </button>
                </div>
              )}
            </div>
            <button
              type="button"
              className={`${iconButton} lg:hidden`}
              aria-expanded={menuOpen}
              aria-controls="mobile-nav"
              onClick={() => setMenuOpen((o) => !o)}
            >
              {menuOpen ? <X size={18} /> : <Menu size={18} />}
              <span>Menu</span>
            </button>
          </div>
        </div>

        {menuOpen && (
          <nav
            id="mobile-nav"
            aria-label="Primary"
            className="lg:hidden border-t border-hairline-soft dark:border-dark-hairline bg-white dark:bg-dark-card px-4 py-2"
          >
            {NAV_LINKS.map((l) => (
              <a
                key={l.href}
                href={l.href}
                aria-current={currentPath === l.href ? 'page' : undefined}
                className={`flex items-center h-11 px-2 text-[15px] rounded-sm ${linkClass(l.href)} ${
                  currentPath === l.href ? 'bg-surface-soft dark:bg-dark-surface' : ''
                }`}
              >
                {l.label}
              </a>
            ))}
          </nav>
        )}
      </header>
      <ActiveSessionsModal
        isOpen={sessionsOpen}
        onClose={() => setSessionsOpen(false)}
        onLogout={handleLogout}
      />
    </>
  );
};

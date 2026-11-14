import React, { useCallback, useEffect, useState } from 'react';
import { EyeOff } from 'lucide-react';
import { PageShell } from './PageShell';
import { JwtInspectorTab } from './JwtInspectorTab';
import { LogRedactorTab } from './LogRedactorTab';

type TabId = 'jwt' | 'redactor';

const TABS: { id: TabId; label: string }[] = [
  { id: 'jwt', label: 'JWT Inspector' },
  { id: 'redactor', label: 'Log & .env Redactor' },
];

export const RedactorPage: React.FC = () => {
  const [active, setActive] = useState<TabId>('jwt');
  // Mengubah key me-remount kedua tab sehingga seluruh state internal (token, secret, teks) dikosongkan.
  const [resetKey, setResetKey] = useState(0);

  const clearAll = useCallback(() => setResetKey((k) => k + 1), []);

  // Cleanup saat halaman ditutup/disembunyikan; unmount ditangani cleanup tiap tab.
  // Tidak memakai localStorage, sessionStorage, atau cookie.
  useEffect(() => {
    window.addEventListener('beforeunload', clearAll);
    window.addEventListener('pagehide', clearAll);
    return () => {
      window.removeEventListener('beforeunload', clearAll);
      window.removeEventListener('pagehide', clearAll);
      clearAll();
    };
  }, [clearAll]);

  return (
    <PageShell currentPath="/redactor">
        <div className="mb-8">
          <div className="flex items-center gap-2 text-xs font-semibold text-rausch uppercase tracking-wider mb-1">
            <EyeOff size={14} />
            <span>Privacy Utility</span>
          </div>
          <h1 className="text-3xl font-bold text-ink dark:text-dark-ink tracking-tight mb-2">Secret Redactor</h1>
          <p className="text-sm text-muted dark:text-dark-muted">
            Inspect JWTs and redact secrets from logs and .env files. Nothing is stored in this browser.
          </p>
        </div>

        <div role="tablist" aria-label="Redactor tools" className="flex gap-6 border-b border-hairline dark:border-dark-border mb-6">
          {TABS.map((tab) => {
            const selected = active === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                id={`tab-${tab.id}`}
                aria-selected={selected}
                aria-controls={`panel-${tab.id}`}
                onClick={() => setActive(tab.id)}
                className={`h-11 px-1 text-sm font-medium cursor-pointer relative transition-colors ${
                  selected
                    ? 'text-ink dark:text-dark-ink font-semibold after:content-[""] after:absolute after:bottom-0 after:left-0 after:right-0 after:h-[2px] after:bg-ink dark:after:bg-dark-ink'
                    : 'text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink'
                }`}
              >
                {tab.label}
              </button>
            );
          })}
        </div>

        <div key={resetKey}>
          <div role="tabpanel" id="panel-jwt" aria-labelledby="tab-jwt" hidden={active !== 'jwt'}>
            <JwtInspectorTab />
          </div>
          <div role="tabpanel" id="panel-redactor" aria-labelledby="tab-redactor" hidden={active !== 'redactor'}>
            <LogRedactorTab />
          </div>
        </div>
      </PageShell>
  );
};

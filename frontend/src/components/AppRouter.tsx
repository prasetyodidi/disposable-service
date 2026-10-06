import React, { useEffect, useRef, useState } from 'react';
import { DashboardView } from './DashboardView';
import { PandocConverter } from './PandocConverter';
import { SetupCard } from './SetupCard';
import { LoginCard } from './LoginCard';

interface AppRouterProps {
  page: 'dashboard' | 'pandoc' | 'setup' | 'login';
}

// Peta page prop → pathname yang diharapkan
const PAGE_PATH: Record<AppRouterProps['page'], string> = {
  dashboard: '/',
  pandoc: '/pandoc',
  setup: '/setup',
  login: '/login',
};

export const AppRouter: React.FC<AppRouterProps> = ({ page }) => {
  const [checking, setChecking] = useState(true);
  // Ref guard: abort controller mencegah double-invocation React StrictMode
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    // Batalkan fetch sebelumnya (penting untuk React StrictMode & re-render cepat)
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setChecking(true);

    const checkAuth = async () => {
      try {
        const res = await fetch('/api/auth/status', { signal: controller.signal });

        if (!res.ok) {
          console.warn(`[AppRouter] /api/auth/status → HTTP ${res.status}. Rendering page as-is.`);
          return;
        }

        const data: { is_claimed: boolean; is_authenticated: boolean } = await res.json();
        console.log('[AppRouter] status:', data, '| prop page:', page, '| pathname:', window.location.pathname);

        if (controller.signal.aborted) return;

        // Gunakan pathname aktual sebagai sumber kebenaran (bukan hanya prop)
        const currentPath = window.location.pathname;

        // Kasus 1: belum di-claim → harus ke /setup
        if (!data.is_claimed) {
          if (currentPath !== '/setup') {
            console.log('[AppRouter] → redirect /setup');
            window.location.replace('/setup');
          } else {
            console.log('[AppRouter] → sudah di /setup, render SetupCard');
          }
          return;
        }

        // Kasus 2: sudah di-claim, belum login → harus ke /login
        if (!data.is_authenticated) {
          if (currentPath !== '/login') {
            console.log('[AppRouter] → redirect /login');
            window.location.replace('/login');
          } else {
            console.log('[AppRouter] → sudah di /login, render LoginCard');
          }
          return;
        }

        // Kasus 3: sudah login, masih di halaman auth → ke dashboard
        if (currentPath === '/login' || currentPath === '/setup') {
          console.log('[AppRouter] → authenticated, redirect /');
          window.location.replace('/');
          return;
        }

        // Semua kondisi terpenuhi, render halaman
        console.log('[AppRouter] → auth OK, render page:', page);
      } catch (err: unknown) {
        if (err instanceof Error && err.name === 'AbortError') {
          console.log('[AppRouter] fetch aborted (StrictMode cleanup or re-render)');
          return; // Jangan panggil setChecking — effect baru akan mengurus ini
        }
        console.error('[AppRouter] fetch error:', err);
      } finally {
        // Hanya set checking=false jika fetch ini tidak di-abort
        if (!controller.signal.aborted) {
          setChecking(false);
        }
      }
    };

    checkAuth();

    // Cleanup: abort fetch jika komponen unmount atau page berubah
    return () => {
      controller.abort();
    };
  }, [page]);

  if (checking) {
    return (
      <div className="min-h-screen bg-surface-soft flex items-center justify-center">
        <div className="text-center space-y-3">
          <div className="w-10 h-10 border-4 border-rausch border-t-transparent rounded-full animate-spin mx-auto"></div>
          <p className="text-xs font-semibold text-muted uppercase tracking-wider">
            Verifying Workspace Environment...
          </p>
        </div>
      </div>
    );
  }

  switch (page) {
    case 'setup':
      return <SetupCard />;
    case 'login':
      return <LoginCard />;
    case 'pandoc':
      return <PandocConverter />;
    case 'dashboard':
    default:
      return <DashboardView />;
  }
};


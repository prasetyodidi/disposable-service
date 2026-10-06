import React, { useEffect, useRef, useState } from 'react';
import { DashboardView } from './DashboardView';
import { PandocConverter } from './PandocConverter';
import { SetupCard } from './SetupCard';
import { LoginCard } from './LoginCard';

interface AppRouterProps {
  page: 'dashboard' | 'pandoc' | 'setup' | 'login';
}

export const AppRouter: React.FC<AppRouterProps> = ({ page }) => {
  const [checking, setChecking] = useState(true);
  // Guard: pastikan redirect hanya terjadi sekali per mount
  const redirecting = useRef(false);

  useEffect(() => {
    // Reset guard tiap kali page berubah
    redirecting.current = false;

    const checkAuth = async () => {
      try {
        const res = await fetch('/api/auth/status');
        if (!res.ok) return;

        const data: { is_claimed: boolean; is_authenticated: boolean } = await res.json();

        // Sudah terjadi redirect sebelumnya — abaikan
        if (redirecting.current) return;

        // Kasus 1: instance belum di-claim → wajib ke /setup
        if (!data.is_claimed) {
          if (page !== 'setup') {
            redirecting.current = true;
            window.location.replace('/setup');
          }
          return;
        }

        // Kasus 2: instance sudah di-claim, tapi belum login → wajib ke /login
        if (!data.is_authenticated) {
          if (page !== 'login') {
            redirecting.current = true;
            window.location.replace('/login');
          }
          return;
        }

        // Kasus 3: sudah login, tapi masih di halaman auth → ke dashboard
        if (page === 'login' || page === 'setup') {
          redirecting.current = true;
          window.location.replace('/');
        }
      } catch {
        // network or server offline — biarkan render halaman apa adanya
      } finally {
        if (!redirecting.current) {
          setChecking(false);
        }
      }
    };

    checkAuth();
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

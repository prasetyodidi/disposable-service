import React, { useEffect, useState } from 'react';
import { DashboardView } from './DashboardView';
import { PandocConverter } from './PandocConverter';
import { SetupCard } from './SetupCard';
import { LoginCard } from './LoginCard';

interface AppRouterProps {
  page: 'dashboard' | 'pandoc' | 'setup' | 'login';
}

export const AppRouter: React.FC<AppRouterProps> = ({ page }) => {
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    const checkAuth = async () => {
      try {
        const res = await fetch('/api/auth/status');
        if (res.ok) {
          const data = await res.json();
          if (!data.is_claimed && page !== 'setup') {
            window.location.href = '/setup';
            return;
          }
          if (data.is_claimed && !data.is_authenticated && page !== 'login') {
            window.location.href = '/login';
            return;
          }
          if (data.is_authenticated && (page === 'login' || page === 'setup')) {
            window.location.href = '/';
            return;
          }
        }
      } catch {
        // network or server offline
      } finally {
        setChecking(false);
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

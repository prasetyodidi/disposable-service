import React, { useEffect, useState } from 'react';

type AuthStatus = {
  is_claimed: boolean;
  is_authenticated: boolean;
};

export const AppRouter: React.FC<AppRouterProps> = ({ page }) => {
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    let redirecting = false;

    const checkAuth = async () => {
      setChecking(true);
      setError(null);

      try {
        const res = await fetch('/api/auth/status', {
          signal: controller.signal,
          cache: 'no-store',
        });

        if (!res.ok) {
          throw new Error(`Auth status failed: ${res.status}`);
        }

        const data = (await res.json()) as Partial<AuthStatus>;

        if (
          typeof data.is_claimed !== 'boolean' ||
          typeof data.is_authenticated !== 'boolean'
        ) {
          throw new Error('Invalid auth status response');
        }

        const redirect = (path: string) => {
          redirecting = true;
          window.location.replace(path);
        };

        if (!data.is_claimed) {
          if (page !== 'setup') redirect('/setup');
        } else if (!data.is_authenticated) {
          if (page !== 'login') redirect('/login');
        } else if (page === 'login' || page === 'setup') {
          redirect('/');
        }
      } catch (err) {
        if (controller.signal.aborted) return;
        setError('Gagal memverifikasi status autentikasi.');
      } finally {
        if (!redirecting && !controller.signal.aborted) {
          setChecking(false);
        }
      }
    };

    checkAuth();

    return () => controller.abort();
  }, [page]);

  if (error) {
    return <div>{error}</div>; // ganti dengan UI error
  }

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
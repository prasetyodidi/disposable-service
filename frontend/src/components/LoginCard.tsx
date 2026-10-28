import React, { useState } from 'react';
import { Lock, ArrowRight, ShieldAlert } from 'lucide-react';
import { ThemeToggle } from './ThemeToggle';

export const LoginCard: React.FC = () => {
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    try {
      const res = await fetch('/api/auth/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ password }),
      });

      const data = await res.json();
      if (res.ok && data.success) {
        window.location.href = '/';
      } else {
        setError(data.message || 'Invalid password provided');
      }
    } catch {
      setError('Network communication error with server');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex flex-col items-center justify-center p-6 bg-surface-soft dark:bg-dark-canvas transition-colors relative">
      <div className="absolute top-6 right-6">
        <ThemeToggle />
      </div>

      <div className="max-w-md w-full">
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-14 h-14 rounded-full bg-rausch text-white mb-4 shadow-sm">
            <Lock size={26} />
          </div>
          <h1 className="text-2xl font-bold text-ink dark:text-dark-ink tracking-tight mb-2">
            Disposable Workspace
          </h1>
          <p className="text-sm text-muted dark:text-dark-muted">
            Enter your instance Single Password to access active tools.
          </p>
        </div>

        <div className="bg-white dark:bg-dark-card border border-hairline-soft dark:border-dark-hairline rounded-md shadow-airbnb p-8">
          {error && (
            <div className="bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-sm mb-5 flex items-center gap-2">
              <ShieldAlert size={18} className="shrink-0" />
              <span>{error}</span>
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-5">
            <div>
              <label className="block text-xs font-semibold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">
                Single Password
              </label>
              <input
                type="password"
                className="w-full h-13 px-4 bg-white dark:bg-dark-surface border border-hairline dark:border-dark-border rounded-sm text-ink dark:text-dark-ink placeholder:text-muted dark:placeholder:text-dark-muted-soft focus:border-ink dark:focus:border-dark-ink focus:ring-0 outline-none transition-all text-sm"
                placeholder="Enter password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                autoFocus
              />
            </div>

            <p className="text-xs text-muted dark:text-dark-muted leading-relaxed">
              Sessions expire automatically after <strong>1 hour</strong> of activity.
            </p>

            <button
              type="submit"
              className="w-full h-12 bg-rausch hover:bg-rausch-active disabled:bg-rausch-disabled text-white font-semibold rounded-sm flex items-center justify-center gap-2 cursor-pointer transition-colors shadow-xs"
              disabled={loading}
            >
              <span>{loading ? 'Authenticating...' : 'Sign In'}</span>
              <ArrowRight size={18} />
            </button>
          </form>
        </div>
      </div>
    </div>
  );
};

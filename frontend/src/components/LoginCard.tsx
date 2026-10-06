import React, { useState } from 'react';
import { Lock, ArrowRight, ShieldAlert } from 'lucide-react';

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
    <div className="min-h-screen flex flex-col items-center justify-center p-6 bg-surface-soft">
      <div className="max-w-md w-full">
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-14 h-14 rounded-full bg-rausch text-white mb-4 shadow-sm">
            <Lock size={26} />
          </div>
          <h1 className="text-2xl font-bold text-ink tracking-tight mb-2">
            Disposable Workspace
          </h1>
          <p className="text-sm text-muted">
            Enter your instance Single Password to access active tools.
          </p>
        </div>

        <div className="bg-white rounded-md shadow-airbnb p-8">
          {error && (
            <div className="bg-red-50 border border-red-200 text-airbnb-error px-4 py-3 rounded-sm text-sm mb-5 flex items-center gap-2">
              <ShieldAlert size={18} className="shrink-0" />
              <span>{error}</span>
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-5">
            <div>
              <label className="block text-xs font-semibold text-ink uppercase tracking-wider mb-2">
                Single Password
              </label>
              <input
                type="password"
                className="w-full h-13 px-4 bg-white border border-hairline rounded-sm text-ink placeholder:text-muted focus:border-ink focus:ring-0 outline-none transition-all text-sm"
                placeholder="Enter password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                autoFocus
              />
            </div>

            <p className="text-xs text-muted leading-relaxed">
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

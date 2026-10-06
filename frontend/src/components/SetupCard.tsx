import React, { useState } from 'react';
import { KeyRound, ShieldAlert, ArrowRight, CheckCircle2 } from 'lucide-react';

export const SetupCard: React.FC = () => {
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (password.length < 6) {
      setError('Password must be at least 6 characters long');
      return;
    }

    if (password !== confirmPassword) {
      setError('Passwords do not match');
      return;
    }

    setLoading(true);
    try {
      const res = await fetch('/api/auth/setup', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ password }),
      });

      const data = await res.json();
      if (res.ok && data.success) {
        // Auto login with new password
        const loginRes = await fetch('/api/auth/login', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ password }),
        });
        if (loginRes.ok) {
          window.location.href = '/';
        } else {
          window.location.href = '/login';
        }
      } else {
        setError(data.message || 'Failed to initialize password');
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
            <KeyRound size={28} />
          </div>
          <h1 className="text-2xl font-bold text-ink tracking-tight mb-2">
            Claim Your Workspace
          </h1>
          <p className="text-sm text-muted leading-relaxed">
            This ephemeral instance has just been provisioned. Initialize your <strong>Single Access Password</strong> to secure all services.
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
                Create Single Password
              </label>
              <input
                type="password"
                className="w-full h-13 px-4 bg-white border border-hairline rounded-sm text-ink placeholder:text-muted focus:border-ink focus:ring-0 outline-none transition-all text-sm"
                placeholder="Choose a strong password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                autoFocus
              />
            </div>

            <div>
              <label className="block text-xs font-semibold text-ink uppercase tracking-wider mb-2">
                Confirm Password
              </label>
              <input
                type="password"
                className="w-full h-13 px-4 bg-white border border-hairline rounded-sm text-ink placeholder:text-muted focus:border-ink focus:ring-0 outline-none transition-all text-sm"
                placeholder="Re-enter your password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                required
              />
            </div>

            <div className="flex gap-2.5 items-start bg-surface-soft p-3 rounded-sm">
              <CheckCircle2 size={16} className="text-airbnb-success shrink-0 mt-0.5" />
              <span className="text-xs text-muted leading-snug">
                This password acts as the sole access key for this disposable instance until it is destroyed.
              </span>
            </div>

            <button
              type="submit"
              className="w-full h-12 bg-rausch hover:bg-rausch-active disabled:bg-rausch-disabled text-white font-semibold rounded-sm flex items-center justify-center gap-2 cursor-pointer transition-colors shadow-xs"
              disabled={loading}
            >
              <span>{loading ? 'Initializing...' : 'Claim & Continue'}</span>
              <ArrowRight size={18} />
            </button>
          </form>
        </div>
      </div>
    </div>
  );
};

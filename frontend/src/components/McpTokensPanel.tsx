import React, { useCallback, useEffect, useState } from 'react';
import { KeyRound, Copy, Check, AlertCircle, X } from 'lucide-react';
import { PRIMARY_BUTTON } from '../lib/ui';

interface SessionRow {
  session_id: string;
  user_agent: string;
  login_time: number;
  expires_at: number;
  token_type?: string;
  label?: string;
}

interface CreatedToken {
  token: string;
  expires_at: number;
  label: string;
}

const LIFETIMES = [
  { days: 1, label: '1 day' },
  { days: 7, label: '7 days' },
  { days: 30, label: '30 days' },
  { days: 90, label: '90 days' },
  { days: 365, label: '365 days' },
];

const MAX_LABEL = 64;

const fmtDate = (epochSeconds: number) =>
  new Date(epochSeconds * 1000).toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });

export function buildClientConfig(origin: string, token: string): string {
  return JSON.stringify(
    {
      mcpServers: {
        'disposable-service': {
          url: `${origin}/api/mcp/sse`,
          headers: { Authorization: `Bearer ${token}` },
        },
      },
    },
    null,
    2,
  );
}

export const McpTokensPanel: React.FC = () => {
  const [label, setLabel] = useState('');
  const [days, setDays] = useState(30);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<CreatedToken | null>(null);
  const [copied, setCopied] = useState<'token' | 'config' | null>(null);
  const [tokens, setTokens] = useState<SessionRow[]>([]);

  const loadTokens = useCallback(async () => {
    try {
      const res = await fetch('/api/sessions');
      if (!res.ok) return;
      const data = await res.json();
      const rows = ((data.sessions ?? []) as SessionRow[]).filter(
        (s) => s.token_type === 'mcp_agent' || s.user_agent?.startsWith('MCP Agent'),
      );
      setTokens(rows);
    } catch {
      // The list is informational; ignore network errors here.
    }
  }, []);

  useEffect(() => {
    void loadTokens();
  }, [loadTokens]);

  // Never keep the secret around longer than the page.
  useEffect(() => () => setCreated(null), []);

  const create = async () => {
    setError(null);
    setBusy(true);
    try {
      const res = await fetch('/api/auth/token', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          lifetime_seconds: days * 86400,
          label: label.trim() || undefined,
        }),
      });
      if (res.status === 401) {
        window.location.href = '/login';
        return;
      }
      const data = await res.json().catch(() => null);
      if (!res.ok || !data?.token) {
        throw new Error(data?.message || `Could not create token (status ${res.status})`);
      }
      setCreated({ token: data.token, expires_at: data.expires_at, label: label.trim() });
      setLabel('');
      void loadTokens();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not create token');
    } finally {
      setBusy(false);
    }
  };

  const copy = async (kind: 'token' | 'config', text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(kind);
      window.setTimeout(() => setCopied((c) => (c === kind ? null : c)), 2000);
    } catch {
      setError('Copy failed. Select the text and copy it manually.');
    }
  };

  const config = created ? buildClientConfig(window.location.origin, created.token) : '';

  return (
    <section
      aria-labelledby="mcp-tokens-title"
      className="mt-10 bg-white dark:bg-dark-card rounded-md shadow-airbnb border border-hairline-soft dark:border-dark-hairline p-6"
    >
      <div className="flex items-center gap-2 mb-1">
        <KeyRound size={18} className="text-rausch" />
        <h2 id="mcp-tokens-title" className="text-lg font-bold text-ink dark:text-dark-ink">
          MCP access tokens
        </h2>
      </div>
      <p className="text-sm text-muted dark:text-dark-muted mb-5">
        Create a token for an MCP client such as Kiro, Claude Desktop or Cursor. The token is shown once.
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-[1fr_auto_auto] gap-3 items-end">
        <label className="block">
          <span className="block text-xs font-semibold uppercase tracking-wider text-ink dark:text-dark-ink mb-1.5">
            Label
          </span>
          <input
            value={label}
            maxLength={MAX_LABEL}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="e.g. Kiro on my laptop"
            className="w-full h-12 px-3 rounded-sm border border-hairline dark:border-dark-border bg-white dark:bg-dark-surface text-sm text-ink dark:text-dark-ink"
          />
        </label>
        <label className="block">
          <span className="block text-xs font-semibold uppercase tracking-wider text-ink dark:text-dark-ink mb-1.5">
            Valid for
          </span>
          <select
            value={days}
            onChange={(e) => setDays(Number(e.target.value))}
            className="w-full sm:w-36 h-12 px-3 rounded-sm border border-hairline dark:border-dark-border bg-white dark:bg-dark-surface text-sm text-ink dark:text-dark-ink"
          >
            {LIFETIMES.map((l) => (
              <option key={l.days} value={l.days}>
                {l.label}
              </option>
            ))}
          </select>
        </label>
        <button type="button" onClick={create} disabled={busy} className={PRIMARY_BUTTON}>
          <KeyRound size={16} />
          <span>{busy ? 'Creating...' : 'Create token'}</span>
        </button>
      </div>

      {error && (
        <div
          role="alert"
          className="mt-4 bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-sm flex items-start gap-2.5"
        >
          <AlertCircle size={18} className="shrink-0 mt-0.5" />
          <span className="flex-1 text-xs">{error}</span>
        </div>
      )}

      {created && (
        <div className="mt-5 border border-hairline dark:border-dark-border rounded-sm p-4 bg-surface-soft dark:bg-dark-surface">
          <div className="flex items-start justify-between gap-3 mb-3">
            <p className="text-sm font-semibold text-ink dark:text-dark-ink">
              Copy it now. You will not be able to see this token again.
            </p>
            <button
              type="button"
              aria-label="Dismiss token"
              onClick={() => setCreated(null)}
              className="text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink cursor-pointer"
            >
              <X size={16} />
            </button>
          </div>
          <p className="text-xs text-muted dark:text-dark-muted mb-1.5">Token (expires {fmtDate(created.expires_at)})</p>
          <div className="flex items-stretch gap-2 mb-4">
            <code
              data-testid="created-token"
              className="flex-1 min-w-0 break-all font-mono text-xs px-3 py-2 rounded-sm border border-hairline dark:border-dark-border bg-white dark:bg-dark-card text-ink dark:text-dark-ink"
            >
              {created.token}
            </code>
            <button
              type="button"
              onClick={() => copy('token', created.token)}
              className="shrink-0 inline-flex items-center gap-1.5 h-10 px-3 rounded-sm border border-hairline dark:border-dark-border text-sm font-medium text-ink dark:text-dark-ink hover:bg-surface-strong dark:hover:bg-dark-surface-hover cursor-pointer"
            >
              {copied === 'token' ? <Check size={14} /> : <Copy size={14} />}
              {copied === 'token' ? 'Copied' : 'Copy'}
            </button>
          </div>
          <p className="text-xs text-muted dark:text-dark-muted mb-1.5">Client configuration</p>
          <div className="relative">
            <pre className="font-mono text-xs overflow-x-auto px-3 py-2 rounded-sm border border-hairline dark:border-dark-border bg-white dark:bg-dark-card text-ink dark:text-dark-ink">
              {config}
            </pre>
            <button
              type="button"
              onClick={() => copy('config', config)}
              className="absolute top-2 right-2 inline-flex items-center gap-1.5 h-8 px-2.5 rounded-sm border border-hairline dark:border-dark-border text-xs font-medium text-ink dark:text-dark-ink bg-white dark:bg-dark-surface hover:bg-surface-strong dark:hover:bg-dark-surface-hover cursor-pointer"
            >
              {copied === 'config' ? <Check size={12} /> : <Copy size={12} />}
              {copied === 'config' ? 'Copied' : 'Copy config'}
            </button>
          </div>
        </div>
      )}

      <div className="mt-6">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-ink dark:text-dark-ink mb-2">
          Active tokens
        </h3>
        {tokens.length === 0 ? (
          <p className="text-sm text-muted dark:text-dark-muted">No MCP tokens are active on this instance.</p>
        ) : (
          <ul className="divide-y divide-hairline-soft dark:divide-dark-hairline border border-hairline-soft dark:border-dark-hairline rounded-sm">
            {tokens.map((t) => (
              <li key={t.session_id} className="flex flex-wrap items-center justify-between gap-2 px-3 py-2.5 text-sm">
                <span className="font-medium text-ink dark:text-dark-ink">{t.label || 'MCP Agent'}</span>
                <span className="text-xs text-muted dark:text-dark-muted">
                  created {fmtDate(t.login_time)} · expires {fmtDate(t.expires_at)}
                </span>
              </li>
            ))}
          </ul>
        )}
        <p className="mt-3 text-xs text-muted dark:text-dark-muted leading-relaxed">
          Tokens cannot be revoked yet. A token stays valid until it expires or until the server secret (
          <code className="font-mono">JWT_SECRET</code>) changes, so use short lifetimes and treat each token like a
          password. On AWS Lambda this list only shows tokens seen by the current container.
        </p>
      </div>
    </section>
  );
};

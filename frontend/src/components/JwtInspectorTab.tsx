import React, { useEffect, useRef, useState } from 'react';
import { AlertCircle, AlertTriangle, ShieldCheck } from 'lucide-react';
import type { JwtInspectResult, SignatureCheck, TokenStatus } from '../types/privacy';
import { PRIMARY_BUTTON } from '../lib/ui';

const MAX_TOKEN_CHARS = 8192;
const MAX_SECRET_BYTES = 1024;
const GENERIC_ERROR = 'Inspection failed. The server did not respond or the connection was lost.';

const STATUS_LABEL: Record<TokenStatus, string> = {
  active: 'Active',
  expired: 'Expired',
  not_yet_valid: 'Not yet valid',
};

const SIGNATURE_LABEL: Record<SignatureCheck, string> = {
  not_requested: 'Not requested',
  valid: 'Valid',
  invalid: 'Invalid',
  unsupported: 'Verification not supported for this algorithm',
};

const STATUS_CLASS: Record<TokenStatus, string> = {
  active: 'text-emerald-800 dark:text-emerald-400',
  expired: 'text-airbnb-error dark:text-red-400',
  not_yet_valid: 'text-ink dark:text-dark-ink',
};

const SIGNATURE_CLASS: Record<SignatureCheck, string> = {
  not_requested: 'text-muted dark:text-dark-muted',
  valid: 'text-emerald-800 dark:text-emerald-400',
  invalid: 'text-airbnb-error dark:text-red-400',
  unsupported: 'text-ink dark:text-dark-ink',
};

const cardClass =
  'bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline';
const inputClass =
  'w-full px-3 py-2 rounded-sm text-sm font-mono bg-white dark:bg-dark-surface text-ink dark:text-dark-ink border border-hairline dark:border-dark-border focus:outline-none focus:border-ink dark:focus:border-dark-ink';

export const JwtInspectorTab: React.FC = () => {
  const [token, setToken] = useState('');
  const [secret, setSecret] = useState('');
  const [redact, setRedact] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<JwtInspectResult | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Cleanup saat unmount: batalkan request dan kosongkan state sensitif.
  useEffect(
    () => () => {
      abortRef.current?.abort();
      setToken('');
      setSecret('');
      setResult(null);
    },
    [],
  );

  const trimmedToken = token.trim();
  const secretBytes = new TextEncoder().encode(secret).length;

  const handleInspect = async () => {
    if (busy) return;
    setError(null);
    setResult(null);

    if (trimmedToken.length === 0 || trimmedToken.length > MAX_TOKEN_CHARS) {
      setError(`Token must be between 1 and ${MAX_TOKEN_CHARS} characters.`);
      return;
    }
    if (secret.length > 0 && secretBytes > MAX_SECRET_BYTES) {
      setError(`HMAC secret must be at most ${MAX_SECRET_BYTES} bytes.`);
      return;
    }

    setBusy(true);
    const controller = new AbortController();
    abortRef.current = controller;
    try {
      const res = await fetch('/api/apps/jwt', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          token: trimmedToken,
          secret: secret.length > 0 ? secret : null,
          redact,
        }),
        signal: controller.signal,
      });

      if (!res.ok) {
        if (res.status === 401) {
          window.location.href = '/login';
          return;
        }
        const errJson = await res.json().catch(() => null);
        const msg = errJson?.message || errJson?.error;
        setError(typeof msg === 'string' && msg ? msg : GENERIC_ERROR);
        return;
      }
      setResult((await res.json()) as JwtInspectResult);
    } catch {
      if (!controller.signal.aborted) setError(GENERIC_ERROR);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className={cardClass}>
        <div className="space-y-4">
          <div>
            <label htmlFor="jwt-token" className="block text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">
              JWT token
            </label>
            <textarea
              id="jwt-token"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              rows={5}
              spellCheck={false}
              autoComplete="off"
              placeholder="eyJhbGciOi..."
              className={`${inputClass} resize-y`}
            />
            <p className="text-xs text-muted dark:text-dark-muted mt-1">{trimmedToken.length} / {MAX_TOKEN_CHARS} characters</p>
          </div>

          <div>
            <label htmlFor="jwt-secret" className="block text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">
              HMAC secret (optional)
            </label>
            <input
              id="jwt-secret"
              type="password"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              autoComplete="off"
              spellCheck={false}
              placeholder="Verify HS256 / HS384 / HS512 signature"
              className={`${inputClass} h-10`}
            />
          </div>

          <label className="flex items-center gap-2 text-sm text-ink dark:text-dark-ink cursor-pointer">
            <input type="checkbox" checked={redact} onChange={(e) => setRedact(e.target.checked)} className="cursor-pointer" />
            <span>Redact personal claims (email, phone, sub, name) in the displayed payload</span>
          </label>
        </div>

        <div className="mt-6 pt-4 border-t border-hairline-soft dark:border-dark-hairline flex items-center justify-between">
          <span className="text-xs text-muted dark:text-dark-muted">Decoded on this server only, never sent to third parties</span>
          <button
            type="button"
            onClick={handleInspect}
            disabled={busy || trimmedToken.length === 0}
            className={PRIMARY_BUTTON}
          >
            <ShieldCheck size={16} />
            <span>{busy ? 'Inspecting...' : 'Inspect token'}</span>
          </button>
        </div>
      </div>

      {error && (
        <div
          role="alert"
          className="bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 text-airbnb-error dark:text-red-400 px-4 py-3 rounded-sm text-sm flex items-start gap-2.5"
        >
          <AlertCircle size={18} className="shrink-0 mt-0.5" />
          <div className="flex-1 text-xs">{error}</div>
        </div>
      )}

      {result && (
        <div className="space-y-6" data-testid="jwt-result">
          {result.warnings.length > 0 && (
            <div
              role="alert"
              className="bg-surface-strong dark:bg-dark-surface border border-hairline dark:border-dark-border text-ink dark:text-dark-ink px-4 py-3 rounded-sm text-sm"
            >
              <ul className="space-y-1">
                {result.warnings.map((w, i) => (
                  <li key={i} className="flex items-start gap-2 text-xs">
                    <AlertTriangle size={14} className="shrink-0 mt-0.5" />
                    <span>{w}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          <div className={cardClass}>
            <dl className="grid grid-cols-1 sm:grid-cols-2 gap-4 text-sm">
              <div>
                <dt className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-1">Status</dt>
                <dd className={`font-semibold ${STATUS_CLASS[result.status]}`}>{STATUS_LABEL[result.status]}</dd>
              </div>
              <div>
                <dt className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-1">Signature</dt>
                <dd className={`font-semibold ${SIGNATURE_CLASS[result.signature]}`}>{SIGNATURE_LABEL[result.signature]}</dd>
              </div>
            </dl>

            {result.times.length > 0 && (
              <div className="mt-4">
                <h3 className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">Time claims (UTC)</h3>
                <ul className="space-y-1.5 text-xs">
                  {result.times.map((t) => (
                    <li
                      key={t.claim}
                      className="flex justify-between gap-4 py-1 border-b border-hairline-soft/60 dark:border-dark-hairline/60"
                    >
                      <span className="font-mono text-ink dark:text-dark-ink">{t.claim}</span>
                      {t.valid && t.iso ? (
                        <span className="font-mono text-ink dark:text-dark-ink">{t.iso}</span>
                      ) : (
                        <span className="font-mono text-airbnb-error dark:text-red-400 break-all">
                          {typeof t.raw === 'string' ? t.raw : JSON.stringify(t.raw)} (invalid)
                        </span>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className={cardClass}>
              <h3 className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">Header</h3>
              <pre className="text-xs font-mono text-ink dark:text-dark-ink whitespace-pre-wrap break-all">{result.header_pretty}</pre>
            </div>
            <div className={cardClass}>
              <h3 className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">Payload</h3>
              <pre className="text-xs font-mono text-ink dark:text-dark-ink whitespace-pre-wrap break-all">{result.payload_pretty}</pre>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

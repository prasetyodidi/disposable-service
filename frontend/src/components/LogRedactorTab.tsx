import React, { useEffect, useRef, useState } from 'react';
import { AlertCircle, Check, Copy, EyeOff } from 'lucide-react';
import { validateRedactorInput, REDACTOR_MAX_CHARS } from '../lib/validateRedactorInput';

const GENERIC_ERROR = 'Redaction failed. The server did not respond or the connection was lost.';
const COPY_ERROR = 'Copy failed. Select the result text and copy it manually.';
const COPY_CONFIRM_MS = 2000;
const MAX_CUSTOM_WORDS = 100;
const MAX_WORD_CHARS = 256;

const CATEGORIES: { id: string; label: string }[] = [
  { id: 'jwt', label: 'JWT' },
  { id: 'bearer', label: 'Bearer token' },
  { id: 'api_key', label: 'API key' },
  { id: 'env_value', label: '.env value' },
  { id: 'email', label: 'Email' },
  { id: 'phone', label: 'Phone' },
  { id: 'ipv4', label: 'IPv4' },
  { id: 'ipv6', label: 'IPv6' },
  { id: 'credit_card', label: 'Credit card' },
];

interface RedactResponse {
  text: string;
  counts: Record<string, number>;
}

const cardClass =
  'bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline';
const areaClass =
  'w-full px-3 py-2 rounded-sm text-xs font-mono bg-white dark:bg-dark-surface text-ink dark:text-dark-ink border border-hairline dark:border-dark-border focus:outline-none focus:border-ink dark:focus:border-dark-ink resize-y';

export const LogRedactorTab: React.FC = () => {
  const [text, setText] = useState('');
  const [enabled, setEnabled] = useState<Record<string, boolean>>(
    () => Object.fromEntries(CATEGORIES.map((c) => [c.id, true])),
  );
  const [customRaw, setCustomRaw] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<RedactResponse | null>(null);
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      abortRef.current?.abort();
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
      setText('');
      setCustomRaw('');
      setResult(null);
    },
    [],
  );

  const handleRedact = async () => {
    if (busy) return;
    setError(null);
    setCopyError(null);
    setCopied(false);

    const validation = validateRedactorInput(text);
    if (!validation.ok) {
      // Tolak pengiriman; teks masukan dipertahankan apa adanya.
      setError(validation.error ?? 'Input too large.');
      return;
    }
    if (text.length === 0) {
      setError('Enter some text to redact.');
      return;
    }

    const customWords = customRaw
      .split('\n')
      .map((w) => w.trim())
      .filter((w) => w.length > 0);
    if (customWords.length > MAX_CUSTOM_WORDS) {
      setError(`Custom words are limited to ${MAX_CUSTOM_WORDS} entries.`);
      return;
    }
    if (customWords.some((w) => Array.from(w).length > MAX_WORD_CHARS)) {
      setError(`Each custom word is limited to ${MAX_WORD_CHARS} characters.`);
      return;
    }

    setBusy(true);
    setResult(null);
    const controller = new AbortController();
    abortRef.current = controller;
    try {
      const res = await fetch('/api/apps/redactor', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          text,
          categories: CATEGORIES.filter((c) => enabled[c.id]).map((c) => c.id),
          custom_words: customWords,
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
      setResult((await res.json()) as RedactResponse);
    } catch {
      if (!controller.signal.aborted) setError(GENERIC_ERROR);
    } finally {
      setBusy(false);
    }
  };

  const handleCopy = async () => {
    if (!result) return;
    setCopyError(null);
    if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    try {
      await navigator.clipboard.writeText(result.text);
      setCopied(true);
      copyTimerRef.current = setTimeout(() => setCopied(false), COPY_CONFIRM_MS);
    } catch {
      setCopied(false);
      setCopyError(COPY_ERROR);
    }
  };

  const countEntries = result ? Object.entries(result.counts) : [];

  return (
    <div className="space-y-6">
      <div className={cardClass}>
        <div className="space-y-4">
          <div>
            <label htmlFor="redactor-input" className="block text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">
              Text, log or .env
            </label>
            <textarea
              id="redactor-input"
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={10}
              spellCheck={false}
              autoComplete="off"
              className={areaClass}
            />
            <p
              className={`text-xs mt-1 ${
                text.length > REDACTOR_MAX_CHARS
                  ? 'text-airbnb-error dark:text-red-400'
                  : 'text-muted dark:text-dark-muted'
              }`}
            >
              {text.length.toLocaleString('en-US')} / {REDACTOR_MAX_CHARS.toLocaleString('en-US')} characters
            </p>
          </div>

          <fieldset>
            <legend className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">Categories</legend>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
              {CATEGORIES.map((c) => (
                <label key={c.id} className="flex items-center gap-2 text-sm text-ink dark:text-dark-ink cursor-pointer">
                  <input
                    type="checkbox"
                    checked={enabled[c.id]}
                    onChange={(e) => setEnabled((prev) => ({ ...prev, [c.id]: e.target.checked }))}
                    className="cursor-pointer"
                  />
                  <span>{c.label}</span>
                </label>
              ))}
            </div>
          </fieldset>

          <div>
            <label htmlFor="redactor-custom" className="block text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">
              Custom words (one per line)
            </label>
            <textarea
              id="redactor-custom"
              value={customRaw}
              onChange={(e) => setCustomRaw(e.target.value)}
              rows={3}
              spellCheck={false}
              autoComplete="off"
              className={areaClass}
            />
          </div>
        </div>

        <div className="mt-6 pt-4 border-t border-hairline-soft dark:border-dark-hairline flex items-center justify-between">
          <span className="text-xs text-muted dark:text-dark-muted">Nothing is stored or logged</span>
          <button
            type="button"
            onClick={handleRedact}
            disabled={busy || text.length === 0}
            className="h-12 px-6 bg-rausch hover:bg-rausch-active disabled:bg-rausch-disabled text-white font-semibold text-sm rounded-sm flex items-center gap-2 cursor-pointer disabled:cursor-not-allowed transition-all shadow-xs"
          >
            <EyeOff size={16} />
            <span>{busy ? 'Redacting...' : 'Redact'}</span>
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
        <div className="space-y-4" data-testid="redactor-result">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className={cardClass}>
              <h3 className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-2">Original</h3>
              <pre
                data-testid="redactor-original"
                className="text-xs font-mono text-ink dark:text-dark-ink whitespace-pre-wrap break-all max-h-96 overflow-auto"
              >
                {text}
              </pre>
            </div>
            <div className={cardClass}>
              <div className="flex items-center justify-between mb-2">
                <h3 className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider">Redacted</h3>
                <button
                  type="button"
                  onClick={handleCopy}
                  className="inline-flex items-center gap-1 text-xs font-semibold text-ink dark:text-dark-ink hover:underline cursor-pointer"
                >
                  {copied ? <Check size={14} /> : <Copy size={14} />}
                  <span>{copied ? 'Copied' : 'Copy'}</span>
                </button>
              </div>
              <pre
                data-testid="redactor-output"
                className="text-xs font-mono text-ink dark:text-dark-ink whitespace-pre-wrap break-all max-h-96 overflow-auto"
              >
                {result.text}
              </pre>
              {copied && (
                <p role="status" className="text-xs text-emerald-800 dark:text-emerald-400 mt-2">
                  Copied to clipboard
                </p>
              )}
              {copyError && (
                <p role="alert" className="text-xs text-airbnb-error dark:text-red-400 mt-2">
                  {copyError}
                </p>
              )}
            </div>
          </div>

          {countEntries.length > 0 && (
            <div className={cardClass}>
              <h3 className="text-xs font-bold text-ink dark:text-dark-ink uppercase tracking-wider mb-3">Replacements</h3>
              <ul className="space-y-1.5 text-xs">
                {countEntries.map(([cat, n]) => (
                  <li
                    key={cat}
                    className="flex justify-between py-1 border-b border-hairline-soft/60 dark:border-dark-hairline/60"
                  >
                    <span className="font-mono text-ink dark:text-dark-ink">{cat}</span>
                    <span className="font-mono font-semibold text-ink dark:text-dark-ink">{n}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

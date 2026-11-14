import React, { useEffect, useRef, useState } from 'react';
import { Flame } from 'lucide-react';

interface BurnConfirmDialogProps {
  /** `demo` true: mulai hitung mundur 3 detik yang dapat dibatalkan, bukan kirim langsung. */
  onConfirm: (demo: boolean) => void;
  onCancel: () => void;
}

/** Dialog Burn_Confirmation: tombol "Setuju" dan "Batal"; Escape = Batal. Dirender sinkron saat dibuka. */
export const BurnConfirmDialog: React.FC<BurnConfirmDialogProps> = ({ onConfirm, onCancel }) => {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [demo, setDemo] = useState(false);

  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onCancel]);

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center p-6 bg-ink/60"
      role="presentation"
    >
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="burn-confirm-title"
        aria-describedby="burn-confirm-desc"
        className="max-w-md w-full bg-white dark:bg-dark-card border border-hairline-soft dark:border-dark-hairline rounded-md shadow-airbnb p-6"
      >
        <div className="flex items-center gap-2 mb-3 text-airbnb-error">
          <Flame size={22} aria-hidden="true" />
          <h2 id="burn-confirm-title" className="text-lg font-bold">
            Burn Workspace?
          </h2>
        </div>
        <p id="burn-confirm-desc" className="text-sm text-body dark:text-dark-body mb-6">
          Seluruh data memori akan dihapus dan container akan dimatikan. Tindakan ini permanen dan
          tidak dapat dibatalkan.
        </p>
        <label className="flex items-start gap-2 mb-6 text-sm text-muted dark:text-dark-muted cursor-pointer">
          <input
            type="checkbox"
            checked={demo}
            onChange={(e) => setDemo(e.target.checked)}
            className="mt-0.5"
          />
          <span>
            <span className="font-semibold text-ink dark:text-dark-ink">Demo mode</span>: mulai
            hitung mundur 3 detik yang bisa dibatalkan, bukan langsung membakar.
          </span>
        </label>
        <div className="flex justify-end gap-3">
          <button
            ref={cancelRef}
            type="button"
            onClick={onCancel}
            className="px-4 py-2 rounded-sm text-sm font-semibold border border-hairline-soft dark:border-dark-border text-ink dark:text-dark-ink bg-white dark:bg-dark-surface hover:bg-surface-soft dark:hover:bg-dark-surface-hover transition-colors"
          >
            Batal
          </button>
          <button
            type="button"
            onClick={() => onConfirm(demo)}
            className="px-4 py-2 rounded-sm text-sm font-semibold text-white bg-airbnb-error hover:opacity-90 transition-opacity"
          >
            Setuju
          </button>
        </div>
      </div>
    </div>
  );
};

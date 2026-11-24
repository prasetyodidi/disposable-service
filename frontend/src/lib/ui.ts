/**
 * Shared primary action button.
 * Enabled: rausch fill. Disabled: neutral surface with a hairline border and muted text,
 * so "not available yet" reads as inactive instead of a washed-out version of the primary colour.
 * The border is always present (transparent when enabled) so the button size never shifts.
 */
export const PRIMARY_BUTTON =
  'h-12 px-6 inline-flex items-center justify-center gap-2 rounded-sm border border-transparent bg-rausch text-white text-sm font-semibold shadow-xs cursor-pointer transition-colors hover:bg-rausch-active ' +
  'disabled:cursor-not-allowed disabled:shadow-none disabled:bg-surface-strong disabled:text-muted-soft disabled:border-hairline disabled:hover:bg-surface-strong ' +
  'dark:disabled:bg-dark-surface dark:disabled:text-dark-muted-soft dark:disabled:border-dark-border dark:disabled:hover:bg-dark-surface';

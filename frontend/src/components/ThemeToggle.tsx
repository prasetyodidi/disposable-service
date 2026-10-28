import React, { useEffect, useState } from 'react';
import { Sun, Moon } from 'lucide-react';

interface ThemeToggleProps {
  className?: string;
}

export const ThemeToggle: React.FC<ThemeToggleProps> = ({ className = '' }) => {
  const [isDark, setIsDark] = useState<boolean>(false);

  useEffect(() => {
    // Sync with DOM state set by Layout.astro
    const checkDark = () => {
      setIsDark(document.documentElement.classList.contains('dark'));
    };
    checkDark();

    // Listen to changes from other tabs or components
    const observer = new MutationObserver(() => checkDark());
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });

    return () => observer.disconnect();
  }, []);

  const toggleTheme = () => {
    const nextDark = !isDark;
    setIsDark(nextDark);
    if (nextDark) {
      document.documentElement.classList.add('dark');
      try {
        localStorage.setItem('disposable_theme', 'dark');
      } catch (_) {}
    } else {
      document.documentElement.classList.remove('dark');
      try {
        localStorage.setItem('disposable_theme', 'light');
      } catch (_) {}
    }
  };

  return (
    <button
      type="button"
      onClick={toggleTheme}
      title={isDark ? 'Switch to Light Mode' : 'Switch to Dark Mode'}
      aria-label="Toggle Dark Mode"
      className={`w-9 h-9 flex items-center justify-center rounded-full text-muted hover:text-ink dark:text-dark-muted dark:hover:text-dark-ink hover:bg-surface-strong dark:hover:bg-dark-surface transition-colors cursor-pointer ${className}`}
    >
      {isDark ? (
        <Sun size={18} className="text-amber-400 hover:rotate-45 transition-transform duration-300" />
      ) : (
        <Moon size={18} className="text-muted hover:rotate-12 transition-transform duration-300" />
      )}
    </button>
  );
};

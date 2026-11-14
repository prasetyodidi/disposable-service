import React from 'react';
import { Navbar } from './Navbar';

interface PageShellProps {
  currentPath: string;
  children: React.ReactNode;
}

/** Shared page frame: same navbar, same content width and padding on every page. */
export const PageShell: React.FC<PageShellProps> = ({ currentPath, children }) => (
  <div className="min-h-screen bg-surface-soft dark:bg-dark-canvas flex flex-col transition-colors">
    <Navbar currentPath={currentPath} />
    <main className="flex-1 max-w-6xl w-full mx-auto px-4 sm:px-6 py-8 sm:py-10">{children}</main>
  </div>
);

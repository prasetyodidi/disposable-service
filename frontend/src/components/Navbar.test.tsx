// @vitest-environment jsdom
import React from 'react';
import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, within } from '@testing-library/react';

vi.mock('./BurnButton', () => ({ BurnButton: () => <div data-testid="burn-button" /> }));
vi.mock('./ThemeToggle', () => ({ ThemeToggle: () => <div data-testid="theme-toggle" /> }));
vi.mock('./ActiveSessionsModal', () => ({ ActiveSessionsModal: () => null }));
vi.mock('./McpTokensPanel', () => ({ McpTokensPanel: () => null }));
vi.mock('./PageShell', () => ({
  PageShell: ({ children }: { children: React.ReactNode }) => <main>{children}</main>,
}));

import { Navbar } from './Navbar';
import { DashboardView } from './DashboardView';

afterEach(() => cleanup());

describe('Navbar PDF to Markdown link', () => {
  it('shows a desktop link to /pdf-to-markdown with an accessible name containing "PDF to Markdown"', () => {
    render(<Navbar currentPath="/" />);
    const desktopNav = screen.getAllByRole('navigation', { name: 'Primary' })[0];
    const link = within(desktopNav).getByRole('link', { name: /PDF to Markdown/ });
    expect(link.getAttribute('href')).toBe('/pdf-to-markdown');
  });

  it('sets aria-current="page" only on the PDF to Markdown link when it is the active page', () => {
    const { unmount } = render(<Navbar currentPath="/pdf-to-markdown" />);
    const current = document.querySelectorAll('[aria-current="page"]');
    expect(current).toHaveLength(1);
    expect(current[0].getAttribute('href')).toBe('/pdf-to-markdown');
    unmount();

    render(<Navbar currentPath="/scrubber" />);
    const pdfLink = document.querySelector('a[href="/pdf-to-markdown"]');
    expect(pdfLink?.hasAttribute('aria-current')).toBe(false);
    expect(document.querySelectorAll('[aria-current="page"]')).toHaveLength(1);
  });

  it('lists the link in the mobile "Menu" and closes the menu when it is clicked', () => {
    render(<Navbar currentPath="/" />);
    expect(document.getElementById('mobile-nav')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /Menu/ }));
    const mobileNav = document.getElementById('mobile-nav') as HTMLElement;
    expect(mobileNav).not.toBeNull();
    const link = within(mobileNav).getByRole('link', { name: 'PDF to Markdown' });
    expect(link.getAttribute('href')).toBe('/pdf-to-markdown');
    fireEvent.click(link);
    expect(document.getElementById('mobile-nav')).toBeNull();
  });
});

describe('DashboardView PDF to Markdown card', () => {
  it('contains exactly one link to /pdf-to-markdown', () => {
    const { container } = render(<DashboardView />);
    expect(container.querySelectorAll('a[href="/pdf-to-markdown"]')).toHaveLength(1);
  });
});

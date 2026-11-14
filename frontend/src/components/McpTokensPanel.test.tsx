// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { McpTokensPanel, buildClientConfig } from './McpTokensPanel';

const fetchMock = vi.fn();

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal('fetch', fetchMock);
  Object.assign(navigator, { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const json = (body: unknown, ok = true, status = 200) => ({ ok, status, json: async () => body });

describe('McpTokensPanel', () => {
  it('creates a token, shows it once with a client config, and sends the chosen lifetime and label', async () => {
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === '/api/sessions') return json({ sessions: [] });
      if (url === '/api/auth/token' && init?.method === 'POST') {
        return json({ success: true, token: 'jwt.abc.def', expires_at: 1900000000, session_id: 's1' });
      }
      return json({}, false, 404);
    });
    render(<McpTokensPanel />);
    fireEvent.change(screen.getByPlaceholderText(/Kiro on my laptop/), { target: { value: '  Kiro  ' } });
    fireEvent.change(screen.getByDisplayValue('30 days'), { target: { value: '7' } });
    fireEvent.click(screen.getByRole('button', { name: /Create token/ }));

    await waitFor(() => expect(screen.getByTestId('created-token').textContent).toBe('jwt.abc.def'));
    const post = fetchMock.mock.calls.find((c) => c[0] === '/api/auth/token');
    expect(JSON.parse(post![1].body)).toEqual({ lifetime_seconds: 7 * 86400, label: 'Kiro' });
    expect(screen.getByText(/will not be able to see this token again/)).toBeTruthy();
    expect(screen.getByText(/"Authorization": "Bearer jwt.abc.def"/)).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss token' }));
    expect(screen.queryByTestId('created-token')).toBeNull();
  });

  it('shows the server error and no token when creation fails', async () => {
    fetchMock.mockImplementation(async (url: string) => {
      if (url === '/api/sessions') return json({ sessions: [] });
      return json({ success: false, message: 'lifetime_seconds out of range' }, false, 400);
    });
    render(<McpTokensPanel />);
    fireEvent.click(screen.getByRole('button', { name: /Create token/ }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/lifetime_seconds out of range/));
    expect(screen.queryByTestId('created-token')).toBeNull();
  });

  it('lists only MCP tokens from the sessions endpoint', async () => {
    fetchMock.mockImplementation(async () =>
      json({
        sessions: [
          { session_id: 'a', user_agent: 'Chrome', login_time: 1, expires_at: 2000000000 },
          { session_id: 'b', user_agent: 'MCP Agent (Kiro)', login_time: 1, expires_at: 2000000000, token_type: 'mcp_agent', label: 'Kiro' },
        ],
      }),
    );
    render(<McpTokensPanel />);
    await waitFor(() => expect(screen.getByText('Kiro')).toBeTruthy());
    expect(screen.queryByText('Chrome')).toBeNull();
  });

  it('builds a client config with the origin and bearer header', () => {
    const cfg = JSON.parse(buildClientConfig('https://x.example', 't0k'));
    expect(cfg.mcpServers['disposable-service'].url).toBe('https://x.example/api/mcp/sse');
    expect(cfg.mcpServers['disposable-service'].headers.Authorization).toBe('Bearer t0k');
  });
});

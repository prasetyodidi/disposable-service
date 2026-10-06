import React, { useEffect, useState } from 'react';
import { X, Shield, Globe, Clock, Smartphone, Laptop } from 'lucide-react';

interface Session {
  session_id: string;
  ip_address: string;
  user_agent: string;
  login_time: string;
  expires_at: string;
}

interface ActiveSessionsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onLogout: () => void;
}

export const ActiveSessionsModal: React.FC<ActiveSessionsModalProps> = ({
  isOpen,
  onClose,
  onLogout,
}) => {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      fetchSessions();
    }
  }, [isOpen]);

  const fetchSessions = async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch('/api/sessions');
      if (res.ok) {
        const data = await res.json();
        setSessions(data.sessions || []);
      } else if (res.status === 401) {
        onLogout();
      } else {
        setError('Failed to fetch active sessions');
      }
    } catch {
      setError('Network error when fetching sessions');
    } finally {
      setLoading(false);
    }
  };

  if (!isOpen) return null;

  const parseUserAgent = (ua: string) => {
    if (/mobile/i.test(ua)) return { icon: <Smartphone size={16} />, label: 'Mobile Device' };
    return { icon: <Laptop size={16} />, label: 'Desktop / Browser' };
  };

  const formatDate = (iso: string) => {
    try {
      return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    } catch {
      return iso;
    }
  };

  return (
    <div
      className="fixed inset-0 bg-black/50 backdrop-blur-xs flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-md shadow-airbnb-modal max-w-xl w-full p-6 sm:p-8 max-h-[85vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex justify-between items-center mb-5">
          <div className="flex items-center gap-3">
            <div className="bg-surface-soft p-2.5 rounded-sm text-rausch">
              <Shield size={22} />
            </div>
            <div>
              <h2 className="text-lg font-bold text-ink">Active Sessions</h2>
              <p className="text-xs text-muted">
                Single password authentication tracking by device & IP
              </p>
            </div>
          </div>
          <button
            className="p-2 text-muted hover:text-ink rounded-full hover:bg-surface-strong transition-colors cursor-pointer"
            onClick={onClose}
          >
            <X size={20} />
          </button>
        </div>

        {error && (
          <div className="bg-red-50 border border-red-200 text-airbnb-error px-4 py-3 rounded-sm text-sm mb-4">
            {error}
          </div>
        )}

        {loading ? (
          <div className="py-10 text-center text-muted text-sm">
            Loading active sessions...
          </div>
        ) : sessions.length === 0 ? (
          <div className="py-10 text-center text-muted text-sm">
            No other active sessions detected.
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse mt-2">
              <thead>
                <tr className="border-b border-hairline-soft">
                  <th className="text-left py-3 px-3 text-xs font-semibold text-muted uppercase tracking-wider">Device</th>
                  <th className="text-left py-3 px-3 text-xs font-semibold text-muted uppercase tracking-wider">IP Address</th>
                  <th className="text-left py-3 px-3 text-xs font-semibold text-muted uppercase tracking-wider">Login Time</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-hairline-soft">
                {sessions.map((s) => {
                  const { icon, label } = parseUserAgent(s.user_agent);
                  return (
                    <tr key={s.session_id} className="hover:bg-surface-soft/60">
                      <td className="py-3 px-3">
                        <div className="flex items-center gap-2 font-medium text-ink text-sm">
                          <span className="text-muted">{icon}</span>
                          <span>{label}</span>
                        </div>
                        <div className="text-[11px] text-muted-soft max-w-[200px] truncate">
                          {s.user_agent}
                        </div>
                      </td>
                      <td className="py-3 px-3">
                        <div className="flex items-center gap-1.5 text-sm">
                          <Globe size={14} className="text-muted-soft" />
                          <span className="font-mono text-xs text-body">{s.ip_address}</span>
                        </div>
                      </td>
                      <td className="py-3 px-3 text-sm text-muted">
                        <div className="flex items-center gap-1.5">
                          <Clock size={14} />
                          <span>{formatDate(s.login_time)}</span>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}

        <div className="mt-6 pt-5 flex items-center justify-between border-t border-hairline-soft">
          <span className="text-xs text-muted">
            Total Active: <strong className="text-ink">{sessions.length}</strong>
          </span>
          <div className="flex gap-2.5">
            <button
              className="h-10 px-4 text-sm font-semibold text-ink bg-white border border-hairline hover:border-ink rounded-sm transition-all cursor-pointer"
              onClick={onClose}
            >
              Close
            </button>
            <button
              className="h-10 px-5 text-sm font-semibold text-white bg-rausch hover:bg-rausch-active rounded-sm transition-all cursor-pointer"
              onClick={onLogout}
            >
              Sign Out
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

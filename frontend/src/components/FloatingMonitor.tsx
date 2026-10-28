import React, { useState } from 'react';
import { Activity, ChevronDown, Maximize2, X, Cpu, Server, HardDrive } from 'lucide-react';
import { useServerMetrics } from '../hooks/useServerMetrics';

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export const FloatingMonitor: React.FC = () => {
  const [expanded, setExpanded] = useState<boolean>(false);
  const [closed, setClosed] = useState<boolean>(false);
  const { metrics, history, isConnected } = useServerMetrics(!closed);

  if (closed) {
    return (
      <button
        onClick={() => setClosed(false)}
        title="Open Host Monitor"
        className="fixed bottom-5 right-5 z-40 bg-white dark:bg-dark-card border border-hairline-soft dark:border-dark-hairline shadow-airbnb hover:shadow-airbnb-hover p-2.5 rounded-full text-ink dark:text-dark-ink hover:text-rausch transition-all cursor-pointer flex items-center gap-1.5 text-xs font-semibold"
      >
        <Activity size={16} className="text-rausch" />
        <span className="hidden sm:inline">Monitor</span>
      </button>
    );
  }

  const cpuPct = metrics ? Math.round(metrics.cpu_usage_pct) : 0;
  const memUsed = metrics?.memory_used_bytes || 0;
  const memTotal = metrics?.memory_total_bytes || 1;
  const memPct = Math.round((memUsed / memTotal) * 100);

  // Status color based on load
  const statusColor =
    cpuPct > 85 || memPct > 90
      ? 'bg-rausch text-white'
      : cpuPct > 60 || memPct > 75
      ? 'bg-amber-500 text-white'
      : 'bg-emerald-500 text-white';

  const pulseColor =
    cpuPct > 85 || memPct > 90
      ? 'bg-rausch'
      : cpuPct > 60 || memPct > 75
      ? 'bg-amber-500'
      : 'bg-emerald-500';

  if (!expanded) {
    // Collapsed Pill View
    return (
      <div className="fixed bottom-5 right-5 z-40">
        <button
          onClick={() => setExpanded(true)}
          className="bg-white/95 dark:bg-dark-card/95 backdrop-blur-md border border-hairline-soft dark:border-dark-hairline shadow-airbnb hover:shadow-airbnb-hover px-3.5 py-2 rounded-full flex items-center gap-3 transition-all cursor-pointer group"
          title="Click to expand real-time server monitor"
        >
          <span className="relative flex h-2.5 w-2.5">
            <span
              className={`animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 ${pulseColor}`}
            ></span>
            <span
              className={`relative inline-flex rounded-full h-2.5 w-2.5 ${pulseColor}`}
            ></span>
          </span>

          <div className="flex items-center gap-2.5 text-xs font-medium text-ink dark:text-dark-ink">
            <span className="flex items-center gap-1">
              <Cpu size={13} className="text-muted dark:text-dark-muted" />
              <span>{isConnected ? `${cpuPct}%` : 'Connecting...'}</span>
            </span>
            <span className="text-hairline dark:text-dark-hairline">|</span>
            <span className="flex items-center gap-1">
              <Server size={13} className="text-muted dark:text-dark-muted" />
              <span>{isConnected ? `${memPct}%` : '--'}</span>
            </span>
          </div>

          <span className="text-[11px] font-semibold text-rausch bg-rose-50 dark:bg-rose-950/40 px-2 py-0.5 rounded-full group-hover:bg-rausch group-hover:text-white transition-colors">
            Live Host
          </span>
        </button>
      </div>
    );
  }

  // Expanded Floating Card View
  return (
    <div className="fixed bottom-5 right-5 z-50 w-88 sm:w-96 bg-white/95 dark:bg-dark-card/95 backdrop-blur-md rounded-md shadow-airbnb-modal border border-hairline-soft dark:border-dark-hairline p-5 animate-in fade-in slide-in-from-bottom-2 duration-200">
      {/* Header */}
      <div className="flex items-center justify-between pb-3 mb-4 border-b border-hairline-soft dark:border-dark-hairline">
        <div className="flex items-center gap-2">
          <span className="relative flex h-2.5 w-2.5">
            <span
              className={`animate-ping absolute inline-flex h-full w-full rounded-full opacity-75 ${pulseColor}`}
            ></span>
            <span
              className={`relative inline-flex rounded-full h-2.5 w-2.5 ${pulseColor}`}
            ></span>
          </span>
          <div>
            <h3 className="text-sm font-bold text-ink dark:text-dark-ink flex items-center gap-1.5">
              <span>Host Live Monitor</span>
              <span className="text-[10px] uppercase font-bold tracking-wider px-1.5 py-0.5 bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 rounded-full border border-emerald-200 dark:border-emerald-800">
                SSE
              </span>
            </h3>
            <p className="text-[11px] text-muted dark:text-dark-muted truncate max-w-[180px]">
              {metrics ? `${metrics.hostname} (${metrics.os_name})` : 'Connecting to host...'}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1">
          <a
            href="/monitor"
            title="Open Full Page View"
            className="p-1.5 text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink hover:bg-surface-strong dark:hover:bg-dark-surface rounded-sm transition-colors"
          >
            <Maximize2 size={15} />
          </a>
          <button
            onClick={() => setExpanded(false)}
            title="Minimize to Pill"
            className="p-1.5 text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink hover:bg-surface-strong dark:hover:bg-dark-surface rounded-sm transition-colors cursor-pointer"
          >
            <ChevronDown size={17} />
          </button>
          <button
            onClick={() => {
              setExpanded(false);
              setClosed(true);
            }}
            title="Close Panel"
            className="p-1.5 text-muted dark:text-dark-muted hover:text-rausch hover:bg-rose-50 dark:hover:bg-rose-950/40 rounded-sm transition-colors cursor-pointer"
          >
            <X size={15} />
          </button>
        </div>
      </div>

      {/* Main Gauges */}
      <div className="space-y-4">
        {/* CPU Meter */}
        <div>
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="font-semibold text-ink dark:text-dark-ink flex items-center gap-1.5">
              <Cpu size={14} className="text-muted dark:text-dark-muted" />
              <span>CPU Load</span>
            </span>
            <span className="font-bold text-ink dark:text-dark-ink">{cpuPct}%</span>
          </div>
          <div className="w-full h-2 bg-surface-strong dark:bg-dark-surface rounded-full overflow-hidden">
            <div
              className={`h-full transition-all duration-500 rounded-full ${
                cpuPct > 85 ? 'bg-rausch' : cpuPct > 60 ? 'bg-amber-500' : 'bg-emerald-500'
              }`}
              style={{ width: `${cpuPct}%` }}
            />
          </div>

          {/* Sparkline */}
          {history.length > 3 && (
            <div className="mt-2 h-7 w-full flex items-end gap-[2px]">
              {history.map((pt, i) => (
                <div
                  key={i}
                  className="flex-1 bg-surface-strong dark:bg-dark-surface hover:bg-rausch rounded-t-[1px] transition-colors"
                  style={{ height: `${Math.max(10, Math.min(100, pt.cpu))}%` }}
                  title={`CPU: ${Math.round(pt.cpu)}%`}
                />
              ))}
            </div>
          )}
        </div>

        {/* Memory Meter */}
        <div>
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="font-semibold text-ink dark:text-dark-ink flex items-center gap-1.5">
              <Server size={14} className="text-muted dark:text-dark-muted" />
              <span>Memory (RAM)</span>
            </span>
            <span className="text-xs text-muted dark:text-dark-muted">
              <strong className="text-ink dark:text-dark-ink">{formatBytes(memUsed)}</strong> / {formatBytes(memTotal)}
            </span>
          </div>
          <div className="w-full h-2 bg-surface-strong dark:bg-dark-surface rounded-full overflow-hidden">
            <div
              className={`h-full transition-all duration-500 rounded-full ${
                memPct > 85 ? 'bg-rausch' : memPct > 65 ? 'bg-amber-500' : 'bg-blue-500'
              }`}
              style={{ width: `${memPct}%` }}
            />
          </div>
        </div>

        {/* Load Averages */}
        {metrics && (
          <div className="grid grid-cols-3 gap-2 pt-2 border-t border-hairline-soft dark:border-dark-hairline text-center">
            <div className="bg-surface-soft dark:bg-dark-surface p-1.5 rounded-sm">
              <div className="text-[10px] text-muted dark:text-dark-muted">Load 1m</div>
              <div className="text-xs font-bold text-ink dark:text-dark-ink">{metrics.load_avg_1m.toFixed(2)}</div>
            </div>
            <div className="bg-surface-soft dark:bg-dark-surface p-1.5 rounded-sm">
              <div className="text-[10px] text-muted dark:text-dark-muted">Load 5m</div>
              <div className="text-xs font-bold text-ink dark:text-dark-ink">{metrics.load_avg_5m.toFixed(2)}</div>
            </div>
            <div className="bg-surface-soft dark:bg-dark-surface p-1.5 rounded-sm">
              <div className="text-[10px] text-muted dark:text-dark-muted">Load 15m</div>
              <div className="text-xs font-bold text-ink dark:text-dark-ink">{metrics.load_avg_15m.toFixed(2)}</div>
            </div>
          </div>
        )}

        {/* Top Processes (Top 3) */}
        {metrics && metrics.top_processes.length > 0 && (
          <div className="pt-2 border-t border-hairline-soft dark:border-dark-hairline">
            <div className="text-[11px] font-bold text-ink dark:text-dark-ink mb-1.5 flex items-center justify-between">
              <span>Top Host Processes</span>
              <span className="text-[10px] text-muted dark:text-dark-muted font-normal">by CPU</span>
            </div>
            <div className="space-y-1">
              {metrics.top_processes.slice(0, 3).map((p) => (
                <div
                  key={p.pid}
                  className="flex items-center justify-between text-xs py-1 px-2 rounded-sm bg-surface-soft/80 dark:bg-dark-surface/80"
                >
                  <span className="font-mono text-ink dark:text-dark-ink text-[11px] truncate max-w-[170px]" title={p.name}>
                    {p.name}
                  </span>
                  <div className="flex items-center gap-2">
                    <span className="text-[10px] text-muted dark:text-dark-muted font-mono">{formatBytes(p.memory_bytes)}</span>
                    <span
                      className={`text-[11px] font-bold font-mono ${
                        p.cpu_usage > 50 ? 'text-rausch' : 'text-ink dark:text-dark-ink'
                      }`}
                    >
                      {p.cpu_usage.toFixed(1)}%
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="mt-4 pt-3 border-t border-hairline-soft dark:border-dark-hairline flex items-center justify-between text-[11px] text-muted dark:text-dark-muted">
        <span>Uptime: {metrics ? Math.floor(metrics.uptime_seconds / 3600) : 0}h {metrics ? Math.floor((metrics.uptime_seconds % 3600) / 60) : 0}m</span>
        <a href="/monitor" className="text-rausch font-semibold hover:underline">
          Full details &rarr;
        </a>
      </div>
    </div>
  );
};

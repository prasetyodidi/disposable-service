import React, { useState } from 'react';
import {
  Activity,
  Cpu,
  Server,
  HardDrive,
  Clock,
  Layers,
  ArrowLeft,
  Radio,
  Search,
} from 'lucide-react';
import { Navbar } from './Navbar';
import { useServerMetrics } from '../hooks/useServerMetrics';

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export const ServerMonitor: React.FC = () => {
  const [streamActive, setStreamActive] = useState(true);
  const [searchProcess, setSearchProcess] = useState('');
  const { metrics, history, isConnected, error } = useServerMetrics(streamActive);

  const cpuPct = metrics ? Math.round(metrics.cpu_usage_pct) : 0;
  const memUsed = metrics?.memory_used_bytes || 0;
  const memTotal = metrics?.memory_total_bytes || 1;
  const memPct = Math.round((memUsed / memTotal) * 100);

  const swapUsed = metrics?.swap_used_bytes || 0;
  const swapTotal = metrics?.swap_total_bytes || 0;
  const swapPct = swapTotal > 0 ? Math.round((swapUsed / swapTotal) * 100) : 0;

  const filteredProcesses = (metrics?.top_processes || []).filter((p) =>
    p.name.toLowerCase().includes(searchProcess.toLowerCase()) ||
    p.pid.toString().includes(searchProcess)
  );

  return (
    <div className="min-h-screen bg-surface-soft dark:bg-dark-canvas flex flex-col transition-colors">
      <Navbar currentPath="/monitor" />

      <main className="flex-1 max-w-6xl w-full mx-auto px-6 py-10">
        {/* Top Navigation & Status Bar */}
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
          <div>
            <div className="flex items-center gap-2 mb-2">
              <a
                href="/"
                className="inline-flex items-center gap-1.5 text-xs font-semibold text-muted dark:text-dark-muted hover:text-ink dark:hover:text-dark-ink transition-colors"
              >
                <ArrowLeft size={14} />
                <span>Back to Services</span>
              </a>
              <span className="text-hairline dark:text-dark-border">•</span>
              <span className="bg-surface-strong dark:bg-dark-surface text-muted dark:text-dark-muted text-[11px] font-semibold px-2.5 py-0.5 rounded-full">
                App 2 • Observability
              </span>
            </div>
            <h1 className="text-3xl font-bold text-ink dark:text-dark-ink tracking-tight">
              Host System Monitor
            </h1>
            <p className="text-muted dark:text-dark-muted text-sm mt-1">
              Real-time read-only resource telemetry streamed directly from host kernel interfaces via SSE.
            </p>
          </div>

          {/* Stream controls */}
          <div className="flex items-center gap-3">
            <button
              onClick={() => setStreamActive(!streamActive)}
              className={`h-10 px-4 rounded-sm text-xs font-semibold flex items-center gap-2 transition-all cursor-pointer border ${
                streamActive
                  ? 'bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-400 border-emerald-200 dark:border-emerald-800 hover:bg-emerald-100 dark:hover:bg-emerald-900/60'
                  : 'bg-white dark:bg-dark-card text-muted dark:text-dark-muted border-hairline dark:border-dark-border hover:text-ink dark:hover:text-dark-ink'
              }`}
            >
              <Radio
                size={14}
                className={streamActive && isConnected ? 'animate-pulse text-emerald-600 dark:text-emerald-400' : ''}
              />
              <span>{streamActive ? 'Live Streaming (SSE)' : 'Stream Paused'}</span>
            </button>
          </div>
        </div>

        {error && (
          <div className="mb-6 p-4 rounded-sm bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800 text-amber-800 dark:text-amber-400 text-xs flex items-center justify-between">
            <span>{error}</span>
            <button
              onClick={() => window.location.reload()}
              className="font-bold underline cursor-pointer ml-3"
            >
              Retry
            </button>
          </div>
        )}

        {/* Top 4 KPI Cards */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-5 mb-8">
          {/* Card 1: CPU */}
          <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-5 border border-hairline-soft dark:border-dark-hairline flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between text-muted dark:text-dark-muted mb-2">
                <span className="text-xs font-semibold uppercase tracking-wider">Overall CPU</span>
                <Cpu size={18} className="text-muted-soft dark:text-dark-muted-soft" />
              </div>
              <div className="text-3xl font-extrabold text-ink dark:text-dark-ink tracking-tight mb-2">
                {cpuPct}%
              </div>
              <div className="w-full h-2 bg-surface-strong dark:bg-dark-surface rounded-full overflow-hidden mb-2">
                <div
                  className={`h-full transition-all duration-300 rounded-full ${
                    cpuPct > 85 ? 'bg-rausch' : cpuPct > 60 ? 'bg-amber-500' : 'bg-emerald-500'
                  }`}
                  style={{ width: `${cpuPct}%` }}
                />
              </div>
            </div>
            <div className="pt-3 border-t border-hairline-soft dark:border-dark-hairline text-xs text-muted dark:text-dark-muted flex items-center justify-between">
              <span>Cores: {metrics?.cpu_cores.length || '--'}</span>
              <span>Load: {metrics ? metrics.load_avg_1m.toFixed(2) : '--'}</span>
            </div>
          </div>

          {/* Card 2: Memory */}
          <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-5 border border-hairline-soft dark:border-dark-hairline flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between text-muted dark:text-dark-muted mb-2">
                <span className="text-xs font-semibold uppercase tracking-wider">RAM Usage</span>
                <Server size={18} className="text-muted-soft dark:text-dark-muted-soft" />
              </div>
              <div className="text-3xl font-extrabold text-ink dark:text-dark-ink tracking-tight mb-2">
                {memPct}%
              </div>
              <div className="w-full h-2 bg-surface-strong dark:bg-dark-surface rounded-full overflow-hidden mb-2">
                <div
                  className={`h-full transition-all duration-300 rounded-full ${
                    memPct > 85 ? 'bg-rausch' : memPct > 65 ? 'bg-amber-500' : 'bg-blue-500'
                  }`}
                  style={{ width: `${memPct}%` }}
                />
              </div>
            </div>
            <div className="pt-3 border-t border-hairline-soft dark:border-dark-hairline text-xs text-muted dark:text-dark-muted flex items-center justify-between">
              <span>{formatBytes(memUsed)}</span>
              <span>of {formatBytes(memTotal)}</span>
            </div>
          </div>

          {/* Card 3: Swap Memory */}
          <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-5 border border-hairline-soft dark:border-dark-hairline flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between text-muted dark:text-dark-muted mb-2">
                <span className="text-xs font-semibold uppercase tracking-wider">Swap Space</span>
                <Layers size={18} className="text-muted-soft dark:text-dark-muted-soft" />
              </div>
              <div className="text-3xl font-extrabold text-ink dark:text-dark-ink tracking-tight mb-2">
                {swapPct}%
              </div>
              <div className="w-full h-2 bg-surface-strong dark:bg-dark-surface rounded-full overflow-hidden mb-2">
                <div
                  className="h-full bg-purple-500 transition-all duration-300 rounded-full"
                  style={{ width: `${swapPct}%` }}
                />
              </div>
            </div>
            <div className="pt-3 border-t border-hairline-soft dark:border-dark-hairline text-xs text-muted dark:text-dark-muted flex items-center justify-between">
              <span>{formatBytes(swapUsed)}</span>
              <span>of {formatBytes(swapTotal)}</span>
            </div>
          </div>

          {/* Card 4: Host System */}
          <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-5 border border-hairline-soft dark:border-dark-hairline flex flex-col justify-between">
            <div>
              <div className="flex items-center justify-between text-muted dark:text-dark-muted mb-2">
                <span className="text-xs font-semibold uppercase tracking-wider">Host Node</span>
                <Clock size={18} className="text-muted-soft dark:text-dark-muted-soft" />
              </div>
              <div className="text-lg font-bold text-ink dark:text-dark-ink truncate mb-1">
                {metrics?.hostname || 'Connecting...'}
              </div>
              <div className="text-xs text-muted dark:text-dark-muted truncate">
                {metrics?.os_name || 'Operating System'}
              </div>
            </div>
            <div className="pt-3 border-t border-hairline-soft dark:border-dark-hairline text-xs text-muted dark:text-dark-muted flex items-center justify-between">
              <span>Uptime:</span>
              <span className="font-semibold text-ink dark:text-dark-ink">
                {metrics ? `${Math.floor(metrics.uptime_seconds / 3600)}h ${Math.floor((metrics.uptime_seconds % 3600) / 60)}m` : '--'}
              </span>
            </div>
          </div>
        </div>

        {/* Real-time Trend & Cores Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-8">
          {/* CPU & Memory History Trend */}
          <div className="lg:col-span-2 bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline">
            <div className="flex items-center justify-between mb-6">
              <div>
                <h3 className="text-base font-bold text-ink dark:text-dark-ink">Resource Activity (Last 30s)</h3>
                <p className="text-xs text-muted dark:text-dark-muted">Continuous 1-second interval rolling telemetry</p>
              </div>
              <div className="flex items-center gap-4 text-xs font-medium">
                <span className="flex items-center gap-1.5 text-ink dark:text-dark-ink">
                  <span className="w-2.5 h-2.5 rounded-full bg-rausch inline-block"></span>
                  <span>CPU %</span>
                </span>
                <span className="flex items-center gap-1.5 text-ink dark:text-dark-ink">
                  <span className="w-2.5 h-2.5 rounded-full bg-blue-500 inline-block"></span>
                  <span>RAM %</span>
                </span>
              </div>
            </div>

            {/* Sparkline chart container */}
            <div className="h-44 w-full flex items-end gap-1.5 pt-4 pb-2 border-b border-hairline-soft dark:border-dark-hairline">
              {history.length === 0 ? (
                <div className="w-full h-full flex items-center justify-center text-xs text-muted dark:text-dark-muted">
                  Awaiting telemetry data stream...
                </div>
              ) : (
                history.map((pt, i) => (
                  <div key={i} className="flex-1 flex flex-col justify-end items-center h-full gap-0.5">
                    {/* CPU bar */}
                    <div
                      className="w-full bg-rose-400 dark:bg-rose-500/70 hover:bg-rausch rounded-t-[2px] transition-all"
                      style={{ height: `${Math.max(4, Math.min(100, pt.cpu))}%` }}
                      title={`CPU: ${pt.cpu.toFixed(1)}%`}
                    />
                  </div>
                ))
              )}
            </div>

            <div className="flex items-center justify-between text-[11px] text-muted dark:text-dark-muted pt-2">
              <span>-30 seconds</span>
              <span>Current Time</span>
            </div>
          </div>

          {/* Per-Core CPU Breakdown */}
          <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline flex flex-col justify-between">
            <div>
              <h3 className="text-base font-bold text-ink dark:text-dark-ink mb-1">Per-Core CPU Load</h3>
              <p className="text-xs text-muted dark:text-dark-muted mb-4">Core thread activity distribution</p>

              <div className="space-y-2.5 max-h-56 overflow-y-auto pr-1">
                {(metrics?.cpu_cores || []).map((coreLoad, idx) => (
                  <div key={idx} className="space-y-1">
                    <div className="flex items-center justify-between text-xs">
                      <span className="font-mono text-muted dark:text-dark-muted text-[11px]">Core #{idx}</span>
                      <span className="font-mono font-semibold text-ink dark:text-dark-ink text-[11px]">
                        {coreLoad.toFixed(1)}%
                      </span>
                    </div>
                    <div className="w-full h-1.5 bg-surface-strong dark:bg-dark-surface rounded-full overflow-hidden">
                      <div
                        className={`h-full rounded-full ${
                          coreLoad > 85 ? 'bg-rausch' : coreLoad > 60 ? 'bg-amber-500' : 'bg-emerald-500'
                        }`}
                        style={{ width: `${coreLoad}%` }}
                      />
                    </div>
                  </div>
                ))}
              </div>
            </div>

            <div className="pt-4 border-t border-hairline-soft dark:border-dark-hairline mt-4 text-xs text-muted dark:text-dark-muted flex items-center justify-between">
              <span>Load 1m: {metrics?.load_avg_1m.toFixed(2)}</span>
              <span>5m: {metrics?.load_avg_5m.toFixed(2)}</span>
              <span>15m: {metrics?.load_avg_15m.toFixed(2)}</span>
            </div>
          </div>
        </div>

        {/* Disks & Storage Section */}
        {metrics && metrics.disks.length > 0 && (
          <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb p-6 border border-hairline-soft dark:border-dark-hairline mb-8">
            <h3 className="text-base font-bold text-ink dark:text-dark-ink mb-1">Storage & Filesystem Capacity</h3>
            <p className="text-xs text-muted dark:text-dark-muted mb-4">Host storage mounts and ephemeral disk usage</p>

            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {metrics.disks.map((d, i) => {
                const used = d.total_bytes - d.available_bytes;
                const pct = d.total_bytes > 0 ? Math.round((used / d.total_bytes) * 100) : 0;
                return (
                  <div key={i} className="p-4 rounded-sm bg-surface-soft dark:bg-dark-surface border border-hairline-soft dark:border-dark-hairline">
                    <div className="flex items-center justify-between mb-2">
                      <span className="font-mono text-xs font-bold text-ink dark:text-dark-ink truncate" title={d.mount_point}>
                        {d.mount_point}
                      </span>
                      <HardDrive size={15} className="text-muted-soft dark:text-dark-muted-soft" />
                    </div>
                    <div className="w-full h-2 bg-surface-strong dark:bg-dark-surface rounded-full overflow-hidden mb-2">
                      <div
                        className={`h-full rounded-full ${pct > 85 ? 'bg-rausch' : 'bg-emerald-600'}`}
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                    <div className="flex items-center justify-between text-[11px] text-muted dark:text-dark-muted">
                      <span>{pct}% used</span>
                      <span>
                        {formatBytes(used)} / {formatBytes(d.total_bytes)}
                      </span>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Top Active Host Processes Table */}
        <div className="bg-white dark:bg-dark-card rounded-md shadow-airbnb border border-hairline-soft dark:border-dark-hairline overflow-hidden">
          <div className="p-6 border-b border-hairline-soft dark:border-dark-hairline flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
            <div>
              <h3 className="text-base font-bold text-ink dark:text-dark-ink">Top Host Processes</h3>
              <p className="text-xs text-muted dark:text-dark-muted">Ranked by instantaneous CPU consumption</p>
            </div>

            <div className="relative w-full sm:w-64">
              <Search size={15} className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-soft dark:text-dark-muted-soft" />
              <input
                type="text"
                placeholder="Filter by name or PID..."
                value={searchProcess}
                onChange={(e) => setSearchProcess(e.target.value)}
                className="w-full pl-9 pr-3 py-1.5 text-xs bg-surface-soft dark:bg-dark-surface border border-hairline-soft dark:border-dark-border rounded-sm text-ink dark:text-dark-ink focus:outline-none focus:border-ink dark:focus:border-dark-ink"
              />
            </div>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse">
              <thead>
                <tr className="bg-surface-soft dark:bg-dark-surface border-b border-hairline-soft dark:border-dark-hairline text-[11px] font-semibold text-muted dark:text-dark-muted uppercase tracking-wider">
                  <th className="py-3 px-6">PID</th>
                  <th className="py-3 px-6">Command / Process</th>
                  <th className="py-3 px-6 text-right">CPU %</th>
                  <th className="py-3 px-6 text-right">Memory (RSS)</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-hairline-soft dark:divide-dark-hairline text-xs font-mono">
                {filteredProcesses.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="py-8 text-center text-muted dark:text-dark-muted font-sans text-xs">
                      No matching processes found.
                    </td>
                  </tr>
                ) : (
                  filteredProcesses.map((p) => (
                    <tr key={p.pid} className="hover:bg-surface-soft/60 dark:hover:bg-dark-surface/60 transition-colors">
                      <td className="py-3 px-6 text-muted dark:text-dark-muted">{p.pid}</td>
                      <td className="py-3 px-6 font-bold text-ink dark:text-dark-ink">{p.name}</td>
                      <td className="py-3 px-6 text-right">
                        <span
                          className={`font-bold px-2 py-0.5 rounded-full ${
                            p.cpu_usage > 50
                              ? 'bg-rose-50 dark:bg-rose-950/40 text-rausch'
                              : p.cpu_usage > 20
                              ? 'bg-amber-50 dark:bg-amber-950/40 text-amber-800 dark:text-amber-400'
                              : 'text-ink dark:text-dark-ink'
                          }`}
                        >
                          {p.cpu_usage.toFixed(1)}%
                        </span>
                      </td>
                      <td className="py-3 px-6 text-right text-muted dark:text-dark-muted">{formatBytes(p.memory_bytes)}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </main>
    </div>
  );
};

import { useEffect, useState } from 'react';
import type { HostMetrics } from '../types/monitor';

export interface MetricHistoryPoint {
  time: number;
  cpu: number;
  memoryPct: number;
}

export function useServerMetrics(enabled: boolean = true) {
  const [metrics, setMetrics] = useState<HostMetrics | null>(null);
  const [history, setHistory] = useState<MetricHistoryPoint[]>([]);
  const [isConnected, setIsConnected] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled) {
      setIsConnected(false);
      return;
    }

    let isMounted = true;

    const fetchMetrics = async () => {
      try {
        const res = await fetch('/api/apps/monitor', {
          credentials: 'same-origin',
        });
        if (!res.ok) {
          throw new Error(`HTTP ${res.status}`);
        }
        const json = await res.json();
        const data: HostMetrics = (json && json.data) ? json.data : json;
        if (!isMounted) return;

        setMetrics(data);
        setIsConnected(true);
        setError(null);

        const memoryPct =
          data.memory_total_bytes > 0
            ? (data.memory_used_bytes / data.memory_total_bytes) * 100
            : 0;

        setHistory((prev) => {
          const next = [...prev, { time: data.timestamp, cpu: data.cpu_usage_pct, memoryPct }];
          if (next.length > 30) {
            return next.slice(next.length - 30);
          }
          return next;
        });
      } catch (err: any) {
        if (!isMounted) return;
        setIsConnected(false);
        setError('Failed to fetch server metrics');
      }
    };

    // Immediate initial fetch
    fetchMetrics();

    // Poll every 5 seconds
    const intervalId = setInterval(fetchMetrics, 5000);

    return () => {
      isMounted = false;
      clearInterval(intervalId);
    };
  }, [enabled]);

  return { metrics, history, isConnected, error };
}

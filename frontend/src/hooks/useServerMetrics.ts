import { useEffect, useRef, useState } from 'react';
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
  const eventSourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    if (!enabled) {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
        setIsConnected(false);
      }
      return;
    }

    const es = new EventSource('/api/apps/monitor/stream');
    eventSourceRef.current = es;

    es.onopen = () => {
      setIsConnected(true);
      setError(null);
    };

    es.onmessage = (event) => {
      try {
        const data: HostMetrics = JSON.parse(event.data);
        setMetrics(data);
        setIsConnected(true);

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
      } catch (e) {
        console.error('Failed to parse SSE metrics data', e);
      }
    };

    es.onerror = () => {
      setIsConnected(false);
      setError('Connection interrupted. Reconnecting...');
    };

    return () => {
      es.close();
      eventSourceRef.current = null;
    };
  }, [enabled]);

  return { metrics, history, isConnected, error };
}

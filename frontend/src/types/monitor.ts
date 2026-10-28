export interface ProcessMetric {
  pid: number;
  name: string;
  cpu_usage: number;
  memory_bytes: number;
}

export interface DiskMetric {
  name: string;
  mount_point: string;
  total_bytes: number;
  available_bytes: number;
}

export interface HostMetrics {
  cpu_usage_pct: number;
  cpu_cores: number[];
  memory_used_bytes: number;
  memory_total_bytes: number;
  swap_used_bytes: number;
  swap_total_bytes: number;
  load_avg_1m: number;
  load_avg_5m: number;
  load_avg_15m: number;
  uptime_seconds: number;
  hostname: string;
  os_name: string;
  disks: DiskMetric[];
  top_processes: ProcessMetric[];
  timestamp: number;
}

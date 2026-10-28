use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatusResponse {
    pub is_claimed: bool,
    pub is_authenticated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPasswordRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub login_time: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSessionsResponse {
    pub sessions: Vec<SessionInfo>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PandocConvertRequest {
    pub markdown: String,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessMetric {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiskMetric {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostMetrics {
    pub cpu_usage_pct: f32,
    pub cpu_cores: Vec<f32>,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub load_avg_1m: f64,
    pub load_avg_5m: f64,
    pub load_avg_15m: f64,
    pub uptime_seconds: u64,
    pub hostname: String,
    pub os_name: String,
    pub disks: Vec<DiskMetric>,
    pub top_processes: Vec<ProcessMetric>,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_metrics_serde_roundtrip() {
        let metrics = HostMetrics {
            cpu_usage_pct: 42.5,
            cpu_cores: vec![40.0, 45.0],
            memory_used_bytes: 4 * 1024 * 1024 * 1024,
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            swap_used_bytes: 1024 * 1024,
            swap_total_bytes: 2 * 1024 * 1024 * 1024,
            load_avg_1m: 0.85,
            load_avg_5m: 1.12,
            load_avg_15m: 0.95,
            uptime_seconds: 3600,
            hostname: "test-node".to_string(),
            os_name: "Linux".to_string(),
            disks: vec![DiskMetric {
                name: "root".to_string(),
                mount_point: "/".to_string(),
                total_bytes: 50 * 1024 * 1024 * 1024,
                available_bytes: 25 * 1024 * 1024 * 1024,
            }],
            top_processes: vec![ProcessMetric {
                pid: 1234,
                name: "pandoc".to_string(),
                cpu_usage: 95.0,
                memory_bytes: 256 * 1024 * 1024,
            }],
            timestamp: 1700000000,
        };

        let json = serde_json::to_string(&metrics).expect("Serialization failed");
        let decoded: HostMetrics = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(metrics, decoded);
    }

    #[test]
    fn test_empty_disks_and_processes() {
        let metrics = HostMetrics {
            cpu_usage_pct: 0.0,
            cpu_cores: vec![],
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            swap_used_bytes: 0,
            swap_total_bytes: 0,
            load_avg_1m: 0.0,
            load_avg_5m: 0.0,
            load_avg_15m: 0.0,
            uptime_seconds: 0,
            hostname: "".to_string(),
            os_name: "".to_string(),
            disks: vec![],
            top_processes: vec![],
            timestamp: 0,
        };

        let json = serde_json::to_string(&metrics).expect("Serialization failed");
        let decoded: HostMetrics = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(metrics, decoded);
    }
}

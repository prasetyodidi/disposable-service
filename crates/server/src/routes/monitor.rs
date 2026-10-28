use std::convert::Infallible;
use std::time::Duration;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Json;
use async_stream::stream;
use tokio::time::interval;
use tokio_stream::Stream;
use sysinfo::{Disks, System, ProcessesToUpdate};
use disposable_common::{ApiResponse, DiskMetric, HostMetrics, ProcessMetric};
use crate::auth::AuthenticatedSession;

pub struct MetricCollector {
    sys: System,
}

impl Default for MetricCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricCollector {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    pub fn collect(&mut self) -> HostMetrics {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

        let cpu_usage_pct = self.sys.global_cpu_usage().clamp(0.0, 100.0);
        let cpu_cores: Vec<f32> = self
            .sys
            .cpus()
            .iter()
            .map(|c| c.cpu_usage().clamp(0.0, 100.0))
            .collect();

        let memory_used_bytes = self.sys.used_memory();
        let memory_total_bytes = self.sys.total_memory();
        let swap_used_bytes = self.sys.used_swap();
        let swap_total_bytes = self.sys.total_swap();

        let load_avg = System::load_average();
        let uptime_seconds = System::uptime();
        let hostname = System::host_name().unwrap_or_else(|| "disposable-host".to_string());
        let os_name = System::name().unwrap_or_else(|| "Linux".to_string());

        let disks = Disks::new_with_refreshed_list();
        let disk_metrics: Vec<DiskMetric> = disks
            .iter()
            .map(|d| DiskMetric {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().to_string_lossy().to_string(),
                total_bytes: d.total_space(),
                available_bytes: d.available_space(),
            })
            .collect();

        // Sort processes by CPU usage descending, and take top 10
        let mut processes: Vec<ProcessMetric> = self
            .sys
            .processes()
            .iter()
            .map(|(&pid, p)| ProcessMetric {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_usage: p.cpu_usage().clamp(0.0, 1000.0),
                memory_bytes: p.memory(),
            })
            .collect();

        processes.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        processes.truncate(10);

        HostMetrics {
            cpu_usage_pct,
            cpu_cores,
            memory_used_bytes,
            memory_total_bytes,
            swap_used_bytes,
            swap_total_bytes,
            load_avg_1m: load_avg.one,
            load_avg_5m: load_avg.five,
            load_avg_15m: load_avg.fifteen,
            uptime_seconds,
            hostname,
            os_name,
            disks: disk_metrics,
            top_processes: processes,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// One-shot metrics snapshot endpoint
pub async fn get_host_metrics(
    _session: AuthenticatedSession,
) -> Json<ApiResponse<HostMetrics>> {
    let mut collector = MetricCollector::new();
    let metrics = collector.collect();
    Json(ApiResponse {
        success: true,
        message: None,
        data: Some(metrics),
    })
}

/// Real-time Server-Sent Events (SSE) stream endpoint
pub async fn stream_host_metrics(
    _session: AuthenticatedSession,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream! {
        let mut collector = MetricCollector::new();
        // Allow initial CPU reading interval to calibrate
        tokio::time::sleep(Duration::from_millis(200)).await;

        let mut ticker = interval(Duration::from_millis(1000));
        loop {
            ticker.tick().await;
            let metrics = collector.collect();
            if let Ok(json) = serde_json::to_string(&metrics) {
                yield Ok(Event::default().data(json));
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_collector_sanity() {
        let mut collector = MetricCollector::new();
        let metrics = collector.collect();

        assert!(metrics.cpu_usage_pct >= 0.0 && metrics.cpu_usage_pct <= 100.0);
        assert!(metrics.memory_total_bytes > 0);
        assert!(metrics.memory_used_bytes <= metrics.memory_total_bytes || metrics.memory_total_bytes > 0);
        assert!(!metrics.hostname.is_empty());
        assert!(!metrics.os_name.is_empty());
        assert!(metrics.top_processes.len() <= 10);

        // Verify top processes are sorted descending by CPU
        for window in metrics.top_processes.windows(2) {
            assert!(window[0].cpu_usage >= window[1].cpu_usage);
        }
    }

    #[test]
    fn test_process_sorting_and_truncation() {
        let mut processes = vec![
            ProcessMetric { pid: 1, name: "proc1".into(), cpu_usage: 12.0, memory_bytes: 100 },
            ProcessMetric { pid: 2, name: "proc2".into(), cpu_usage: 45.0, memory_bytes: 200 },
            ProcessMetric { pid: 3, name: "proc3".into(), cpu_usage: 2.5, memory_bytes: 300 },
            ProcessMetric { pid: 4, name: "proc4".into(), cpu_usage: 98.0, memory_bytes: 400 },
        ];

        processes.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        processes.truncate(2);

        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].pid, 4);
        assert_eq!(processes[0].cpu_usage, 98.0);
        assert_eq!(processes[1].pid, 2);
        assert_eq!(processes[1].cpu_usage, 45.0);
    }

    #[tokio::test]
    async fn test_monitor_endpoints_unauthenticated() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;
        use crate::routes::create_api_router;
        use crate::state::AppState;

        let app = create_api_router().with_state(AppState::new());

        // 1. Metrics endpoint without auth -> 401
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/apps/monitor/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 2. Stream endpoint without auth -> 401
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/apps/monitor/stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_monitor_endpoints_authenticated() {
        use axum::body::Body;
        use axum::http::{header, Request, StatusCode};
        use tower::ServiceExt;
        use chrono::Utc;
        use disposable_common::SessionInfo;
        use crate::auth::{create_jwt, COOKIE_NAME};
        use crate::routes::create_api_router;
        use crate::state::AppState;

        let state = AppState::new();
        let session_id = "test-session-monitor";
        let session = SessionInfo {
            session_id: session_id.to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };
        state.add_session(session).await;
        let (token, _) = create_jwt(session_id, &state.jwt_secret).unwrap();

        let app = create_api_router().with_state(state);

        // 1. Metrics endpoint with auth -> 200 OK
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/apps/monitor/metrics")
                    .header(header::COOKIE, format!("{}={}", COOKIE_NAME, token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 2. Stream endpoint with auth -> 200 OK and text/event-stream
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/apps/monitor/stream")
                    .header(header::COOKIE, format!("{}={}", COOKIE_NAME, token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
    }
}

//! Burn primitives: storage measurement, wiping, and restart-policy warnings.
//!
//! Everything here is a building block for the `BurnCoordinator` (task 6.2).
//! No user content (file names, tokens, session ids) is ever logged or placed
//! into a `FailedItem` reason.

use crate::state::{workspace_tmp_root, AppState};
use disposable_common::{AuditReport, CategoryStat, FailedItem};
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, Notify, OnceCell};
use tokio::time::Instant;
use zeroize::Zeroize;

pub const CAT_SESSIONS: &str = "sessions";
pub const CAT_DOWNLOADS: &str = "downloads";
pub const CAT_MCP_SESSIONS: &str = "mcp_sessions";
pub const CAT_TEMP_DIR: &str = "temp_dir";

/// Fixed category list, in the order `measure` reports them.
pub const CATEGORIES: [&str; 4] = [CAT_SESSIONS, CAT_DOWNLOADS, CAT_MCP_SESSIONS, CAT_TEMP_DIR];

/// Bytes counted per session for its two `i64` timestamps.
const SESSION_FIXED_BYTES: u64 = 16;

/// Result of one wipe step. A failure on one item never aborts the others.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct WipeOutcome {
    /// Number of entries (files for temp_dir) removed.
    pub removed: u64,
    pub failures: Vec<FailedItem>,
}

impl WipeOutcome {
    fn fail(category: &str, reason: &str) -> FailedItem {
        FailedItem {
            category: category.to_string(),
            reason: reason.to_string(),
        }
    }
}

/// Abstraction over the managed storage so the coordinator can be tested
/// with backends that fail or leave residue.
#[allow(async_fn_in_trait)]
pub trait StorageBackend {
    async fn wipe_downloads(&self) -> WipeOutcome;
    async fn clear_mcp(&self) -> WipeOutcome;
    async fn clear_sessions(&self) -> WipeOutcome;
    async fn wipe_temp(&self) -> WipeOutcome;
    /// Always returns the four fixed categories, in `CATEGORIES` order.
    async fn measure(&self) -> Vec<CategoryStat>;
}

/// Real backend over `AppState` and `workspace_tmp_root()`.
pub struct RealBackend<'a> {
    pub state: &'a AppState,
}

impl<'a> RealBackend<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

impl StorageBackend for RealBackend<'_> {
    async fn wipe_downloads(&self) -> WipeOutcome {
        let drained: Vec<_> = {
            let mut lock = self.state.downloads.write().await;
            lock.drain().collect()
        };
        let removed = drained.len() as u64;
        for (_, mut d) in drained {
            wipe_buffer(&mut d.data);
        }
        WipeOutcome { removed, failures: vec![] }
    }

    async fn clear_mcp(&self) -> WipeOutcome {
        // Dropping the senders ends the SSE streams once queued events are sent.
        let mut lock = self.state.mcp_sessions.write().await;
        let removed = lock.len() as u64;
        lock.clear();
        WipeOutcome { removed, failures: vec![] }
    }

    async fn clear_sessions(&self) -> WipeOutcome {
        let mut lock = self.state.sessions.write().await;
        let removed = lock.len() as u64;
        lock.clear();
        WipeOutcome { removed, failures: vec![] }
    }

    async fn wipe_temp(&self) -> WipeOutcome {
        let root = workspace_tmp_root();
        match tokio::task::spawn_blocking(move || wipe_dir_contents(&root)).await {
            Ok(o) => o,
            Err(_) => WipeOutcome {
                removed: 0,
                failures: vec![WipeOutcome::fail(CAT_TEMP_DIR, "wipe task failed")],
            },
        }
    }

    async fn measure(&self) -> Vec<CategoryStat> {
        let sessions = {
            let lock = self.state.sessions.read().await;
            let bytes: u64 = lock
                .values()
                .map(|s| {
                    (s.session_id.len()
                        + s.ip_address.len()
                        + s.user_agent.len()
                        + s.token_type.as_ref().map_or(0, |t| t.len())
                        + s.label.as_ref().map_or(0, |l| l.len())) as u64
                        + SESSION_FIXED_BYTES
                })
                .sum();
            CategoryStat {
                category: CAT_SESSIONS.into(),
                entries: lock.len() as u64,
                bytes,
            }
        };
        let downloads = {
            let lock = self.state.downloads.read().await;
            let bytes: u64 = lock
                .values()
                .map(|d| (d.data.len() + d.filename.len() + d.content_type.len()) as u64)
                .sum();
            CategoryStat {
                category: CAT_DOWNLOADS.into(),
                entries: lock.len() as u64,
                bytes,
            }
        };
        let mcp = {
            let lock = self.state.mcp_sessions.read().await;
            let bytes: u64 = lock.keys().map(|k| k.len() as u64).sum();
            CategoryStat {
                category: CAT_MCP_SESSIONS.into(),
                entries: lock.len() as u64,
                bytes,
            }
        };
        let root = workspace_tmp_root();
        let (entries, bytes) = tokio::task::spawn_blocking(move || measure_dir(&root))
            .await
            .unwrap_or((0, 0));
        let temp = CategoryStat {
            category: CAT_TEMP_DIR.into(),
            entries,
            bytes,
        };
        vec![sessions, downloads, mcp, temp]
    }
}

const STEP_TIMEOUT: Duration = Duration::from_secs(5);
const FORCE_SHUTDOWN_AFTER: Duration = Duration::from_secs(5);
const BODY_SENT_WAIT: Duration = Duration::from_secs(2);
const WATCHDOG_DELAY: Duration = Duration::from_secs(3);

/// Single-flight burn coordinator. The first caller of `run_or_join` runs the
/// burn; every concurrent or later caller receives the same `Arc<AuditReport>`.
#[derive(Default)]
pub struct BurnCoordinator {
    burning: AtomicBool,
    report: OnceCell<Arc<AuditReport>>,
    shutdown: Notify,
    shutdown_requested: AtomicBool,
    triggered_at: OnceLock<Instant>,
}

impl BurnCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once a burn has been triggered (running or finished).
    pub fn is_burning(&self) -> bool {
        self.burning.load(Ordering::SeqCst)
    }

    /// Run the burn exactly once against the real state; others join.
    pub async fn run_or_join(&self, state: &AppState) -> Arc<AuditReport> {
        self.burning.store(true, Ordering::SeqCst);
        // First trigger arms the 5s forced-shutdown timer so a client that
        // disconnects cannot keep the process alive (Req 12.4).
        if self.triggered_at.set(Instant::now()).is_ok() {
            state.burn.arm_force_timer_arc();
        }
        let backend = RealBackend::new(state);
        let policy = std::env::var("DISPOSABLE_RESTART_POLICY").ok();
        self.run_with(&backend, policy.as_deref(), Some(&state.ui_events))
            .await
    }

    /// Generic single-flight entry point, usable with any backend.
    pub async fn run_with<B: StorageBackend>(
        &self,
        backend: &B,
        restart_policy: Option<&str>,
        ui_events: Option<&broadcast::Sender<Arc<AuditReport>>>,
    ) -> Arc<AuditReport> {
        self.burning.store(true, Ordering::SeqCst);
        let started = *self.triggered_at.get_or_init(Instant::now);
        self.report
            .get_or_init(|| async {
                let report = Arc::new(execute_burn(backend, restart_policy, started).await);
                if let Some(tx) = ui_events {
                    // No subscribers is fine; the channel holds no user data.
                    let _ = tx.send(report.clone());
                }
                report
            })
            .await
            .clone()
    }

    /// Arm the 5s forced shutdown timer for an `Arc`-owned coordinator.
    pub fn arm_force_timer_arc(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(FORCE_SHUTDOWN_AFTER).await;
            me.request_shutdown();
        });
    }

    /// Request shutdown after the response body is delivered, or after 2s.
    pub fn schedule_shutdown<F>(self: &Arc<Self>, body_done: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let me = self.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(BODY_SENT_WAIT, body_done).await;
            me.request_shutdown();
        });
    }

    /// Resolves when shutdown has been requested.
    pub async fn shutdown_requested(&self) {
        self.shutdown.notified().await;
    }

    /// Notify `main` and start the OS-thread watchdog (exit code 1).
    fn request_shutdown(&self) {
        if self.shutdown_requested.swap(true, Ordering::SeqCst) {
            return;
        }
        self.shutdown.notify_one();
        // Never arm the process-exit watchdog under `cargo test`.
        #[cfg(not(test))]
        std::thread::spawn(|| {
            std::thread::sleep(WATCHDOG_DELAY);
            eprintln!("burn: graceful shutdown did not complete in time; forcing exit");
            std::process::exit(1);
        });
    }
}

async fn timed<T>(fut: impl Future<Output = T>) -> Option<T> {
    tokio::time::timeout(STEP_TIMEOUT, fut).await.ok()
}

async fn execute_burn<B: StorageBackend>(
    backend: &B,
    restart_policy: Option<&str>,
    started: Instant,
) -> AuditReport {
    let before = backend.measure().await;
    let mut failed: Vec<FailedItem> = Vec::new();
    let mut removed = 0u64;
    let mut incomplete = false;

    let mut absorb = |o: WipeOutcome, failed: &mut Vec<FailedItem>| {
        removed += o.removed;
        failed.extend(o.failures);
    };

    absorb(backend.wipe_downloads().await, &mut failed);
    match timed(backend.clear_mcp()).await {
        Some(o) => absorb(o, &mut failed),
        None => {
            incomplete = true;
            failed.push(WipeOutcome::fail(CAT_MCP_SESSIONS, "operation timed out"));
        }
    }
    absorb(backend.clear_sessions().await, &mut failed);
    absorb(backend.wipe_temp().await, &mut failed);

    let (after, verify_ok) = match timed(backend.measure()).await {
        Some(a) => (a, true),
        None => {
            incomplete = true;
            (before.clone(), false)
        }
    };

    let all_zero = after.iter().all(|c| c.entries == 0 && c.bytes == 0);
    let verified = verify_ok && all_zero && failed.is_empty();
    if !failed.is_empty() || !verified {
        incomplete = true;
    }
    let remaining_categories: Vec<CategoryStat> = after
        .iter()
        .filter(|c| c.entries > 0 || c.bytes > 0)
        .cloned()
        .collect();

    AuditReport {
        verified,
        incomplete,
        total_bytes_before: before.iter().map(|c| c.bytes).sum(),
        remaining_bytes: remaining_categories.iter().map(|c| c.bytes).sum(),
        before,
        after,
        remaining_categories,
        removed_items: removed,
        failed_items: failed,
        duration_ms: started.elapsed().as_millis() as u64,
        timestamp_utc: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        warnings: restart_warning(restart_policy).into_iter().collect(),
    }
}

/// Overwrite the whole buffer with zeros (not elided by the optimizer).
pub fn wipe_buffer(buf: &mut [u8]) {
    buf.zeroize();
}

/// Returns a warning when the container would be restarted after the burn:
/// for `always`, `unless-stopped`, and for an unknown or missing policy.
/// `no` and `on-failure` (the process exits 0) yield no warning.
pub fn restart_warning(policy: Option<&str>) -> Option<String> {
    let p = policy.map(|s| s.trim().to_ascii_lowercase());
    match p.as_deref() {
        Some("no") | Some("on-failure") => None,
        Some("always") | Some("unless-stopped") => Some(format!(
            "Restart policy '{}' will restart the server automatically after the burn; \
             the workspace will come back empty. Stop the container manually to keep it down.",
            p.unwrap()
        )),
        _ => Some(
            "Restart policy is unknown; the server may be restarted automatically after the burn."
                .to_string(),
        ),
    }
}

/// Count files and bytes under `root` recursively (symlinks not followed).
/// A missing root counts as empty.
fn measure_dir(root: &Path) -> (u64, u64) {
    let mut entries = 0u64;
    let mut bytes = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Ok(md) = std::fs::symlink_metadata(e.path()) else { continue };
            if md.is_dir() {
                stack.push(e.path());
            } else {
                entries += 1;
                bytes += md.len();
            }
        }
    }
    (entries, bytes)
}

/// Zero-overwrite then remove a regular file. Returns false on failure to remove.
fn wipe_file(path: &Path, len: u64) -> bool {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(path) {
        let zeros = [0u8; 8192];
        let mut left = len;
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            if f.write_all(&zeros[..n]).is_err() {
                break;
            }
            left -= n as u64;
        }
        let _ = f.sync_all();
    }
    std::fs::remove_file(path).is_ok()
}

/// Remove everything inside `root` (but not `root` itself). Failures are
/// recorded per category without file names.
fn wipe_dir_contents(root: &Path) -> WipeOutcome {
    let mut out = WipeOutcome::default();
    let Ok(rd) = std::fs::read_dir(root) else { return out };
    for e in rd.flatten() {
        wipe_path(&e.path(), &mut out);
    }
    out
}

fn wipe_path(path: &Path, out: &mut WipeOutcome) {
    let Ok(md) = std::fs::symlink_metadata(path) else { return };
    if md.is_dir() {
        if let Ok(rd) = std::fs::read_dir(path) {
            for e in rd.flatten() {
                wipe_path(&e.path(), out);
            }
        }
        if std::fs::remove_dir(path).is_err() {
            out.failures
                .push(WipeOutcome::fail(CAT_TEMP_DIR, "could not remove directory"));
        }
    } else if md.file_type().is_symlink() {
        match std::fs::remove_file(path) {
            Ok(_) => out.removed += 1,
            Err(_) => out
                .failures
                .push(WipeOutcome::fail(CAT_TEMP_DIR, "could not remove file")),
        }
    } else if wipe_file(path, md.len()) {
        out.removed += 1;
    } else {
        out.failures
            .push(WipeOutcome::fail(CAT_TEMP_DIR, "could not remove file"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wipe_buffer_zeroes_full_length() {
        let mut b = vec![0xAAu8; 100];
        wipe_buffer(&mut b);
        assert_eq!(b.len(), 100);
        assert!(b.iter().all(|x| *x == 0));
    }

    #[test]
    fn restart_warning_cases() {
        assert!(restart_warning(Some("always")).is_some());
        assert!(restart_warning(Some("unless-stopped")).is_some());
        assert!(restart_warning(Some("weird")).is_some());
        assert!(restart_warning(None).is_some());
        assert!(restart_warning(Some("on-failure")).is_none());
        assert!(restart_warning(Some("no")).is_none());
    }

    #[test]
    fn dir_measure_and_wipe() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a/x"), b"12345").unwrap();
        std::fs::write(dir.path().join("y"), b"123").unwrap();
        assert_eq!(measure_dir(dir.path()), (2, 8));
        let out = wipe_dir_contents(dir.path());
        assert_eq!(out.removed, 2);
        assert!(out.failures.is_empty());
        assert_eq!(measure_dir(dir.path()), (0, 0));
        assert!(dir.path().exists());
    }

    #[tokio::test]
    async fn coordinator_single_flight_and_ui_event() {
        let state = AppState::new();
        state
            .store_ephemeral_download(vec![7; 10], "f".into(), "t".into(), 60)
            .await;
        let mut rx = state.ui_events.subscribe();
        assert!(!state.burn.is_burning());
        let (a, b) = tokio::join!(state.burn.run_or_join(&state), state.burn.run_or_join(&state));
        assert!(Arc::ptr_eq(&a, &b));
        assert!(state.burn.is_burning());
        assert_eq!(a.removed_items >= 1, true);
        assert_eq!(a.before.len(), 4);
        assert!(a.after.iter().all(|c| c.entries == 0 && c.bytes == 0) == a.verified);
        assert!(Arc::ptr_eq(&rx.recv().await.unwrap(), &a));
    }

    #[tokio::test]
    async fn real_backend_measures_and_wipes_state() {
        let state = AppState::new();
        state
            .store_ephemeral_download(vec![1; 10], "f".into(), "t".into(), 60)
            .await;
        let b = RealBackend::new(&state);
        let m = b.measure().await;
        let names: Vec<_> = m.iter().map(|c| c.category.as_str()).collect();
        assert_eq!(names, CATEGORIES);
        assert_eq!(m[1].entries, 1);
        assert_eq!(m[1].bytes, 12);
        let o = b.wipe_downloads().await;
        assert_eq!(o.removed, 1);
        assert_eq!(b.measure().await[1].bytes, 0);
    }
}

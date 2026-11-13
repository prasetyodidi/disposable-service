//! proptest generators and helpers for the burn service (`privacy/burn.rs`).
//!
//! - `burn_spec()`: random counts of sessions / downloads / MCP sessions and a
//!   random file tree, every item carrying a unique marker (in file names,
//!   session ids, download tokens/filenames and contents).
//! - `populate_state` / `write_tree`: materialise a spec into an `AppState`
//!   and into a `tempfile::tempdir` (production `workspace_tmp_root()` is never
//!   touched).
//! - `FakeBackend`: a `StorageBackend` over a temp dir that can fail whole
//!   steps or leave residue, for Property 30 and the -32603 example test.
//! - `expected_before`: independent computation of the `before` stats.
//!
//! Helpers are `pub(crate)` so the `prop_26..prop_33` tests can reuse them.
#![allow(dead_code)]
use crate::privacy::burn::{
    wipe_buffer, RealBackend, StorageBackend, WipeOutcome, CATEGORIES, CAT_DOWNLOADS,
    CAT_MCP_SESSIONS, CAT_SESSIONS, CAT_TEMP_DIR,
};
use crate::state::{AppState, EphemeralDownload};
use chrono::{Duration, Utc};
use disposable_common::{CategoryStat, FailedItem, SessionInfo};
use proptest::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const MAX_ITEMS: usize = 6;
pub(crate) const MAX_FILES: usize = 8;
pub(crate) const MAX_DEPTH: usize = 3;
pub(crate) const MAX_BLOB: usize = 300;

// ---------------------------------------------------------------------------
// Spec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct SessionSpec {
    pub id: String,
    pub ip: String,
    pub user_agent: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct DownloadSpec {
    pub token: String,
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct FileSpec {
    /// Directory components below the root (may be empty).
    pub dirs: Vec<String>,
    pub name: String,
    pub content: Vec<u8>,
}

impl FileSpec {
    pub fn rel_path(&self) -> PathBuf {
        let mut p = PathBuf::new();
        for d in &self.dirs {
            p.push(d);
        }
        p.push(&self.name);
        p
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BurnSpec {
    /// Unique prefix shared by every marker of this spec.
    pub marker: String,
    pub sessions: Vec<SessionSpec>,
    pub downloads: Vec<DownloadSpec>,
    pub mcp_ids: Vec<String>,
    pub files: Vec<FileSpec>,
    pub empty_dirs: Vec<String>,
}

impl BurnSpec {
    /// Every marker string that is placed somewhere in state or on disk.
    pub fn markers(&self) -> Vec<String> {
        let mut m = vec![];
        m.extend(self.sessions.iter().map(|s| s.id.clone()));
        m.extend(self.downloads.iter().map(|d| d.token.clone()));
        m.extend(self.downloads.iter().map(|d| d.filename.clone()));
        m.extend(self.mcp_ids.iter().cloned());
        m.extend(self.files.iter().map(|f| f.name.clone()));
        m
    }

    /// Number of regular files in the tree (what `temp_dir.entries` counts).
    pub fn file_count(&self) -> u64 {
        self.files.len() as u64
    }
}

fn blob(marker: &str, tail: Vec<u8>) -> Vec<u8> {
    let mut v = marker.as_bytes().to_vec();
    v.extend(tail);
    v
}

/// Random burn spec: 0..=6 sessions, downloads and MCP sessions, 0..=8 files
/// spread over up to 3 directory levels, plus a few empty directories.
pub(crate) fn burn_spec() -> impl Strategy<Value = BurnSpec> {
    (
        any::<u32>(),
        0..=MAX_ITEMS,
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..MAX_BLOB), 0..=MAX_ITEMS),
        0..=MAX_ITEMS,
        prop::collection::vec(
            (0..=MAX_DEPTH, prop::collection::vec(any::<u8>(), 0..MAX_BLOB)),
            0..=MAX_FILES,
        ),
        0..=3usize,
        prop::collection::vec(any::<bool>(), MAX_ITEMS),
    )
        .prop_map(|(seed, n_sessions, dl_data, n_mcp, files, n_empty, labels)| {
            let marker = format!("mk{seed:08x}");
            let sessions = (0..n_sessions)
                .map(|i| SessionSpec {
                    id: format!("{marker}-sess{i}"),
                    ip: format!("10.0.0.{}", i + 1),
                    user_agent: format!("ua-{marker}-{i}"),
                    label: labels[i].then(|| format!("label-{marker}-{i}")),
                })
                .collect();
            let downloads = dl_data
                .into_iter()
                .enumerate()
                .map(|(i, tail)| DownloadSpec {
                    token: format!("{marker}-tok{i}"),
                    filename: format!("{marker}-dl{i}.bin"),
                    content_type: "application/octet-stream".into(),
                    data: blob(&marker, tail),
                })
                .collect();
            let mcp_ids = (0..n_mcp).map(|i| format!("{marker}-mcp{i}")).collect();
            let files = files
                .into_iter()
                .enumerate()
                .map(|(i, (depth, tail))| FileSpec {
                    dirs: (0..depth).map(|d| format!("{marker}-dir{d}")).collect(),
                    name: format!("{marker}-file{i}.dat"),
                    content: blob(&marker, tail),
                })
                .collect();
            let empty_dirs = (0..n_empty).map(|i| format!("{marker}-empty{i}")).collect();
            BurnSpec { marker, sessions, downloads, mcp_ids, files, empty_dirs }
        })
}

// ---------------------------------------------------------------------------
// Materialisation
// ---------------------------------------------------------------------------

/// Keeps the MCP receivers alive so senders stay open until burned.
pub(crate) struct Populated {
    pub receivers: Vec<tokio::sync::mpsc::Receiver<axum::response::sse::Event>>,
}

/// Fill `state` with the spec's sessions, downloads and MCP sessions
/// (inserted directly so nothing is purged or renamed).
pub(crate) async fn populate_state(state: &AppState, spec: &BurnSpec) -> Populated {
    let now = Utc::now();
    {
        let mut lock = state.sessions.write().await;
        for s in &spec.sessions {
            lock.insert(
                s.id.clone(),
                SessionInfo {
                    session_id: s.id.clone(),
                    ip_address: s.ip.clone(),
                    user_agent: s.user_agent.clone(),
                    login_time: now.timestamp(),
                    expires_at: (now + Duration::hours(1)).timestamp(),
                    token_type: None,
                    label: s.label.clone(),
                },
            );
        }
    }
    {
        let mut lock = state.downloads.write().await;
        for d in &spec.downloads {
            lock.insert(
                d.token.clone(),
                EphemeralDownload {
                    data: d.data.clone(),
                    filename: d.filename.clone(),
                    content_type: d.content_type.clone(),
                    expires_at: now + Duration::minutes(5),
                },
            );
        }
    }
    let mut receivers = vec![];
    {
        let mut lock = state.mcp_sessions.write().await;
        for id in &spec.mcp_ids {
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            lock.insert(id.clone(), tx);
            receivers.push(rx);
        }
    }
    Populated { receivers }
}

/// Create the spec's file tree under `root` (which must exist).
pub(crate) fn write_tree(root: &Path, spec: &BurnSpec) -> std::io::Result<()> {
    for f in &spec.files {
        let path = root.join(f.rel_path());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &f.content)?;
    }
    for d in &spec.empty_dirs {
        std::fs::create_dir_all(root.join(d))?;
    }
    Ok(())
}

/// Convenience: fresh `AppState` + temp dir populated from `spec`.
pub(crate) async fn build_world(
    spec: &BurnSpec,
) -> (AppState, tempfile::TempDir, Populated) {
    let state = AppState::new();
    let populated = populate_state(&state, spec).await;
    let dir = tempfile::tempdir().expect("tempdir");
    write_tree(dir.path(), spec).expect("write tree");
    (state, dir, populated)
}

// ---------------------------------------------------------------------------
// Independent measurement (does not use burn.rs code paths)
// ---------------------------------------------------------------------------

fn stat(category: &str, entries: u64, bytes: u64) -> CategoryStat {
    CategoryStat { category: category.into(), entries, bytes }
}

/// Expected `before` stats computed straight from the spec using the design's
/// byte rules. Order matches `CATEGORIES`.
pub(crate) fn expected_before(spec: &BurnSpec) -> Vec<CategoryStat> {
    let sessions: u64 = spec
        .sessions
        .iter()
        .map(|s| {
            (s.id.len() + s.ip.len() + s.user_agent.len() + s.label.as_ref().map_or(0, |l| l.len()))
                as u64
                + 16
        })
        .sum();
    let downloads: u64 = spec
        .downloads
        .iter()
        .map(|d| (d.data.len() + d.filename.len() + d.content_type.len()) as u64)
        .sum();
    let mcp: u64 = spec.mcp_ids.iter().map(|k| k.len() as u64).sum();
    let temp: u64 = spec.files.iter().map(|f| f.content.len() as u64).sum();
    vec![
        stat(CAT_SESSIONS, spec.sessions.len() as u64, sessions),
        stat(CAT_DOWNLOADS, spec.downloads.len() as u64, downloads),
        stat(CAT_MCP_SESSIONS, spec.mcp_ids.len() as u64, mcp),
        stat(CAT_TEMP_DIR, spec.file_count(), temp),
    ]
}

/// Count regular files and bytes under `root` (missing root = empty).
pub(crate) fn dir_stats(root: &Path) -> (u64, u64) {
    let (mut entries, mut bytes) = (0u64, 0u64);
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

/// True if any file or directory name under `root` contains `needle`,
/// or any regular file's content contains it.
pub(crate) fn tree_contains(root: &Path, needle: &str) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().contains(needle) {
                return true;
            }
            let Ok(md) = std::fs::symlink_metadata(e.path()) else { continue };
            if md.is_dir() {
                stack.push(e.path());
            } else if let Ok(bytes) = std::fs::read(e.path()) {
                if bytes.windows(needle.len().max(1)).any(|w| w == needle.as_bytes()) {
                    return true;
                }
            }
        }
    }
    false
}

/// True if any marker of the spec is still present in `state` (session ids,
/// download tokens/filenames/data, MCP ids) or in the tree under `root`.
pub(crate) async fn any_marker_left(state: &AppState, root: &Path, spec: &BurnSpec) -> bool {
    {
        let s = state.sessions.read().await;
        if s.keys().any(|k| k.contains(&spec.marker))
            || s.values().any(|v| v.user_agent.contains(&spec.marker))
        {
            return true;
        }
    }
    {
        let d = state.downloads.read().await;
        if d.iter().any(|(k, v)| {
            k.contains(&spec.marker)
                || v.filename.contains(&spec.marker)
                || v.data.windows(spec.marker.len()).any(|w| w == spec.marker.as_bytes())
        }) {
            return true;
        }
    }
    if state.mcp_sessions.read().await.keys().any(|k| k.contains(&spec.marker)) {
        return true;
    }
    tree_contains(root, &spec.marker)
}

// ---------------------------------------------------------------------------
// FakeBackend
// ---------------------------------------------------------------------------

/// Which cleanup steps fail (leaving their data in place).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FaultPlan {
    pub fail_downloads: bool,
    pub fail_mcp: bool,
    pub fail_sessions: bool,
    pub fail_temp: bool,
}

impl FaultPlan {
    pub fn any(&self) -> bool {
        self.fail_downloads || self.fail_mcp || self.fail_sessions || self.fail_temp
    }

    /// Categories whose step fails, in `CATEGORIES` order.
    pub fn failed_categories(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.fail_sessions {
            v.push(CAT_SESSIONS);
        }
        if self.fail_downloads {
            v.push(CAT_DOWNLOADS);
        }
        if self.fail_mcp {
            v.push(CAT_MCP_SESSIONS);
        }
        if self.fail_temp {
            v.push(CAT_TEMP_DIR);
        }
        v
    }
}

/// Residue reported by `measure` after any wipe step ran, even if the wipe
/// itself "succeeded" (simulates bytes that survive a wipe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Residue {
    /// Index into `CATEGORIES`.
    pub category: usize,
    pub entries: u64,
    pub bytes: u64,
}

pub(crate) fn fault_plan() -> impl Strategy<Value = FaultPlan> {
    (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(|(d, m, s, t)| {
        FaultPlan { fail_downloads: d, fail_mcp: m, fail_sessions: s, fail_temp: t }
    })
}

pub(crate) fn residue() -> impl Strategy<Value = Option<Residue>> {
    prop::option::of((0..CATEGORIES.len(), 1u64..=3, 1u64..=4096).prop_map(
        |(category, entries, bytes)| Residue { category, entries, bytes },
    ))
}

/// `StorageBackend` over an `AppState` and an arbitrary temp root. Healthy
/// steps really wipe (state via `RealBackend`, files via zero-and-remove);
/// failing steps report a `FailedItem` and leave data untouched.
pub(crate) struct FakeBackend<'a> {
    pub state: &'a AppState,
    pub root: PathBuf,
    pub faults: FaultPlan,
    pub residue: Option<Residue>,
    wiped: AtomicBool,
}

impl<'a> FakeBackend<'a> {
    pub fn new(state: &'a AppState, root: &Path) -> Self {
        Self {
            state,
            root: root.to_path_buf(),
            faults: FaultPlan::default(),
            residue: None,
            wiped: AtomicBool::new(false),
        }
    }

    pub fn with_faults(mut self, faults: FaultPlan) -> Self {
        self.faults = faults;
        self
    }

    pub fn with_residue(mut self, residue: Option<Residue>) -> Self {
        self.residue = residue;
        self
    }

    fn failure(category: &str) -> WipeOutcome {
        WipeOutcome {
            removed: 0,
            failures: vec![FailedItem {
                category: category.into(),
                reason: "injected failure".into(),
            }],
        }
    }
}

fn wipe_tree(root: &Path) -> WipeOutcome {
    let mut removed = 0u64;
    let Ok(rd) = std::fs::read_dir(root) else { return WipeOutcome::default() };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        if md.is_dir() {
            removed += wipe_tree(&p).removed;
            let _ = std::fs::remove_dir(&p);
        } else {
            let mut zeros = vec![0u8; md.len() as usize];
            wipe_buffer(&mut zeros);
            let _ = std::fs::write(&p, &zeros);
            if std::fs::remove_file(&p).is_ok() {
                removed += 1;
            }
        }
    }
    WipeOutcome { removed, failures: vec![] }
}

impl StorageBackend for FakeBackend<'_> {
    async fn wipe_downloads(&self) -> WipeOutcome {
        self.wiped.store(true, Ordering::SeqCst);
        if self.faults.fail_downloads {
            return Self::failure(CAT_DOWNLOADS);
        }
        RealBackend::new(self.state).wipe_downloads().await
    }

    async fn clear_mcp(&self) -> WipeOutcome {
        self.wiped.store(true, Ordering::SeqCst);
        if self.faults.fail_mcp {
            return Self::failure(CAT_MCP_SESSIONS);
        }
        RealBackend::new(self.state).clear_mcp().await
    }

    async fn clear_sessions(&self) -> WipeOutcome {
        self.wiped.store(true, Ordering::SeqCst);
        if self.faults.fail_sessions {
            return Self::failure(CAT_SESSIONS);
        }
        RealBackend::new(self.state).clear_sessions().await
    }

    async fn wipe_temp(&self) -> WipeOutcome {
        self.wiped.store(true, Ordering::SeqCst);
        if self.faults.fail_temp {
            return Self::failure(CAT_TEMP_DIR);
        }
        wipe_tree(&self.root)
    }

    async fn measure(&self) -> Vec<CategoryStat> {
        // State categories from the real backend; temp_dir from our own root.
        let mut stats = RealBackend::new(self.state).measure().await;
        let (entries, bytes) = dir_stats(&self.root);
        stats[3] = stat(CAT_TEMP_DIR, entries, bytes);
        if self.wiped.load(Ordering::SeqCst) {
            if let Some(r) = &self.residue {
                stats[r.category].entries += r.entries;
                stats[r.category].bytes += r.bytes;
            }
        }
        stats
    }
}

#[cfg(test)]
mod self_tests {
    use super::*;
    use proptest::test_runner::{Config, TestRunner};

    #[test]
    fn spec_markers_unique_and_expected_matches_real_measure() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut runner = TestRunner::new(Config { cases: 20, ..Config::default() });
        runner
            .run(&burn_spec(), |spec| {
                rt.block_on(async {
                    let (state, dir, _keep) = build_world(&spec).await;
                    let mut m = spec.markers();
                    let n = m.len();
                    m.sort();
                    m.dedup();
                    prop_assert_eq!(m.len(), n);
                    let backend = FakeBackend::new(&state, dir.path());
                    prop_assert_eq!(backend.measure().await, expected_before(&spec));
                    prop_assert!(any_marker_left(&state, dir.path(), &spec).await == (n > 0 || !spec.files.is_empty()));
                    Ok(())
                })
            })
            .unwrap();
    }
}

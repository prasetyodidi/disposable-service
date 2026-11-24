//! Shared helpers for pdf2md tests: tool-skip macro, authed app with an injected
//! `PdfRuntime`, PDF byte builder, temp-dir listing and a `tracing` log capture.
#![allow(dead_code, unused_imports)]
use crate::pdf2md::PdfRuntime;
use crate::routes::create_api_router;
use crate::state::{workspace_tmp_root, AppState};
use axum::Router;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// True when `tool` is an executable file in one of `paths`.
pub(crate) fn tool_in_paths(tool: &str, paths: &std::ffi::OsStr) -> bool {
    std::env::split_paths(paths).any(|dir| is_executable(&dir.join(tool)))
}

fn is_executable(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Checks the process `PATH`.
pub(crate) fn tool_available(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| tool_in_paths(tool, &p))
        .unwrap_or(false)
}

/// Message printed (or panicked with) when a required tool is missing.
pub(crate) fn skip_message(tool: &str) -> String {
    if tool == "pdftohtml" {
        "SKIP: pdftohtml tidak ditemukan di PATH; tes ini membutuhkan poppler-utils".to_string()
    } else {
        format!("SKIP: {tool} tidak ditemukan di PATH; tes ini membutuhkan {tool}")
    }
}

/// `true` when `PDF_MD_REQUIRE_TOOLS=1`.
pub(crate) fn require_tools() -> bool {
    std::env::var("PDF_MD_REQUIRE_TOOLS").map(|v| v == "1").unwrap_or(false)
}

/// Called by `skip_unless_tool!` when the tool is missing: panics under
/// `PDF_MD_REQUIRE_TOOLS=1`, otherwise prints the skip message to stderr.
pub(crate) fn report_missing_tool(tool: &str) {
    let msg = skip_message(tool);
    if require_tools() {
        panic!("{msg} (PDF_MD_REQUIRE_TOOLS=1)");
    }
    eprintln!("{msg}");
}

/// `skip_unless_tool!("pdftohtml")`: returns from the enclosing test when the
/// tool is not on `PATH` (panics when `PDF_MD_REQUIRE_TOOLS=1`).
#[allow(unused_macros)]
macro_rules! skip_unless_tool {
    ($tool:expr) => {
        if !$crate::pdf2md::tests::helpers::tool_available($tool) {
            $crate::pdf2md::tests::helpers::report_missing_tool($tool);
            return;
        }
    };
}
pub(crate) use skip_unless_tool;

/// `PdfRuntime` with default config whose jobs run under `job_root`. Callers keep
/// a `tempfile::tempdir()` alive and pass its path so tests never touch the shared
/// `workspace_tmp_root()`.
pub(crate) fn test_runtime(
    engine: Arc<dyn crate::pdf2md::engine::ExtractionEngine>,
    job_root: &Path,
) -> PdfRuntime {
    PdfRuntime {
        config: crate::pdf2md::config::PdfConfig::defaults(),
        engine,
        job_root: job_root.to_path_buf(),
    }
}

/// Real API router (same as `test_support::http::authed_app`) with `runtime`
/// injected through `AppState::with_pdf`. The Bearer token stays valid because
/// sessions are shared by `Arc`.
pub(crate) async fn authed_pdf_app(runtime: PdfRuntime) -> (Router, AppState, String) {
    debug_assert!(
        runtime.job_root != workspace_tmp_root(),
        "pdf2md tests must use an isolated job_root"
    );
    let (_, state, token) = crate::test_support::http::authed_app().await;
    let state = state.with_pdf(runtime);
    let app = create_api_router(state.clone()).with_state(state.clone());
    (app, state, token)
}

/// `len` bytes starting with `%PDF-` followed by filler (truncated prefix when `len < 5`).
pub(crate) fn pdf_bytes(len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(len);
    v.extend_from_slice(b"%PDF-");
    v.truncate(len);
    while v.len() < len {
        v.push(b'a' + (v.len() % 26) as u8);
    }
    v
}

/// Sorted recursive listing (relative paths) of `workspace_tmp_root()`.
/// Empty when the directory does not exist.
pub(crate) fn tmp_listing() -> Vec<PathBuf> {
    tmp_listing_of(&workspace_tmp_root())
}

pub(crate) fn tmp_listing_of(root: &Path) -> Vec<PathBuf> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            out.push(path.strip_prefix(base).unwrap_or(&path).to_path_buf());
            if entry.file_type()?.is_dir() {
                walk(base, &path, out)?;
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    let _ = walk(root, root, &mut out);
    out.sort();
    out
}

#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedBuf {
    type Writer = SharedBuf;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Captures `tracing` output of the current thread while alive. Use with
/// `#[tokio::test]` (current-thread runtime) so all events land on this thread.
pub(crate) struct LogCapture {
    buf: SharedBuf,
    _guard: tracing::subscriber::DefaultGuard,
}

impl LogCapture {
    /// Everything logged so far (all levels, no ANSI).
    pub fn contents(&self) -> String {
        String::from_utf8_lossy(&self.buf.0.lock().unwrap()).into_owned()
    }
}

pub(crate) fn capture_logs() -> LogCapture {
    let buf = SharedBuf::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    LogCapture { buf, _guard: guard }
}

#[cfg(test)]
mod self_tests {
    use super::*;

    #[test]
    fn pdf_bytes_has_prefix_and_exact_length() {
        assert_eq!(pdf_bytes(0).len(), 0);
        assert_eq!(pdf_bytes(3), b"%PD");
        let v = pdf_bytes(1000);
        assert_eq!(v.len(), 1000);
        assert!(v.starts_with(b"%PDF-"));
    }

    #[test]
    fn tool_lookup_uses_given_paths() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("faketool");
        std::fs::write(&tool, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let paths = std::env::join_paths([dir.path()]).unwrap();
        assert!(tool_in_paths("faketool", &paths));
        assert!(!tool_in_paths("tidak-ada-xyz", &paths));
    }

    #[test]
    fn skip_message_matches_spec() {
        assert_eq!(
            skip_message("pdftohtml"),
            "SKIP: pdftohtml tidak ditemukan di PATH; tes ini membutuhkan poppler-utils"
        );
    }

    #[test]
    fn tmp_listing_of_lists_recursively_and_tolerates_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::write(dir.path().join("a").join("f"), b"x").unwrap();
        let l = tmp_listing_of(dir.path());
        assert_eq!(l, vec![PathBuf::from("a"), PathBuf::from("a/f")]);
        assert!(tmp_listing_of(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn log_capture_collects_events() {
        let cap = capture_logs();
        tracing::warn!("halo-capture");
        assert!(cap.contents().contains("halo-capture"));
    }
}

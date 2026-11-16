//! Unit tests for `proc::run_bounded` (Unix only). No poppler needed.
//! Requirements: 2.4, 2.5, 2.6, 2.7, 2.8, 8.5, 8.6, 16.1.
#![cfg(unix)]
use crate::pdf2md::proc::{run_bounded, Deadline, ProcError, ProcSpec};
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

fn spec<'a>(program: &'a str, args: &[&str], dir: &'a Path, max_stdout: usize) -> ProcSpec<'a> {
    ProcSpec {
        program,
        args: args.iter().map(OsString::from).collect(),
        work_dir: dir,
        max_stdout,
        max_stderr: 1024,
    }
}

/// Live (non-zombie) members of group `pgid` as `(pid, state)` (Linux, via /proc).
/// A zombie still answers `kill(-pgid, 0)` until its parent reaps it. In a
/// container whose PID 1 does not reap orphans, killed grandchildren stay
/// zombies forever, so they must not count as alive.
#[cfg(target_os = "linux")]
fn group_members(pgid: i32) -> Vec<(i32, char)> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return v };
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else { continue };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        // Format: pid (comm) state ppid pgrp ...; comm may contain spaces/parens.
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else { continue };
        let mut f = rest.split_whitespace();
        let state = f.next().and_then(|s| s.chars().next()).unwrap_or('?');
        let _ppid = f.next();
        let pgrp = f.next().and_then(|s| s.parse::<i32>().ok());
        if pgrp == Some(pgid) && state != 'Z' && state != 'X' {
            v.push((pid, state));
        }
    }
    v
}

/// True when no live process of group `pgid` exists.
#[cfg(target_os = "linux")]
fn group_gone(pgid: i32) -> bool {
    group_members(pgid).is_empty()
}

/// True when no process of group `pgid` exists (`kill(-pgid, 0)` -> ESRCH).
#[cfg(not(target_os = "linux"))]
fn group_gone(pgid: i32) -> bool {
    let r = unsafe { libc::kill(-pgid, 0) };
    r == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Polls up to `within` for the group to disappear.
async fn wait_group_gone(pgid: i32, within: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < within {
        if group_gone(pgid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    group_gone(pgid)
}

async fn read_pid(dir: &Path) -> i32 {
    let p = dir.join("pid");
    for _ in 0..100 {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(v) = s.trim().parse::<i32>() {
                return v;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("pid file not written");
}

const PID_SCRIPT: &str = "echo $$ > pid.tmp && mv pid.tmp pid; sleep 30; sleep 30";

#[tokio::test]
async fn sleep_beyond_budget_times_out_and_kills_group() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec("sh", &["-c", PID_SCRIPT], dir.path(), 1024);
    let start = Instant::now();
    let r = run_bounded(&s, &Deadline::new(Duration::from_millis(300))).await;
    assert_eq!(r.unwrap_err(), ProcError::Timeout);
    assert!(start.elapsed() < Duration::from_secs(1), "took {:?}", start.elapsed());
    let pgid = read_pid(dir.path()).await;
    assert!(wait_group_gone(pgid, Duration::from_secs(1)).await, "group {pgid} still alive");
}

#[tokio::test]
async fn plain_sleep_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec("sleep", &["30"], dir.path(), 1024);
    let start = Instant::now();
    let r = run_bounded(&s, &Deadline::new(Duration::from_millis(200))).await;
    assert_eq!(r.unwrap_err(), ProcError::Timeout);
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn stdout_over_cap_is_output_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec("yes", &[], dir.path(), 4096);
    let start = Instant::now();
    let r = run_bounded(&s, &Deadline::new(Duration::from_secs(10))).await;
    assert_eq!(r.unwrap_err(), ProcError::OutputTooLarge);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn dropping_the_future_kills_the_group() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec("sh", &["-c", PID_SCRIPT], dir.path(), 1024);
    let d = Deadline::new(Duration::from_secs(30));
    let r = tokio::time::timeout(Duration::from_millis(500), run_bounded(&s, &d)).await;
    assert!(r.is_err(), "run_bounded should still be running when cancelled");
    let pgid = read_pid(dir.path()).await;
    assert!(wait_group_gone(pgid, Duration::from_secs(1)).await, "group {pgid} still alive");
}

#[tokio::test]
async fn success_captures_stdout_and_status() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec("sh", &["-c", "printf hello; printf err >&2; exit 3"], dir.path(), 1024);
    let o = run_bounded(&s, &Deadline::new(Duration::from_secs(10))).await.unwrap();
    assert_eq!(o.stdout, b"hello");
    assert_eq!(o.stderr_snippet, b"err");
    assert!(!o.status_ok);
    assert_eq!(o.code, Some(3));
}

#[tokio::test]
async fn missing_program_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec("tidak-ada-xyz", &[], dir.path(), 1024);
    let r = run_bounded(&s, &Deadline::new(Duration::from_secs(5))).await;
    assert_eq!(r.unwrap_err(), ProcError::NotFound);
}

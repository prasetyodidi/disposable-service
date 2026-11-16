//! Bounded subprocess runner: no shell, scrubbed environment, own process
//! group, output caps and a cumulative deadline. Never logs stdout/stderr.
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

/// Cumulative time budget shared by all subprocesses of one request.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    end: Instant,
}

impl Deadline {
    pub fn new(total: Duration) -> Self {
        Self {
            end: Instant::now() + total,
        }
    }

    pub fn remaining(&self) -> Duration {
        self.end.saturating_duration_since(Instant::now())
    }
}

pub struct ProcSpec<'a> {
    pub program: &'a str,
    pub args: Vec<OsString>,
    pub work_dir: &'a Path,
    pub max_stdout: usize,
    pub max_stderr: usize,
}

#[derive(Debug)]
pub struct ProcOutput {
    pub status_ok: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr_snippet: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcError {
    NotFound,
    SpawnFailed,
    Timeout,
    OutputTooLarge,
    Io,
}

#[cfg(not(unix))]
pub async fn run_bounded(_spec: &ProcSpec<'_>, _deadline: &Deadline) -> Result<ProcOutput, ProcError> {
    Err(ProcError::NotFound)
}

#[cfg(unix)]
struct PgidGuard(i32);

#[cfg(unix)]
impl PgidGuard {
    fn kill_now(&self) {
        // SAFETY: killpg with a pgid we created via process_group(0).
        unsafe {
            libc::killpg(self.0, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
impl Drop for PgidGuard {
    fn drop(&mut self) {
        self.kill_now();
    }
}

#[cfg(unix)]
async fn read_capped<R: tokio::io::AsyncRead + Unpin>(
    mut r: R,
    cap: usize,
    kill_on_overflow: Option<i32>,
) -> Result<Vec<u8>, ProcError> {
    use tokio::io::AsyncReadExt;
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = r.read(&mut buf).await.map_err(|_| ProcError::Io)?;
        if n == 0 {
            return Ok(out);
        }
        let room = cap.saturating_sub(out.len());
        if n <= room {
            out.extend_from_slice(&buf[..n]);
        } else if let Some(pgid) = kill_on_overflow {
            // stdout over the cap: kill the group, discard everything.
            unsafe {
                libc::killpg(pgid, libc::SIGKILL);
            }
            return Err(ProcError::OutputTooLarge);
        } else {
            // stderr: keep the head, drain and discard the rest.
            out.extend_from_slice(&buf[..room]);
        }
    }
}

#[cfg(unix)]
pub async fn run_bounded(spec: &ProcSpec<'_>, deadline: &Deadline) -> Result<ProcOutput, ProcError> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use tokio::process::Command;

    let home = spec.work_dir.join(".home");
    let cache = spec.work_dir.join(".cache");
    let config = spec.work_dir.join(".config");
    let data = spec.work_dir.join(".data");
    for d in [&home, &cache, &config, &data] {
        std::fs::create_dir_all(d).map_err(|_| ProcError::Io)?;
    }

    let mut cmd = Command::new(spec.program);
    for a in &spec.args {
        cmd.arg(a);
    }
    cmd.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        cmd.env("PATH", path);
    }
    cmd.env("HOME", &home)
        .env("XDG_CACHE_HOME", &cache)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", &data)
        .env("LC_ALL", "C.UTF-8")
        .current_dir(spec.work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd.as_std_mut().process_group(0);

    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ProcError::NotFound,
        _ => ProcError::SpawnFailed,
    })?;
    let pid = child.id().ok_or(ProcError::SpawnFailed)? as i32;
    let guard = PgidGuard(pid);

    let stdout = child.stdout.take().ok_or(ProcError::Io)?;
    let stderr = child.stderr.take().ok_or(ProcError::Io)?;

    let joined = tokio::time::timeout(deadline.remaining(), async {
        tokio::join!(
            read_capped(stdout, spec.max_stdout, Some(pid)),
            read_capped(stderr, spec.max_stderr, None),
            child.wait()
        )
    })
    .await;

    match joined {
        Err(_) => {
            guard.kill_now();
            let _ = child.wait().await;
            Err(ProcError::Timeout)
        }
        Ok((out, err, status)) => {
            // Reap any lingering descendants of the group.
            guard.kill_now();
            let out = match out {
                Ok(v) => v,
                Err(e) => {
                    let _ = child.wait().await;
                    return Err(e);
                }
            };
            let err = err.unwrap_or_default();
            let status = status.map_err(|_| ProcError::Io)?;
            Ok(ProcOutput {
                status_ok: status.success(),
                code: status.code(),
                stdout: out,
                stderr_snippet: err,
            })
        }
    }
}

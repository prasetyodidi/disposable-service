//! Child-process integration tests for PDF to Markdown start-up validation.
//!
//! Validates: Requirements 7.4, 9.3, 9.4
//!
//! Spawns the real `disposable-server` binary on a random port, authenticates
//! over raw HTTP/1.1, triggers `POST /api/burn` and observes the exit status.
//!
//! The binary built for integration tests is compiled without `cfg(test)`, so
//! the exit(1) watchdog in `privacy/burn.rs` is armed here.
//!
//! Validates: Requirements 12.1, 12.4, 12.6

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const PASSWORD: &str = "integration-test-password";

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn spawn_server(envs: &[(&str, &str)]) -> Server {
    let port = free_port();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_disposable-server"));
    cmd.env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("RUST_LOG", "warn")
        .env("JWT_SECRET", "integration-test-jwt-secret")
        .env_remove("DISPOSABLE_PASSWORD_HASH")
        .env_remove("PDF_MD_ENGINE")
        .env_remove("PDF_MD_MAX_BYTES")
        .env_remove("PDF_MD_MAX_RESPONSE_BYTES")
        .env_remove("AWS_LAMBDA_FUNCTION_NAME")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let child = cmd.spawn().expect("spawn server binary");
    let server = Server { child, port };

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return server;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("server did not start listening on port {port}");
}

/// Minimal HTTP/1.1 request; returns (status, header block, body).
fn request(port: u16, method: &str, path: &str, cookie: Option<&str>, body: &str) -> (u16, String, String) {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return (0, String::new(), String::new());
    };
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(c) = cookie {
        req.push_str(&format!("Cookie: {c}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();

    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (status, head.to_string(), body.to_string())
}

/// Claim the instance, log in and return the `name=value` auth cookie.
fn authenticate(port: u16) -> String {
    let body = format!("{{\"password\":\"{PASSWORD}\"}}");
    let (status, _, _) = request(port, "POST", "/api/auth/setup", None, &body);
    assert_eq!(status, 200, "setup failed");
    let (status, head, _) = request(port, "POST", "/api/auth/login", None, &body);
    assert_eq!(status, 200, "login failed");
    let line = head
        .lines()
        .find(|l| l.to_lowercase().starts_with("set-cookie:"))
        .expect("login must set a cookie");
    line["set-cookie:".len()..]
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

fn wait_exit(server: &mut Server, limit: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if let Some(st) = server.child.try_wait().unwrap() {
            return Some(st);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

fn read_stderr(server: &mut Server) -> String {
    let mut buf = String::new();
    if let Some(mut e) = server.child.stderr.take() {
        let _ = e.read_to_string(&mut buf);
    }
    buf
}


/// Spawns the binary with an invalid config; expects it to exit without listening.
fn expect_rejected(envs: &[(&str, &str)], must_name: &[&str]) {
    let port = free_port();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_disposable-server"));
    cmd.env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("RUST_LOG", "warn")
        .env("JWT_SECRET", "integration-test-jwt-secret")
        .env_remove("PDF_MD_ENGINE")
        .env_remove("PDF_MD_MAX_BYTES")
        .env_remove("PDF_MD_MAX_RESPONSE_BYTES")
        .env_remove("AWS_LAMBDA_FUNCTION_NAME")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let child = cmd.spawn().expect("spawn server binary");
    let mut server = Server { child, port };
    let st = wait_exit(&mut server, Duration::from_secs(15))
        .unwrap_or_else(|| panic!("server must exit for {envs:?}"));
    assert!(!st.success(), "exit must be non-zero for {envs:?}");
    assert_eq!(st.code(), Some(2), "{envs:?}");
    let err = read_stderr(&mut server);
    for n in must_name {
        assert!(err.contains(n), "stderr for {envs:?} must name {n}: {err}");
    }
    assert!(
        TcpStream::connect(("127.0.0.1", port)).is_err(),
        "nothing may listen on the port for {envs:?}"
    );
}

#[test]
fn unknown_engine_stops_startup() {
    expect_rejected(&[("PDF_MD_ENGINE", "nope")], &["PDF_MD_ENGINE", "poppler"]);
}

#[test]
fn invalid_max_bytes_stops_startup() {
    for bad in ["abc", "0", "-1", "1.5"] {
        expect_rejected(&[("PDF_MD_MAX_BYTES", bad)], &["PDF_MD_MAX_BYTES"]);
    }
}

#[test]
fn invalid_max_response_bytes_stops_startup() {
    expect_rejected(&[("PDF_MD_MAX_RESPONSE_BYTES", "abc")], &["PDF_MD_MAX_RESPONSE_BYTES"]);
}

#[test]
fn oversized_max_bytes_is_capped_at_25_mb() {
    let server = spawn_server(&[("PDF_MD_MAX_BYTES", "99999999999")]);
    let cookie = authenticate(server.port);
    let (status, _, body) = request(
        server.port,
        "GET",
        "/api/apps/pdf-to-markdown/limits",
        Some(&cookie),
        "",
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("26214400"), "limits body: {body}");
}

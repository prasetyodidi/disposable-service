//! Child-process integration tests for Burn Workspace shutdown behaviour.
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

fn spawn_server(block_shutdown: bool) -> Server {
    let port = free_port();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_disposable-server"));
    cmd.env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env("RUST_LOG", "warn")
        .env("JWT_SECRET", "integration-test-jwt-secret")
        .env_remove("DISPOSABLE_PASSWORD_HASH")
        .env_remove("DISPOSABLE_TEST_BLOCK_SHUTDOWN")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if block_shutdown {
        cmd.env("DISPOSABLE_TEST_BLOCK_SHUTDOWN", "1");
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

#[test]
fn burn_exits_zero_within_two_seconds() {
    let mut server = spawn_server(false);
    let cookie = authenticate(server.port);

    let start = Instant::now();
    let (status, _, body) = request(server.port, "POST", "/api/burn", Some(&cookie), "");
    assert_eq!(status, 200, "burn failed: {body}");

    let st = wait_exit(&mut server, Duration::from_secs(2).saturating_sub(start.elapsed()))
        .expect("server must exit within 2 seconds of the burn trigger");
    assert_eq!(st.code(), Some(0));
}

#[test]
fn burn_with_disconnected_client_exits_zero_within_five_seconds() {
    let mut server = spawn_server(false);
    let cookie = authenticate(server.port);

    let start = Instant::now();
    {
        // Send the burn request but never read the response.
        let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        let req = format!(
            "POST /api/burn HTTP/1.1\r\nHost: 127.0.0.1\r\nCookie: {cookie}\r\nContent-Length: 0\r\n\r\n"
        );
        s.write_all(req.as_bytes()).unwrap();
        s.flush().unwrap();

        // Hyper discards a request whose connection closes before dispatch, so
        // keep the socket open until the burn is observably triggered (the
        // gate answers 503 for app routes), then disconnect without reading.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let (status, _, _) = request(server.port, "POST", "/api/apps/redactor", None, "{}");
            // 503: gate is active. 0: server already gone (burn ran and exited).
            if status == 503 || status == 0 {
                break;
            }
            assert!(Instant::now() < deadline, "burn was never triggered");
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = s.shutdown(std::net::Shutdown::Both);
    }

    let st = wait_exit(&mut server, Duration::from_secs(5).saturating_sub(start.elapsed()))
        .expect("server must exit within 5 seconds when the client disconnects");
    assert_eq!(st.code(), Some(0));
}

#[test]
fn blocked_shutdown_is_forced_to_exit_one_with_stderr_message() {
    let mut server = spawn_server(true);
    let cookie = authenticate(server.port);

    let (status, _, body) = request(server.port, "POST", "/api/burn", Some(&cookie), "");
    assert_eq!(status, 200, "burn failed: {body}");

    // <= 2s body wait + 3s watchdog, with margin.
    let st = wait_exit(&mut server, Duration::from_secs(10))
        .expect("watchdog must terminate the process");
    assert_eq!(st.code(), Some(1));

    let stderr = read_stderr(&mut server);
    assert!(
        stderr.contains("burn: graceful shutdown did not complete in time"),
        "unexpected stderr: {stderr:?}"
    );
}

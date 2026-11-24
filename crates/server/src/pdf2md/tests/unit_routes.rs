//! Unit tests for the three PDF to Markdown routes (`MockEngine`, no poppler needed).
//!
//! Every test uses its own temporary `job_root`, never the shared workspace temp dir.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use disposable_common::CategoryStat;

use crate::pdf2md::error::EngineError;
use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::tests::helpers::{
    authed_pdf_app, capture_logs, pdf_bytes, test_runtime, tmp_listing_of,
};
use crate::pdf2md::tests::mock_engine::MockEngine;
use crate::privacy::burn::{StorageBackend, WipeOutcome};
use crate::state::AppState;
use crate::test_support::http::{get, send};

const URI: &str = "/api/apps/pdf-to-markdown";
const LIMITS_URI: &str = "/api/apps/pdf-to-markdown/limits";
const BOUNDARY: &str = "----unitroutesboundary";

fn doc_with_text(text: &str) -> IntermediateDoc {
    let span = Span { text: text.to_string(), bold: false, italic: false, mono: false, size: 12 };
    let line = TextLine::new(vec![span], 12, 80, 150, 400, 16);
    IntermediateDoc {
        total_pages: 1,
        first_page: 1,
        last_page: 1,
        pages: vec![Page { number: 1, width: 918, height: 1188, lines: vec![line] }],
    }
}

/// Multipart body from `(name, filename, data)` parts.
fn multipart_body(parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, filename, data) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match filename {
            Some(f) => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\n")
                    .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn request(method: Method, uri: &str, bearer: Option<&str>, body: Option<Vec<u8>>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    match body {
        Some(body) => b
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={BOUNDARY}"))
            .body(Body::from(body))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

fn post_file(uri: &str, bearer: Option<&str>, data: &[u8]) -> Request<Body> {
    request(
        Method::POST,
        uri,
        bearer,
        Some(multipart_body(&[("file", Some("a.pdf"), data)])),
    )
}

fn json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(body)))
}

fn assert_no_store(h: &HeaderMap) {
    assert_eq!(h.get(header::CACHE_CONTROL).map(|v| v.to_str().unwrap()), Some("no-store"));
}

/// Waits up to 1 second for the job root to be empty.
fn assert_job_root_clean(root: &Path) {
    let start = Instant::now();
    loop {
        let l = tmp_listing_of(root);
        if l.is_empty() {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(1), "job root not clean: {l:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct NoopBackend;

impl StorageBackend for NoopBackend {
    async fn wipe_downloads(&self) -> WipeOutcome {
        WipeOutcome::default()
    }
    async fn clear_mcp(&self) -> WipeOutcome {
        WipeOutcome::default()
    }
    async fn clear_sessions(&self) -> WipeOutcome {
        WipeOutcome::default()
    }
    async fn wipe_temp(&self) -> WipeOutcome {
        WipeOutcome::default()
    }
    async fn measure(&self) -> Vec<CategoryStat> {
        vec![]
    }
}

struct Fixture {
    app: Router,
    state: AppState,
    token: String,
    engine: Arc<MockEngine>,
    root: tempfile::TempDir,
}

async fn fixture_with(engine: Arc<MockEngine>, max_bytes: Option<u64>) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let mut rt = test_runtime(engine.clone(), root.path());
    if let Some(m) = max_bytes {
        rt.config.max_bytes = m;
    }
    let (app, state, token) = authed_pdf_app(rt).await;
    Fixture { app, state, token, engine, root }
}

async fn fixture() -> Fixture {
    fixture_with(MockEngine::doc(doc_with_text("Halo dunia")), None).await
}

async fn start_burn(state: &AppState) {
    state.burn.run_with(&NoopBackend, None, None).await;
    assert!(state.burn.is_burning());
}

async fn seed_download(state: &AppState) -> String {
    state
        .store_ephemeral_download(b"x".to_vec(), "document.md".into(), "text/markdown; charset=utf-8".into(), 300)
        .await
}

// ---------- 401 / 503 per route ----------

#[tokio::test]
async fn unauthenticated_requests_get_401_without_side_effects() {
    let f = fixture().await;
    let tok = seed_download(&f.state).await;
    let dl = format!("{URI}/download/{tok}");

    let (s, h, b) = send(&f.app, post_file(URI, None, &pdf_bytes(64))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_no_store(&h);
    assert_eq!(json(&b)["code"], "unauthorized");

    let (s, h, _) = send(&f.app, get(LIMITS_URI, None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_no_store(&h);

    let (s, h, _) = send(&f.app, get(&dl, None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_no_store(&h);

    assert_eq!(f.engine.calls(), 0);
    assert_job_root_clean(f.root.path());
    assert!(f.state.downloads.read().await.contains_key(&tok), "download consumed on 401");
}

#[tokio::test]
async fn burning_requests_get_503_without_side_effects() {
    let f = fixture().await;
    let tok = seed_download(&f.state).await;
    let dl = format!("{URI}/download/{tok}");
    start_burn(&f.state).await;

    for req in [
        post_file(URI, Some(&f.token), &pdf_bytes(64)),
        get(LIMITS_URI, Some(&f.token)),
        get(&dl, Some(&f.token)),
    ] {
        let (s, h, _) = send(&f.app, req).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_no_store(&h);
    }

    assert_eq!(f.engine.calls(), 0);
    assert_job_root_clean(f.root.path());
    assert!(f.state.downloads.read().await.contains_key(&tok), "download consumed during burn");
}

// ---------- limits ----------

#[tokio::test]
async fn limits_returns_max_bytes_and_max_pages() {
    let f = fixture_with(MockEngine::doc(doc_with_text("x")), Some(64)).await;
    let (s, h, b) = send(&f.app, get(LIMITS_URI, Some(&f.token))).await;
    assert_eq!(s, StatusCode::OK);
    assert_no_store(&h);
    let v = json(&b);
    assert_eq!(v["max_bytes"], 64);
    assert_eq!(v["max_pages"], 300);
}

// ---------- exact size limits ----------

#[tokio::test]
async fn file_limit_is_exact_at_max_bytes() {
    let f = fixture_with(MockEngine::doc(doc_with_text("Halo")), Some(64)).await;

    let (s, h, _) = send(&f.app, post_file(URI, Some(&f.token), &pdf_bytes(64))).await;
    assert_eq!(s, StatusCode::OK);
    assert_no_store(&h);
    assert_eq!(f.engine.calls(), 1);

    let (s, h, b) = send(&f.app, post_file(URI, Some(&f.token), &pdf_bytes(65))).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert_no_store(&h);
    assert_eq!(json(&b)["code"], "file_too_large");
    assert_eq!(f.engine.calls(), 1, "engine must not run for 413");
    assert_job_root_clean(f.root.path());
}

/// Body of exactly `total` bytes: a 64-byte PDF plus a padding part named `pad`.
fn padded_body(total: usize) -> Vec<u8> {
    let pdf = pdf_bytes(64);
    let base = multipart_body(&[("file", Some("a.pdf"), &pdf), ("pad", None, b"")]).len();
    let pad = vec![b'p'; total - base];
    let body = multipart_body(&[("file", Some("a.pdf"), &pdf), ("pad", None, &pad)]);
    assert_eq!(body.len(), total);
    body
}

#[tokio::test]
async fn body_limit_is_exact_at_max_bytes_plus_overhead() {
    let f = fixture_with(MockEngine::doc(doc_with_text("Halo")), Some(64)).await;
    let limit = 64 + 65_536;

    let req = request(Method::POST, URI, Some(&f.token), Some(padded_body(limit)));
    let (s, _, b) = send(&f.app, req).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&b));

    let req = request(Method::POST, URI, Some(&f.token), Some(padded_body(limit + 1)));
    let (s, h, b) = send(&f.app, req).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert_no_store(&h);
    assert_eq!(json(&b)["code"], "file_too_large");
    assert_job_root_clean(f.root.path());
}

// ---------- error paths leave no temp files ----------

#[tokio::test]
async fn error_paths_leave_job_root_clean() {
    let cases: Vec<(EngineError, StatusCode, &str)> = vec![
        (EngineError::NoTextLayer, StatusCode::UNPROCESSABLE_ENTITY, "no_text_layer"),
        (EngineError::Corrupt, StatusCode::UNPROCESSABLE_ENTITY, "corrupt_pdf"),
        (EngineError::Timeout, StatusCode::GATEWAY_TIMEOUT, "timeout"),
        (EngineError::Unavailable, StatusCode::SERVICE_UNAVAILABLE, "engine_unavailable"),
    ];
    for (err, status, code) in cases {
        let f = fixture_with(MockEngine::err(err), None).await;
        let (s, h, b) = send(&f.app, post_file(URI, Some(&f.token), &pdf_bytes(200))).await;
        assert_eq!(s, status, "{code}");
        assert_no_store(&h);
        assert_eq!(json(&b)["code"], code);
        assert_job_root_clean(f.root.path());
    }

    let f = fixture().await;
    // 415
    let (s, _, b) = send(&f.app, post_file(URI, Some(&f.token), b"not a pdf")).await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(json(&b)["code"], "not_a_pdf");
    // 400 invalid options
    let body = multipart_body(&[
        ("file", Some("a.pdf"), &pdf_bytes(100)),
        ("options", None, br#"{"bogus":1}"#),
    ]);
    let (s, _, b) = send(&f.app, request(Method::POST, URI, Some(&f.token), Some(body))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(json(&b)["code"], "invalid_options");
    // 400 missing file
    let body = multipart_body(&[("options", None, b"{}")]);
    let (s, _, _) = send(&f.app, request(Method::POST, URI, Some(&f.token), Some(body))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // 401
    let (s, _, _) = send(&f.app, post_file(URI, None, &pdf_bytes(100))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(f.engine.calls(), 0);
    assert_job_root_clean(f.root.path());
}

#[tokio::test]
async fn successful_conversion_leaves_job_root_clean() {
    let f = fixture().await;
    let (s, _, _) = send(&f.app, post_file(URI, Some(&f.token), &pdf_bytes(200))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(f.engine.calls(), 1);
    assert_job_root_clean(f.root.path());
}

// ---------- logging ----------

#[tokio::test]
async fn logs_contain_no_markers_on_success_and_failure() {
    let cap = capture_logs();
    let text = "zqtextmarker1234567";
    let fname = "zqfilemarker1234567.pdf";
    let content = "zqcontentmarker1234567";

    let mut pdf = pdf_bytes(64);
    pdf.extend_from_slice(content.as_bytes());
    let send_it = |app: &Router, token: &str, data: &[u8]| {
        let req = request(
            Method::POST,
            URI,
            Some(token),
            Some(multipart_body(&[("file", Some(fname), data)])),
        );
        let app = app.clone();
        async move { send(&app, req).await.0 }
    };

    let ok = fixture_with(MockEngine::doc(doc_with_text(text)), None).await;
    assert_eq!(send_it(&ok.app, &ok.token, &pdf).await, StatusCode::OK);
    let bad = fixture_with(MockEngine::err(EngineError::Corrupt), None).await;
    assert_eq!(send_it(&bad.app, &bad.token, &pdf).await, StatusCode::UNPROCESSABLE_ENTITY);

    let logs = cap.contents();
    for m in [text, fname, "zqfilemarker1234567", content] {
        assert!(!logs.contains(m), "log leaked {m}");
    }
}

// ---------- methods ----------

#[tokio::test]
async fn other_methods_get_405() {
    let f = fixture().await;
    let tok = seed_download(&f.state).await;
    let dl = format!("{URI}/download/{tok}");
    let cases = [
        (Method::GET, URI.to_string()),
        (Method::PUT, URI.to_string()),
        (Method::DELETE, URI.to_string()),
        (Method::POST, LIMITS_URI.to_string()),
        (Method::PUT, LIMITS_URI.to_string()),
        (Method::POST, dl.clone()),
        (Method::DELETE, dl.clone()),
    ];
    for (m, uri) in cases {
        let (s, _, _) = send(&f.app, request(m.clone(), &uri, Some(&f.token), None)).await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED, "{m} {uri}");
    }
    assert_eq!(f.engine.calls(), 0);
    assert!(f.state.downloads.read().await.contains_key(&tok));
}

// ---------- multipart parts ----------

#[tokio::test]
async fn missing_file_part_is_400_file_required() {
    let f = fixture().await;
    let body = multipart_body(&[("other", None, b"zzz")]);
    let (s, h, b) = send(&f.app, request(Method::POST, URI, Some(&f.token), Some(body))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_no_store(&h);
    assert_eq!(json(&b)["code"], "file_required");
    assert_eq!(f.engine.calls(), 0);
}

#[tokio::test]
async fn multiple_file_parts_yield_single_warning() {
    let f = fixture().await;
    let pdf = pdf_bytes(100);
    let body = multipart_body(&[
        ("file", Some("a.pdf"), &pdf),
        ("file", Some("b.pdf"), &pdf),
        ("file", Some("c.pdf"), &pdf),
    ]);
    let (s, _, b) = send(&f.app, request(Method::POST, URI, Some(&f.token), Some(body))).await;
    assert_eq!(s, StatusCode::OK);
    let warnings = json(&b)["warnings"].as_array().unwrap().clone();
    let extra: Vec<_> = warnings
        .iter()
        .filter(|w| w.as_str().unwrap().contains("File tambahan"))
        .collect();
    assert_eq!(extra.len(), 1, "{warnings:?}");
    assert_eq!(f.engine.calls(), 1);
}

// ---------- download query ----------

#[tokio::test]
async fn download_query_only_true_creates_token() {
    let f = fixture().await;
    for (q, expect_token) in [("true", true), ("false", false), ("1", false), ("TRUE", false)] {
        let uri = format!("{URI}?download={q}");
        let before = f.state.downloads.read().await.len();
        let (s, h, b) = send(&f.app, post_file(&uri, Some(&f.token), &pdf_bytes(100))).await;
        assert_eq!(s, StatusCode::OK, "{q}");
        assert_no_store(&h);
        let v = json(&b);
        let token = v.get("download_token").and_then(|t| t.as_str());
        let after = f.state.downloads.read().await.len();
        assert_eq!(token.is_some(), expect_token, "download={q}");
        assert_eq!(after - before, usize::from(expect_token), "download={q}");
        if let Some(t) = token {
            let (s, h, body) = send(&f.app, get(&format!("{URI}/download/{t}"), Some(&f.token))).await;
            assert_eq!(s, StatusCode::OK);
            assert_no_store(&h);
            assert!(h[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/markdown"));
            assert!(String::from_utf8(body).unwrap().contains("Halo dunia"));
        }
    }
}

#[tokio::test]
async fn unknown_download_token_is_404_no_store() {
    let f = fixture().await;
    for t in [uuid::Uuid::new_v4().to_string(), "not-a-uuid".to_string()] {
        let (s, h, _) = send(&f.app, get(&format!("{URI}/download/{t}"), Some(&f.token))).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_no_store(&h);
    }
}

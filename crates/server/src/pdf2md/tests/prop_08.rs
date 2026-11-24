//! Property 8: Tidak ada data pengguna di log.
//!
//! `tracing` output is captured (thread-scoped, TRACE level) while the real router converts
//! documents with a `MockEngine`, on success and on every engine failure, plus option and
//! file validation failures. Unique markers in the upload file name, the document text, the
//! file content and the option names/values must never appear in the captured output.

// Feature: pdf-to-markdown, Property 8: Tidak ada data pengguna di log
// **Validates: Requirements 8.8, 12.4, 16.8**

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use proptest::prelude::*;

use crate::pdf2md::error::EngineError;
use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::tests::helpers::{authed_pdf_app, capture_logs, pdf_bytes, test_runtime};
use crate::pdf2md::tests::mock_engine::MockEngine;
use crate::test_support::http::send;

const URI: &str = "/api/apps/pdf-to-markdown";

fn multipart(token: &str, uri: &str, filename: &str, data: &[u8], options: Option<&str>) -> Request<Body> {
    let boundary = "----prop08boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/pdf\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
    if let Some(opts) = options {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"options\"\r\n\r\n{opts}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap()
}

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

fn arb_marker(tag: &'static str) -> impl Strategy<Value = String> {
    "[a-z0-9]{10,16}".prop_map(move |s| format!("zq{tag}{s}"))
}

#[derive(Debug, Clone)]
struct Markers {
    filename: String,
    text: String,
    content: String,
    option: String,
}

fn arb_markers() -> impl Strategy<Value = Markers> {
    (arb_marker("fn"), arb_marker("tx"), arb_marker("ct"), arb_marker("op"))
        .prop_map(|(filename, text, content, option)| Markers { filename, text, content, option })
}

fn engine_errors() -> Vec<EngineError> {
    vec![
        EngineError::Corrupt,
        EngineError::Encrypted,
        EngineError::NoTextLayer,
        EngineError::Timeout,
        EngineError::TooManyPages,
        EngineError::Unavailable,
        EngineError::OutputTooLarge,
        EngineError::PageFromOutOfRange { total_pages: 1 },
    ]
}

/// Runs all scenarios for one marker set; returns (leaks, per-scenario statuses).
fn run_case(m: &Markers) -> (Vec<String>, Vec<StatusCode>) {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let cap = capture_logs();
    let job_root = tempfile::tempdir().unwrap();
    let fname = format!("{}.pdf", m.filename);

    let statuses = rt.block_on(async {
        let mut st = Vec::new();
        let pdf = {
            let mut p = pdf_bytes(64);
            p.extend_from_slice(m.content.as_bytes());
            p
        };

        // Success, with and without download, with valid options.
        let ok = MockEngine::doc(doc_with_text(&m.text));
        let (app, _state, token) = authed_pdf_app(test_runtime(ok, job_root.path())).await;
        let opts = r#"{"page_breaks":true,"strip_headers_footers":false}"#;
        st.push(send(&app, multipart(&token, URI, &fname, &pdf, Some(opts))).await.0);
        let uri_dl = format!("{URI}?download=true");
        st.push(send(&app, multipart(&token, &uri_dl, &fname, &pdf, None)).await.0);

        // Validation failures: marker as option key, as option value, and as non-PDF content.
        let bad_key = format!("{{\"{}\":1}}", m.option);
        st.push(send(&app, multipart(&token, URI, &fname, &pdf, Some(&bad_key))).await.0);
        let bad_val = format!("{{\"page_from\":\"{}\"}}", m.option);
        st.push(send(&app, multipart(&token, URI, &fname, &pdf, Some(&bad_val))).await.0);
        let bad_json = format!("{{\"page_breaks\":{}", m.option);
        st.push(send(&app, multipart(&token, URI, &fname, &pdf, Some(&bad_json))).await.0);
        st.push(send(&app, multipart(&token, URI, &fname, m.content.as_bytes(), None)).await.0);
        // Unauthenticated request carrying markers.
        st.push(send(&app, multipart("bad-token", URI, &fname, &pdf, Some(&bad_key))).await.0);

        // Every engine failure.
        for e in engine_errors() {
            let eng = MockEngine::err(e);
            let (app, _s, token) = authed_pdf_app(test_runtime(eng, job_root.path())).await;
            st.push(send(&app, multipart(&token, URI, &fname, &pdf, Some(opts))).await.0);
        }
        st
    });

    let captured = cap.contents();
    let mut leaks = Vec::new();
    for (name, needle) in [
        ("filename", &m.filename),
        ("text", &m.text),
        ("content", &m.content),
        ("option", &m.option),
    ] {
        if captured.contains(needle.as_str()) {
            leaks.push(name.to_string());
        }
    }
    (leaks, statuses)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

    #[test]
    fn prop_no_user_data_in_logs(m in arb_markers()) {
        let (leaks, statuses) = run_case(&m);
        prop_assert!(leaks.is_empty(), "leaked into logs: {:?}", leaks);
        // Success and failure paths were both exercised.
        prop_assert_eq!(statuses[0], StatusCode::OK);
        prop_assert_eq!(statuses[1], StatusCode::OK);
        prop_assert_eq!(statuses[2], StatusCode::BAD_REQUEST);
        prop_assert_eq!(statuses[3], StatusCode::BAD_REQUEST);
        prop_assert_eq!(statuses[4], StatusCode::BAD_REQUEST);
        prop_assert_eq!(statuses[5], StatusCode::UNSUPPORTED_MEDIA_TYPE);
        prop_assert_eq!(statuses[6], StatusCode::UNAUTHORIZED);
        prop_assert!(statuses[7..].iter().all(|s| s.is_client_error() || s.is_server_error()));
        prop_assert_eq!(statuses.len(), 7 + engine_errors().len());
    }
}

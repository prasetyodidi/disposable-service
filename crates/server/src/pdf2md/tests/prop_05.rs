//! Feature: pdf-to-markdown, Property 5: Urutan prioritas galat
//!
//! **Validates: Requirements 2.1, 2.2, 2.12, 9.6**
//!
//! For any combination of simultaneous error conditions the response status is that of
//! the first applicable condition in the order 413, 415, 400, 422, 504; and 413 occurs
//! iff the file size is greater than `max_bytes`.
use super::helpers::{authed_pdf_app, pdf_bytes, test_runtime};
use super::mock_engine::{MockEngine, MockOutcome};
use crate::pdf2md::error::EngineError;
use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::test_support::http::send;
use axum::body::Body;
use axum::http::{header, Method, Request};
use proptest::prelude::*;

const MAX_BYTES: u64 = 64;
const URI: &str = "/api/apps/pdf-to-markdown";

#[derive(Debug, Clone, Copy)]
enum EngineKind {
    Ok,
    Unprocessable(u8),
    Timeout,
}

fn text_doc() -> IntermediateDoc {
    let span = Span { text: "halo dunia".into(), bold: false, italic: false, mono: false, size: 12 };
    let line = TextLine::new(vec![span], 12, 50, 100, 200, 16);
    IntermediateDoc {
        total_pages: 1,
        first_page: 1,
        last_page: 1,
        pages: vec![Page { number: 1, width: 600, height: 800, lines: vec![line] }],
    }
}

fn outcome(kind: EngineKind) -> MockOutcome {
    match kind {
        EngineKind::Ok => MockOutcome::Doc(text_doc()),
        EngineKind::Timeout => MockOutcome::Err(EngineError::Timeout),
        EngineKind::Unprocessable(0) => MockOutcome::Err(EngineError::Corrupt),
        EngineKind::Unprocessable(1) => MockOutcome::Err(EngineError::Encrypted),
        EngineKind::Unprocessable(2) => MockOutcome::Err(EngineError::TooManyPages),
        EngineKind::Unprocessable(_) => MockOutcome::Err(EngineError::OutputTooLarge),
    }
}

fn multipart(file: &[u8], options: Option<&str>) -> Request<Body> {
    let b = "----prop05boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.pdf\"\r\nContent-Type: application/pdf\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(file);
    body.extend_from_slice(b"\r\n");
    if let Some(o) = options {
        body.extend_from_slice(
            format!("--{b}\r\nContent-Disposition: form-data; name=\"options\"\r\n\r\n{o}\r\n").as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{b}--\r\n").as_bytes());
    Request::builder()
        .method(Method::POST)
        .uri(URI)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={b}"))
        .body(Body::from(body))
        .unwrap()
}

/// Status expected from the priority order 413, 415, 400, 422, 504.
fn expected_status(size: u64, is_pdf: bool, bad_options: bool, kind: EngineKind) -> u16 {
    if size > MAX_BYTES {
        413
    } else if !is_pdf {
        415
    } else if bad_options {
        400
    } else {
        match kind {
            EngineKind::Unprocessable(_) => 422,
            EngineKind::Timeout => 504,
            EngineKind::Ok => 200,
        }
    }
}

fn arb_engine() -> impl Strategy<Value = EngineKind> {
    prop_oneof![
        Just(EngineKind::Ok),
        Just(EngineKind::Timeout),
        (0u8..4).prop_map(EngineKind::Unprocessable),
    ]
}

/// Sizes cluster around `MAX_BYTES` so the exact boundary is exercised often.
fn arb_size() -> impl Strategy<Value = usize> {
    prop_oneof![
        3 => (MAX_BYTES as usize - 2)..=(MAX_BYTES as usize + 2),
        2 => 5usize..=MAX_BYTES as usize,
        2 => (MAX_BYTES as usize + 1)..=400usize,
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(150))]

    #[test]
    fn prop_05_error_priority_order(
        size in arb_size(),
        is_pdf in any::<bool>(),
        bad_options in any::<bool>(),
        kind in arb_engine(),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let root = tempfile::tempdir().unwrap();
        let (status, calls) = rt.block_on(async {
            let engine = MockEngine::arc(outcome(kind));
            let mut runtime = test_runtime(engine.clone(), root.path());
            runtime.config.max_bytes = MAX_BYTES;
            let (app, _state, token) = authed_pdf_app(runtime).await;
            let mut file = pdf_bytes(size);
            if !is_pdf {
                file[0] = b'X';
            }
            let options = if bad_options { Some(r#"{"unknown_field":1}"#) } else { None };
            let mut req = multipart(&file, options);
            req.headers_mut().insert(
                header::AUTHORIZATION,
                format!("Bearer {token}").parse().unwrap(),
            );
            let (status, _, _) = send(&app, req).await;
            (status.as_u16(), engine.calls())
        });
        // An empty/short prefix cannot be a PDF; `pdf_bytes` for size >= 5 starts with %PDF-.
        let want = expected_status(size as u64, is_pdf, bad_options, kind);
        prop_assert_eq!(status, want);
        // Rejections decided before extraction never reach the engine.
        if want == 413 || want == 415 || want == 400 {
            prop_assert_eq!(calls, 0);
        }
        // 413 iff size > max_bytes.
        prop_assert_eq!(status == 413, size as u64 > MAX_BYTES);
    }
}

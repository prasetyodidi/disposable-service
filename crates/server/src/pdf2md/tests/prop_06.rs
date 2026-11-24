// Feature: pdf-to-markdown, Property 6: Bentuk badan galat
//
// For every `ErrorCode`, a request that provokes it (with a unique marker in the upload file
// name and content) answers with the status of the mapping table and a JSON body holding a
// string `code` equal to `code_str()` plus a non-empty string `message`. The body contains
// neither the marker, the file name, nor the job directory path.
//
// **Validates: Requirements 2.11, 7.7**
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use std::sync::Arc;

use super::helpers::{authed_pdf_app, pdf_bytes, test_runtime};
use super::mock_engine::MockEngine;
use crate::pdf2md::engine::ExtractionEngine;
use crate::pdf2md::error::{EngineError, ErrorCode};
use crate::privacy::burn::{StorageBackend, WipeOutcome};
use crate::test_support::http::{post_multipart_file, send};
use disposable_common::CategoryStat;

const URI: &str = "/api/apps/pdf-to-markdown";
const BOUNDARY: &str = "----p6boundary";

const ALL: [ErrorCode; 16] = [
    ErrorCode::Unauthorized,
    ErrorCode::BurnInProgress,
    ErrorCode::FileTooLarge,
    ErrorCode::NotAPdf,
    ErrorCode::BadRequest,
    ErrorCode::FileRequired,
    ErrorCode::BadOptions,
    ErrorCode::PageFromOutOfRange,
    ErrorCode::CorruptPdf,
    ErrorCode::EncryptedPdf,
    ErrorCode::NoTextLayer,
    ErrorCode::TooManyPages,
    ErrorCode::OutputTooLarge,
    ErrorCode::Timeout,
    ErrorCode::EngineUnavailable,
    ErrorCode::Internal,
];

/// Backend that wipes nothing; only flips the burning flag.
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

fn engine_err(e: EngineError) -> Arc<dyn ExtractionEngine> {
    MockEngine::err(e)
}

/// Multipart body from raw parts: (name, optional filename, data). `close` adds the end boundary.
fn multipart(bearer: &str, parts: &[(&str, Option<&str>, &[u8])], close: bool) -> Request<Body> {
    let mut body = Vec::new();
    for (name, filename, data) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match filename {
            Some(f) => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\n\r\n")
                    .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    if close {
        body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    }
    Request::builder()
        .method(Method::POST)
        .uri(URI)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={BOUNDARY}"))
        .body(Body::from(body))
        .unwrap()
}

/// Runs one request that provokes `code`; returns (status, body bytes, job root path text).
async fn provoke(code: ErrorCode, marker: &str, filename: &str) -> (StatusCode, Vec<u8>, String) {
    let root = tempfile::tempdir().unwrap();
    let mut job_root = root.path().to_path_buf();
    let mut engine: Arc<dyn ExtractionEngine> = engine_err(EngineError::Corrupt);
    let mut max_bytes = None;
    match code {
        ErrorCode::PageFromOutOfRange => {
            engine = engine_err(EngineError::PageFromOutOfRange { total_pages: 3 })
        }
        ErrorCode::CorruptPdf => engine = engine_err(EngineError::Corrupt),
        ErrorCode::EncryptedPdf => engine = engine_err(EngineError::Encrypted),
        ErrorCode::NoTextLayer => engine = engine_err(EngineError::NoTextLayer),
        ErrorCode::TooManyPages => engine = engine_err(EngineError::TooManyPages),
        ErrorCode::OutputTooLarge => engine = engine_err(EngineError::OutputTooLarge),
        ErrorCode::Timeout => engine = engine_err(EngineError::Timeout),
        ErrorCode::EngineUnavailable => engine = engine_err(EngineError::Unavailable),
        ErrorCode::FileTooLarge => max_bytes = Some(64u64),
        ErrorCode::Internal => {
            // A regular file as parent makes job directory creation fail.
            let blocker = root.path().join("blocker");
            std::fs::write(&blocker, b"x").unwrap();
            job_root = blocker.join("jobs");
        }
        _ => {}
    }
    let root_text = root.path().to_string_lossy().into_owned();

    let mut rt = test_runtime(engine, &job_root);
    if let Some(m) = max_bytes {
        rt.config.max_bytes = m;
    }
    let (app, state, token) = authed_pdf_app(rt).await;

    let mut content = pdf_bytes(80);
    content.extend_from_slice(marker.as_bytes());
    let req = match code {
        ErrorCode::Unauthorized => {
            post_multipart_file(URI, "not-a-session", filename, &content)
        }
        ErrorCode::BurnInProgress => {
            state.burn.run_with(&NoopBackend, None, None).await;
            assert!(state.burn.is_burning());
            post_multipart_file(URI, &token, filename, &content)
        }
        ErrorCode::NotAPdf => {
            post_multipart_file(URI, &token, filename, format!("hello {marker}").as_bytes())
        }
        ErrorCode::BadRequest => {
            // Body ends without the closing boundary, so reading the field fails.
            multipart(&token, &[("file", Some(filename), content.as_slice())], false)
        }
        ErrorCode::FileRequired => {
            multipart(&token, &[("options", None, marker.as_bytes())], true)
        }
        ErrorCode::BadOptions => multipart(
            &token,
            &[("file", Some(filename), content.as_slice()), ("options", None, marker.as_bytes())],
            true,
        ),
        _ => post_multipart_file(URI, &token, filename, &content),
    };
    let (status, _, body) = send(&app, req).await;
    drop(root);
    (status, body, root_text)
}

fn arb_marker() -> impl Strategy<Value = String> {
    "[a-z0-9]{12}".prop_map(|s| format!("MARK{s}"))
}

fn arb_filename() -> impl Strategy<Value = String> {
    "[a-z0-9]{1,16}".prop_map(|s| format!("{s}.pdf"))
}

#[test]
fn error_bodies_have_stable_shape_and_no_leaks() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut runner = TestRunner::new(Config { cases: 128, ..Config::default() });
    let strat = (0usize..ALL.len(), arb_marker(), arb_filename());
    runner
        .run(&strat, |(idx, marker, stem)| {
            let code = ALL[idx];
            let filename = format!("{marker}-{stem}");
            let (status, body, root_text) =
                rt.block_on(provoke(code, &marker, &filename));

            prop_assert_eq!(status, code.http_status(), "status for {:?}", code);
            let v: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| TestCaseError::fail(format!("{code:?}: body is not JSON: {e}")))?;
            prop_assert_eq!(v.get("code").and_then(|c| c.as_str()), Some(code.code_str()));
            let msg = v.get("message").and_then(|m| m.as_str());
            prop_assert!(msg.map(|m| !m.is_empty()).unwrap_or(false), "message for {:?}", code);

            let text = String::from_utf8_lossy(&body);
            prop_assert!(!text.contains(&marker), "marker leaked for {:?}", code);
            prop_assert!(!text.contains(&filename), "file name leaked for {:?}", code);
            prop_assert!(!text.contains(&root_text), "path leaked for {:?}", code);
            Ok(())
        })
        .unwrap();
}

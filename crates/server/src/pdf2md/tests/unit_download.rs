//! Unit tests for the PDF to Markdown Ephemeral_Download (Requirements 10, 16.10).
//! Uses a mock engine and an isolated `job_root`, so no poppler is needed.
use super::helpers::{authed_pdf_app, pdf_bytes, test_runtime};
use super::mock_engine::MockEngine;
use crate::pdf2md::error::EngineError;
use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::privacy::burn::RealBackend;
use crate::state::AppState;
use crate::test_support::http::{get, post_multipart_file, send};
use axum::http::StatusCode;
use axum::Router;
use chrono::{Duration, Utc};

const CONVERT: &str = "/api/apps/pdf-to-markdown?download=true";
const DL: &str = "/api/apps/pdf-to-markdown/download";

fn one_line_doc() -> IntermediateDoc {
    let span = Span { text: "Halo dunia".into(), bold: false, italic: false, mono: false, size: 12 };
    let line = TextLine::new(vec![span], 12, 10, 10, 100, 12);
    IntermediateDoc {
        total_pages: 1,
        first_page: 1,
        last_page: 1,
        pages: vec![Page { number: 1, width: 600, height: 800, lines: vec![line] }],
    }
}

async fn app_with(engine: std::sync::Arc<MockEngine>, root: &std::path::Path) -> (Router, AppState, String) {
    authed_pdf_app(test_runtime(engine, root)).await
}

/// Converts with `download=true` and returns the download token.
async fn convert_token(app: &Router, token: &str) -> String {
    let req = post_multipart_file(CONVERT, token, "a.pdf", &pdf_bytes(200));
    let (status, _, body) = send(app, req).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    v["download_token"].as_str().expect("download_token").to_string()
}

#[tokio::test]
async fn single_use_second_download_is_404() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::doc(one_line_doc()), root.path()).await;
    let dl = convert_token(&app, &bearer).await;
    let uri = format!("{DL}/{dl}");

    let (s1, h1, body) = send(&app, get(&uri, Some(&bearer))).await;
    assert_eq!(s1, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("Halo dunia"));
    assert!(h1["content-type"].to_str().unwrap().starts_with("text/markdown"));

    let (s2, _, _) = send(&app, get(&uri, Some(&bearer))).await;
    assert_eq!(s2, StatusCode::NOT_FOUND);
    assert!(state.downloads.read().await.is_empty());
}

#[tokio::test]
async fn expired_download_is_404_and_removed() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::doc(one_line_doc()), root.path()).await;
    let dl = convert_token(&app, &bearer).await;
    state.downloads.write().await.get_mut(&dl).unwrap().expires_at = Utc::now() - Duration::seconds(1);

    let (s, _, _) = send(&app, get(&format!("{DL}/{dl}"), Some(&bearer))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(state.downloads.read().await.is_empty());
}

#[tokio::test]
async fn purge_expired_removes_entries_but_keeps_live_ones() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::doc(one_line_doc()), root.path()).await;
    let old = convert_token(&app, &bearer).await;
    let live = convert_token(&app, &bearer).await;
    state.downloads.write().await.get_mut(&old).unwrap().expires_at = Utc::now() - Duration::seconds(1);

    assert_eq!(state.purge_expired_downloads().await, 1);
    let lock = state.downloads.read().await;
    assert!(!lock.contains_key(&old));
    assert!(lock.contains_key(&live));
}

#[tokio::test]
async fn burn_wipes_downloads_and_token_then_503() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::doc(one_line_doc()), root.path()).await;
    let dl = convert_token(&app, &bearer).await;
    assert_eq!(state.downloads.read().await.len(), 1);

    // `run_with` never schedules a process shutdown.
    state.burn.run_with(&RealBackend::new(&state), None, None).await;
    assert!(state.downloads.read().await.is_empty());

    // Burn also cleared sessions; the gate answers 503 before auth is checked.
    let (s, _, _) = send(&app, get(&format!("{DL}/{dl}"), Some(&bearer))).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn unauthenticated_download_is_401_and_keeps_entry() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::doc(one_line_doc()), root.path()).await;
    let dl = convert_token(&app, &bearer).await;
    let uri = format!("{DL}/{dl}");

    let (s, _, _) = send(&app, get(&uri, None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(state.downloads.read().await.contains_key(&dl));

    let (s, _, _) = send(&app, get(&uri, Some(&bearer))).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn download_during_burn_is_503_and_keeps_entry() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::doc(one_line_doc()), root.path()).await;
    state.burn.run_with(&RealBackend::new(&state), None, None).await;
    let token = state
        .store_ephemeral_download(b"x".to_vec(), "document.md".into(), "text/markdown".into(), 60)
        .await;

    let (s, _, _) = send(&app, get(&format!("{DL}/{token}"), Some(&bearer))).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(state.downloads.read().await.contains_key(&token));
}

#[tokio::test]
async fn other_feature_token_is_404_and_not_removed() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::doc(one_line_doc()), root.path()).await;
    let other = state
        .store_ephemeral_download(vec![1, 2, 3], "clean.pdf".into(), "application/pdf".into(), 60)
        .await;

    let (s, _, _) = send(&app, get(&format!("{DL}/{other}"), Some(&bearer))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(state.downloads.read().await.contains_key(&other));
}

#[tokio::test]
async fn failed_conversion_creates_no_download() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, bearer) = app_with(MockEngine::err(EngineError::Corrupt), root.path()).await;
    let req = post_multipart_file(CONVERT, &bearer, "a.pdf", &pdf_bytes(200));
    let (s, _, _) = send(&app, req).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(state.downloads.read().await.is_empty());
}

//! Minimal in-process HTTP helpers for tests: build the real API router, mint an
//! authenticated Bearer token, and send one-shot requests.
#![allow(dead_code)]
use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

use crate::auth::create_jwt;
use crate::routes::create_api_router;
use crate::state::AppState;
use disposable_common::SessionInfo;

/// Fresh state + router + a valid Bearer token backed by a registered session.
pub(crate) async fn authed_app() -> (Router, AppState, String) {
    let state = AppState::new();
    let session_id = uuid::Uuid::new_v4().to_string();
    let (token, exp) = create_jwt(&session_id, &state.jwt_secret).expect("jwt");
    state
        .add_session(SessionInfo {
            session_id,
            ip_address: "127.0.0.1".into(),
            user_agent: "test".into(),
            login_time: chrono::Utc::now().timestamp(),
            expires_at: exp as i64,
            token_type: None,
            label: None,
        })
        .await;
    let app = create_api_router(state.clone()).with_state(state.clone());
    (app, state, token)
}

/// Send one request and collect `(status, headers, body bytes)`.
pub(crate) async fn send(app: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let resp = app.clone().oneshot(req).await.expect("infallible");
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec();
    (status, headers, body)
}

pub(crate) fn get(uri: &str, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(Method::GET).uri(uri);
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

pub(crate) fn post_json(uri: &str, bearer: &str, json: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(json).unwrap()))
        .unwrap()
}

/// POST a single-file multipart form (field name `file`).
pub(crate) fn post_multipart_file(
    uri: &str,
    bearer: &str,
    filename: &str,
    data: &[u8],
) -> Request<Body> {
    let boundary = "----testboundary7f3a";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

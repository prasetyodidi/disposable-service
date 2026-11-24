//! Unit tests for the `/api/burn` HTTP endpoints.
//!
//! The process-exit watchdog is `cfg(not(test))`; `schedule_shutdown` only
//! stores a `Notify` permit here, which no other test waits on.
//!
//! **Validates: Requirements 9.5, 14.2**
use super::helpers_http::{authed_app, get, send};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use disposable_common::AuditReport;

fn post_burn(bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method(Method::POST).uri("/api/burn");
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn burn_without_auth_is_401_and_does_not_burn_req_9_5() {
    let (app, state, _) = authed_app().await;
    let (s, _, _) = send(&app, post_burn(None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = send(&app, post_burn(Some("garbage"))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = send(&app, get("/api/burn/events", None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!state.burn.is_burning());
    assert_eq!(state.sessions.read().await.len(), 1);
}

#[tokio::test]
async fn burn_returns_report_and_removes_session_cookie_req_9_5() {
    let (app, state, t) = authed_app().await;
    state
        .store_ephemeral_download(b"x".to_vec(), "f.bin".into(), "application/octet-stream".into(), 60)
        .await;

    let (s, h, body) = send(&app, post_burn(Some(&t))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "application/json");

    let cookie = h
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("disposable_session="))
        .expect("session cookie removal header");
    assert!(cookie.starts_with("disposable_session=;"), "{cookie}");
    assert!(cookie.contains("Max-Age=0"), "{cookie}");

    let report: AuditReport = serde_json::from_slice(&body).unwrap();
    for cat in ["sessions", "downloads"] {
        let before = report.before.iter().find(|c| c.category == cat).unwrap();
        let after = report.after.iter().find(|c| c.category == cat).unwrap();
        assert!(before.entries >= 1, "{cat}");
        assert_eq!((after.entries, after.bytes), (0, 0), "{cat}");
    }
    assert!(state.sessions.read().await.is_empty());
    assert!(state.downloads.read().await.is_empty());
    assert!(state.burn.is_burning());
}

#[tokio::test]
async fn second_burn_joins_first_report() {
    let (app, _, t) = authed_app().await;
    let (s1, _, b1) = send(&app, post_burn(Some(&t))).await;
    let (s2, _, b2) = send(&app, post_burn(Some(&t))).await;
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    let r1: AuditReport = serde_json::from_slice(&b1).unwrap();
    let r2: AuditReport = serde_json::from_slice(&b2).unwrap();
    assert_eq!(r1, r2);
}

// Feature: privacy-micro-apps-and-burn-workspace, Property 29: Gate 503 selama burning
//
// While `state.burn.is_burning()` is true, every request to `/api/apps/*` and
// `/api/mcp/*` (any method, any body) answers 503 and the body is never polled.
// `/api/burn`, `/api/burn/events` and `/api/auth/status` are exempt. The burning
// flag is set through `run_with` on an empty backend, so no real wipe or
// shutdown is ever triggered.
//
// **Validates: Requirements 10.6**
use crate::privacy::burn::{StorageBackend, WipeOutcome};
use crate::routes::create_api_router;
use crate::state::AppState;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use disposable_common::CategoryStat;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tower::ServiceExt;

/// Backend that wipes nothing; only used to flip the burning flag.
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

/// Body that records whether anyone polled it.
fn tracked_body(data: Vec<u8>, polled: Arc<AtomicBool>) -> Body {
    let stream = futures_util_like(data, polled);
    Body::from_stream(stream)
}

fn futures_util_like(
    data: Vec<u8>,
    polled: Arc<AtomicBool>,
) -> impl tokio_stream::Stream<Item = Result<Vec<u8>, Infallible>> {
    let mut chunk = Some(data);
    tokio_stream::StreamExt::map(
        tokio_stream::iter(std::iter::from_fn(move || {
            polled.store(true, Ordering::SeqCst);
            chunk.take()
        })),
        Ok,
    )
}

fn gated_path() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just("/api/apps"), Just("/api/mcp")],
        prop_oneof![
            Just(String::new()),
            "/[a-zA-Z0-9_.-]{1,12}(/[a-zA-Z0-9_.-]{1,12}){0,3}",
        ],
    )
        .prop_map(|(p, s)| format!("{p}{s}"))
}

fn method() -> impl Strategy<Value = Method> {
    prop_oneof![
        Just(Method::GET),
        Just(Method::POST),
        Just(Method::PUT),
        Just(Method::DELETE),
        Just(Method::PATCH),
    ]
}

async fn burning_app() -> axum::Router {
    let state = AppState::new();
    state.burn.run_with(&NoopBackend, None, None).await;
    assert!(state.burn.is_burning());
    create_api_router(state.clone()).with_state(state)
}

#[test]
fn prop_29_gate_503_while_burning() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    runner
        .run(
            &(gated_path(), method(), prop::collection::vec(any::<u8>(), 0..2048)),
            |(path, method, body)| {
                rt.block_on(async {
                    let app = burning_app().await;
                    let polled = Arc::new(AtomicBool::new(false));
                    let req = Request::builder()
                        .method(method)
                        .uri(&path)
                        .body(tracked_body(body, polled.clone()))
                        .unwrap();
                    let res = app.oneshot(req).await.unwrap();
                    prop_assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE, "path {}", path);
                    prop_assert!(!polled.load(Ordering::SeqCst), "body was read for {}", path);
                    Ok(())
                })
            },
        )
        .unwrap();
}

#[test]
fn prop_29_exempt_routes_not_gated() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    let exempt = prop_oneof![
        Just((Method::GET, "/api/auth/status")),
        // Unauthenticated: rejected with 401 by the extractor, never runs a burn.
        Just((Method::POST, "/api/burn")),
        Just((Method::GET, "/api/burn/events")),
    ];
    runner
        .run(&(exempt, prop::collection::vec(any::<u8>(), 0..256)), |((method, path), body)| {
            rt.block_on(async {
                let app = burning_app().await;
                let req = Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::from(body))
                    .unwrap();
                let res = app.oneshot(req).await.unwrap();
                prop_assert_ne!(res.status(), StatusCode::SERVICE_UNAVAILABLE, "path {}", path);
                Ok(())
            })
        })
        .unwrap();
}

#[test]
fn prop_29_not_gated_when_idle() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    runner
        .run(&gated_path(), |path| {
            rt.block_on(async {
                let state = AppState::new();
                let app = create_api_router(state.clone()).with_state(state);
                let req = Request::builder().method(Method::POST).uri(&path).body(Body::empty()).unwrap();
                let res = app.oneshot(req).await.unwrap();
                prop_assert_ne!(res.status(), StatusCode::SERVICE_UNAVAILABLE, "path {}", path);
                Ok(())
            })
        })
        .unwrap();
}

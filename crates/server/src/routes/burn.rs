//! HTTP adapters for Burn Workspace: `POST /api/burn`, `GET /api/burn/events`
//! and the `burn_gate` middleware (503 for app/MCP routes while burning).
use std::convert::Infallible;

use async_stream::stream;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use disposable_common::ApiResponse;
use tokio::sync::{broadcast::error::RecvError, oneshot};
use tokio_stream::Stream;

use crate::auth::{AuthenticatedSession, COOKIE_NAME};
use crate::state::AppState;

/// Paths that stay reachable while a burn is running or finished.
fn is_exempt(path: &str) -> bool {
    matches!(path, "/api/burn" | "/api/burn/events" | "/api/auth/status")
}

fn is_gated(path: &str) -> bool {
    path.starts_with("/api/apps/") || path == "/api/apps" || path.starts_with("/api/mcp/") || path == "/api/mcp"
}

/// Returns 503 for `/api/apps/*` and `/api/mcp/*` while a burn is in progress,
/// without reading the request body.
pub async fn burn_gate(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if state.burn.is_burning() && !is_exempt(path) && is_gated(path) {
        let mut body = serde_json::to_value(ApiResponse::<()> {
            success: false,
            message: Some("Workspace is being burned".to_string()),
            data: None,
        })
        .unwrap_or_else(|_| serde_json::json!({ "success": false }));
        if let Some(obj) = body.as_object_mut() {
            obj.insert("code".to_string(), serde_json::json!("burn_in_progress"));
        }
        return (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response();
    }
    next.run(req).await
}

/// `POST /api/burn`: run (or join) the burn and return the `AuditReport`.
pub async fn post_burn(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    // The guard `tx` is dropped when the body stream is exhausted or dropped,
    // or when this handler is dropped before building the body (client
    // disconnect). Either way `rx` resolves and shutdown is requested.
    let (tx, rx) = oneshot::channel::<()>();

    // Run the burn in a detached task: hyper drops this handler future when
    // the client disconnects, which must not cancel the wipe midway nor skip
    // the shutdown request (Req 12.4).
    let task_state = state.clone();
    let report = match tokio::spawn(async move {
        let report = task_state.burn.run_or_join(&task_state).await;
        task_state.burn.schedule_shutdown(async move {
            let _ = rx.await;
        });
        report
    })
    .await
    {
        Ok(r) => r,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut removal_cookie = Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build();
    removal_cookie.make_removal();
    let jar = jar.add(removal_cookie);

    let bytes = match serde_json::to_vec(&*report) {
        Ok(b) => b,
        // Dropping `tx` here resolves the shutdown wait immediately.
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let body_stream = stream! {
        let _guard = tx;
        yield Ok::<_, Infallible>(bytes);
    };

    (
        jar,
        [(header::CONTENT_TYPE, "application/json")],
        Body::from_stream(body_stream),
    )
        .into_response()
}

/// `GET /api/burn/events`: forwards the burn report as `event: burn`.
pub async fn burn_events(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.ui_events.subscribe();
    let s = stream! {
        loop {
            match rx.recv().await {
                Ok(report) => {
                    if let Ok(data) = serde_json::to_string(&*report) {
                        yield Ok(Event::default().event("burn").data(data));
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    };
    Sse::new(s).keep_alive(KeepAlive::default())
}

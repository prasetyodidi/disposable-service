pub mod burn;
pub mod auth;
pub mod jwt;
pub mod redactor;
pub mod mcp;
pub mod monitor;
pub mod pandoc;
pub mod pandoc_preset;
pub mod pdf_to_markdown;
pub mod scrubber;

use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

/// Builds the API router. `state` is needed by the burn gate middleware.
pub fn create_api_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(auth::get_auth_status))
        .route("/api/auth/setup", post(auth::setup_password))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/token", post(auth::create_token))
        .route("/api/sessions", get(auth::get_sessions))
        .route("/api/apps/pandoc/preset", get(pandoc::get_pandoc_preset))
        .route("/api/apps/pandoc/convert", post(pandoc::convert_pandoc_json))
        .route(
            "/api/apps/pandoc/convert-upload",
            post(pandoc::convert_pandoc_multipart),
        )
        .route(
            "/api/apps/pandoc/download/:token",
            get(pandoc::download_pandoc_ephemeral),
        )
        .route("/api/apps/monitor", get(monitor::get_host_metrics))
        .route("/api/apps/monitor/metrics", get(monitor::get_host_metrics))
        .route("/api/apps/monitor/stream", get(monitor::stream_host_metrics))
        // Legacy SSE transport (GET) plus Streamable HTTP style POST on the same URL,
        // so clients configured with only the SSE URL do not get HTTP 405.
        .route(
            "/api/mcp/sse",
            get(mcp::mcp_sse_handler).post(mcp::mcp_message_handler).layer(
                axum::extract::DefaultBodyLimit::max(36 * 1024 * 1024),
            ),
        )
        .route(
            "/api/mcp",
            post(mcp::mcp_message_handler).layer(axum::extract::DefaultBodyLimit::max(
                36 * 1024 * 1024,
            )),
        )
        .route(
            "/api/mcp/message",
            post(mcp::mcp_message_handler).layer(axum::extract::DefaultBodyLimit::max(
                // base64 of a 25 MiB file (+33%) plus JSON-RPC overhead.
                36 * 1024 * 1024,
            )),
        )
        .route("/api/apps/redactor", post(redactor::redact_secrets).layer(redactor::body_limit_layer()))
        .route("/api/apps/jwt", post(jwt::inspect_jwt))
        .route("/api/burn", post(burn::post_burn))
        .route("/api/burn/events", get(burn::burn_events))
        .merge(scrubber::router())
        .merge(pdf_to_markdown::router(&state))
        // Gate runs before auth extraction and handlers.
        .layer(from_fn_with_state(state, burn::burn_gate))
        // Outermost: normalizes PDF route responses (no-store, error body shape).
        .layer(axum::middleware::from_fn(pdf_to_markdown::finalize_pdf_response))
}

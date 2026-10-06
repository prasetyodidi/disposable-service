pub mod auth;
pub mod pandoc;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn create_api_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/status", get(auth::get_auth_status))
        .route("/api/auth/setup", post(auth::setup_password))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/sessions", get(auth::get_sessions))
        .route("/api/apps/pandoc/convert", post(pandoc::convert_pandoc_json))
        .route(
            "/api/apps/pandoc/convert-upload",
            post(pandoc::convert_pandoc_multipart),
        )
}

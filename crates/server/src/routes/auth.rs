use axum::extract::{ConnectInfo, State};
use axum::http::header::USER_AGENT;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Utc};
use disposable_common::{
    ActiveSessionsResponse, ApiResponse, AuthStatusResponse, LoginRequest,
    SessionInfo, SetupPasswordRequest,
};
use std::net::SocketAddr;
use uuid::Uuid;

use crate::auth::{
    create_jwt, hash_password, validate_jwt, verify_password, AuthenticatedSession, COOKIE_NAME,
};
use crate::state::AppState;

pub async fn get_auth_status(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    let is_claimed = state.is_claimed().await;
    let mut is_authenticated = false;

    if let Some(cookie) = jar.get(COOKIE_NAME) {
        if let Ok(claims) = validate_jwt(cookie.value(), &state.jwt_secret) {
            if state.get_session(&claims.sub).await.is_some() {
                is_authenticated = true;
            }
        }
    }

    Json(AuthStatusResponse {
        is_claimed,
        is_authenticated,
    })
}

pub async fn setup_password(
    State(state): State<AppState>,
    Json(req): Json<SetupPasswordRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    if req.password.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                message: Some("Password cannot be empty".to_string()),
                data: None,
            }),
        ));
    }

    if state.is_claimed().await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                message: Some("Instance has already been claimed".to_string()),
                data: None,
            }),
        ));
    }

    let hashed = hash_password(&req.password).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                message: Some(e),
                data: None,
            }),
        )
    })?;

    state.set_password_hash(hashed).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                message: Some(e.to_string()),
                data: None,
            }),
        )
    })?;

    Ok(Json(ApiResponse::<()> {
        success: true,
        message: Some("Password successfully set. You can now login.".to_string()),
        data: None,
    }))
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiResponse<()>>)> {
    let stored_hash = {
        let lock = state.password_hash.read().await;
        lock.clone()
    };

    let hash = match stored_hash {
        Some(h) => h,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse {
                    success: false,
                    message: Some("Setup required: no password has been configured yet".to_string()),
                    data: None,
                }),
            ));
        }
    };

    if !verify_password(&req.password, &hash) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse {
                success: false,
                message: Some("Invalid password".to_string()),
                data: None,
            }),
        ));
    }

    let session_id = Uuid::new_v4().to_string();
    let (token, _exp) = create_jwt(&session_id, &state.jwt_secret).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                message: Some(e),
                data: None,
            }),
        )
    })?;

    let ip_address = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| connect_info.map(|ci| ci.0.ip().to_string()))
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("Unknown Device")
        .to_string();

    let now = Utc::now();
    let session = SessionInfo {
        session_id: session_id.clone(),
        ip_address,
        user_agent,
        login_time: now,
        expires_at: now + Duration::hours(1),
    };

    state.add_session(session).await;

    // Cookie expires in 1 hour
    let cookie = Cookie::build((COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::hours(1))
        .build();

    let updated_jar = jar.add(cookie);

    Ok((
        updated_jar,
        Json(ApiResponse::<()> {
            success: true,
            message: Some("Login successful".to_string()),
            data: None,
        }),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if let Some(cookie) = jar.get(COOKIE_NAME) {
        if let Ok(claims) = validate_jwt(cookie.value(), &state.jwt_secret) {
            state.remove_session(&claims.sub).await;
        }
    }

    let mut removal_cookie = Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build();
    removal_cookie.make_removal();

    let updated_jar = jar.add(removal_cookie);

    (
        updated_jar,
        Json(ApiResponse::<()> {
            success: true,
            message: Some("Logged out successfully".to_string()),
            data: None,
        }),
    )
}

pub async fn get_sessions(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let sessions = state.list_sessions().await;
    let total = sessions.len();
    Json(ActiveSessionsResponse { sessions, total })
}

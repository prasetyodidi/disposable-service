use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::CookieJar;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use disposable_common::SessionInfo;

pub const COOKIE_NAME: &str = "disposable_session";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // session_id
    pub exp: usize,  // epoch seconds
    pub iat: usize,  // epoch seconds
}

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("Failed to hash password: {}", e))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn create_jwt(session_id: &str, secret: &[u8]) -> Result<(String, usize), String> {
    let now = Utc::now();
    let expires = now + Duration::hours(1);
    let exp = expires.timestamp() as usize;
    let iat = now.timestamp() as usize;

    let claims = Claims {
        sub: session_id.to_string(),
        exp,
        iat,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| format!("Failed to encode JWT: {}", e))?;

    Ok((token, exp))
}

pub fn validate_jwt(token: &str, secret: &[u8]) -> Result<Claims, String> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &validation,
    )
    .map_err(|e| format!("Invalid token: {}", e))?;

    Ok(token_data.claims)
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct AuthenticatedSession(pub SessionInfo);

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedSession {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Check Authorization: Bearer <token> header first
        let bearer_token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|val| val.to_str().ok())
            .and_then(|val| {
                val.strip_prefix("Bearer ")
                    .or_else(|| val.strip_prefix("bearer "))
                    .map(|t| t.trim().to_string())
            });

        // 2. Fallback to disposable_session cookie if no Bearer token
        let token_str = match bearer_token {
            Some(t) => Some(t),
            None => {
                let jar = CookieJar::from_request_parts(parts, state)
                    .await
                    .map_err(|_| (StatusCode::UNAUTHORIZED, "Unauthorized").into_response())?;
                jar.get(COOKIE_NAME).map(|c| c.value().to_string())
            }
        };

        let token = token_str
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "No active session").into_response())?;

        // 3. Validate token: Try JWT validation first; if valid, fetch session by claims.sub
        let session = if let Ok(claims) = validate_jwt(&token, &state.jwt_secret) {
            if let Some(s) = state.get_session(&claims.sub).await {
                Some(s)
            } else {
                // If claims.sub session not found, also check direct session token
                state.get_session(&token).await
            }
        } else {
            // Also allow direct login session token if it matches a valid session in state
            state.get_session(&token).await
        };

        let session = session.ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, "Session expired or invalid").into_response()
        })?;

        Ok(AuthenticatedSession(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn test_password_hashing_and_verification() {
        let password = "super_secret_ephemeral_password";
        let hash = hash_password(password).expect("Hash should succeed");
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_jwt_lifecycle() {
        let secret = b"my_super_secret_test_key_which_is_long_enough_12345";
        let session_id = "test-session-123";

        let (token, _exp) = create_jwt(session_id, secret).expect("JWT creation should succeed");
        let claims = validate_jwt(&token, secret).expect("JWT validation should succeed");
        assert_eq!(claims.sub, session_id);

        let wrong_secret = b"wrong_secret_key_12345678901234567890";
        assert!(validate_jwt(&token, wrong_secret).is_err());
    }

    #[tokio::test]
    async fn test_bearer_token_authentication_jwt() {
        let state = AppState::new();
        let session_id = "session-bearer-jwt-1";
        let session = SessionInfo {
            session_id: session_id.to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        state.add_session(session).await;

        let (jwt, _) = create_jwt(session_id, &state.jwt_secret).unwrap();

        // 1. With Bearer token in Authorization header
        let req = Request::builder()
            .header("Authorization", format!("Bearer {}", jwt))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();

        let auth = AuthenticatedSession::from_request_parts(&mut parts, &state).await;
        assert!(auth.is_ok());
        assert_eq!(auth.unwrap().0.session_id, session_id);

        // 2. Case-insensitive 'bearer' prefix
        let req = Request::builder()
            .header("Authorization", format!("bearer {}", jwt))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();

        let auth = AuthenticatedSession::from_request_parts(&mut parts, &state).await;
        assert!(auth.is_ok());
    }

    #[tokio::test]
    async fn test_bearer_token_authentication_direct_session_id() {
        let state = AppState::new();
        let session_id = "session-direct-token-999";
        let session = SessionInfo {
            session_id: session_id.to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        state.add_session(session).await;

        // Use raw session_id directly as Bearer token
        let req = Request::builder()
            .header("Authorization", format!("Bearer {}", session_id))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();

        let auth = AuthenticatedSession::from_request_parts(&mut parts, &state).await;
        assert!(auth.is_ok());
        assert_eq!(auth.unwrap().0.session_id, session_id);
    }

    #[tokio::test]
    async fn test_cookie_authentication() {
        let state = AppState::new();
        let session_id = "session-cookie-test-42";
        let session = SessionInfo {
            session_id: session_id.to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        state.add_session(session).await;

        let (jwt, _) = create_jwt(session_id, &state.jwt_secret).unwrap();

        let req = Request::builder()
            .header("Cookie", format!("{}={}", COOKIE_NAME, jwt))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();

        let auth = AuthenticatedSession::from_request_parts(&mut parts, &state).await;
        assert!(auth.is_ok());
        assert_eq!(auth.unwrap().0.session_id, session_id);
    }

    #[tokio::test]
    async fn test_auth_rejection_missing_or_invalid() {
        let state = AppState::new();

        // 1. Missing header and cookie -> 401
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let res = AuthenticatedSession::from_request_parts(&mut parts, &state).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().status(), StatusCode::UNAUTHORIZED);

        // 2. Invalid Bearer token -> 401
        let req = Request::builder()
            .header("Authorization", "Bearer invalid-garbage-token")
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let res = AuthenticatedSession::from_request_parts(&mut parts, &state).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().status(), StatusCode::UNAUTHORIZED);
    }
}

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

#[allow(dead_code)]
pub struct AuthenticatedSession(pub SessionInfo);

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedSession {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Unauthorized").into_response())?;

        let cookie = jar
            .get(COOKIE_NAME)
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "No active session").into_response())?;

        let claims = validate_jwt(cookie.value(), &state.jwt_secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Session expired or invalid").into_response())?;

        let session = state
            .get_session(&claims.sub)
            .await
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Session not found or revoked").into_response())?;

        Ok(AuthenticatedSession(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

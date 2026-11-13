use axum::http::StatusCode;
use axum::Json;
use disposable_common::{ApiResponse, JwtInspectRequest, JwtInspectResult};
use zeroize::Zeroizing;

use crate::auth::AuthenticatedSession;
use crate::privacy::jwt_inspector::{inspect, InspectOptions};

pub async fn inspect_jwt(
    _auth: AuthenticatedSession,
    Json(req): Json<JwtInspectRequest>,
) -> Result<Json<JwtInspectResult>, (StatusCode, Json<ApiResponse<()>>)> {
    let JwtInspectRequest {
        token,
        secret,
        redact,
    } = req;

    // Sensitive inputs are wiped on drop, before the response is returned.
    let token = Zeroizing::new(token);
    let secret = secret.map(Zeroizing::new);

    let now_unix = chrono::Utc::now().timestamp();
    let result = {
        let opts = InspectOptions {
            hmac_secret: secret.as_ref().map(|s| s.as_bytes()),
            redact,
        };
        inspect(token.as_str(), &opts, now_unix)
    };

    drop(token);
    drop(secret);

    result.map(Json).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiResponse {
                success: false,
                message: Some(e.to_string()),
                data: None,
            }),
        )
    })
}

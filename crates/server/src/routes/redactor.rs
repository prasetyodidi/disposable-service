use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::Json;
use disposable_common::{ApiResponse, RedactRequest, RedactResponse};
use std::collections::BTreeSet;

use crate::auth::AuthenticatedSession;
use crate::privacy::redactor::{
    redact, validate, Category, Format, RedactOptions, MAX_TEXT_BYTES,
};

/// Transport-level body limit (JSON escaping can inflate the 5 MiB logical limit).
pub const REDACTOR_BODY_LIMIT_BYTES: usize = 16 * 1024 * 1024;

/// Layer to be applied on the redactor route at registration time.
pub fn body_limit_layer() -> DefaultBodyLimit {
    DefaultBodyLimit::max(REDACTOR_BODY_LIMIT_BYTES)
}

type ApiError = (StatusCode, Json<ApiResponse<()>>);

fn api_error(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (
        status,
        Json(ApiResponse {
            success: false,
            message: Some(msg.into()),
            data: None,
        }),
    )
}

/// Maps the wire request to pure-core options. Error messages never quote user input.
fn to_options(req: &RedactRequest) -> Result<RedactOptions, ApiError> {
    let categories: BTreeSet<Category> = match &req.categories {
        None => Category::ALL.iter().copied().collect(),
        Some(list) => {
            let mut set = BTreeSet::new();
            for name in list {
                match Category::parse(name) {
                    Some(c) => {
                        set.insert(c);
                    }
                    None => {
                        return Err(api_error(
                            StatusCode::BAD_REQUEST,
                            "Kategori redaksi tidak dikenal",
                        ))
                    }
                }
            }
            set
        }
    };

    let format = match &req.format {
        None => Format::Auto,
        Some(f) => Format::parse(f).ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "Format tidak dikenal (gunakan auto, env, atau text)",
            )
        })?,
    };

    Ok(RedactOptions {
        categories,
        custom_words: req.custom_words.clone(),
        format,
    })
}

pub async fn redact_secrets(
    _auth: AuthenticatedSession,
    Json(req): Json<RedactRequest>,
) -> Result<Json<RedactResponse>, ApiError> {
    if req.text.len() > MAX_TEXT_BYTES {
        return Err(api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("Teks melebihi batas {} byte", MAX_TEXT_BYTES),
        ));
    }

    let opts = to_options(&req)?;
    validate(&opts).map_err(|e| api_error(StatusCode::BAD_REQUEST, e.to_string()))?;

    let outcome = redact(&req.text, &opts);
    let counts = outcome
        .counts
        .into_iter()
        .map(|(c, n)| (c.as_str().to_string(), n))
        .collect();

    Ok(Json(RedactResponse {
        text: outcome.text,
        counts,
    }))
}

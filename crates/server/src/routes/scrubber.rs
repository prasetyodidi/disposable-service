//! HTTP adapter for the metadata scrubber. Core logic lives in `privacy::scrubber`.
//!
//! Nothing user-supplied (file name, content, metadata) is ever logged here.

use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use base64::Engine;
use disposable_common::{ApiResponse, ScrubEphemeralResponse};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::privacy::scrubber::{self, FileKind, ScrubError};
use crate::state::AppState;

/// Ephemeral download lifetime in seconds.
pub const EPHEMERAL_TTL_SECS: u64 = 300;

/// 25 MiB logical limit + 1 MiB multipart overhead.
pub const BODY_LIMIT_BYTES: usize = 26 * 1024 * 1024;

type ApiError = (StatusCode, Json<ApiResponse<()>>);

#[derive(Debug, Default, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScrubMode {
    #[default]
    Direct,
    Ephemeral,
}

#[derive(Debug, Deserialize)]
pub struct ScrubQuery {
    #[serde(default)]
    pub mode: ScrubMode,
}

fn api_error(status: StatusCode, message: &str) -> ApiError {
    (
        status,
        Json(ApiResponse {
            success: false,
            message: Some(message.to_string()),
            data: None,
        }),
    )
}

fn map_scrub_error(err: ScrubError) -> ApiError {
    match err {
        ScrubError::TooLarge => api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "File terlalu besar. Batas maksimum adalah 25 MiB (26214400 byte).",
        ),
        ScrubError::UnsupportedType => api_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Tipe file tidak didukung. Hanya JPEG, PNG, dan PDF.",
        ),
        ScrubError::Unreadable(_) => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "File tidak dapat dibaca atau rusak.",
        ),
        ScrubError::EncryptedPdf => api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "PDF terenkripsi tidak didukung.",
        ),
    }
}

fn extension(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Jpeg => "jpg",
        FileKind::Png => "png",
        FileKind::Pdf => "pdf",
    }
}

fn content_type(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Jpeg => "image/jpeg",
        FileKind::Png => "image/png",
        FileKind::Pdf => "application/pdf",
    }
}

/// Build a safe ASCII download name: `scrubbed-<stem>.<ext>`.
fn output_filename(original: Option<&str>, kind: FileKind) -> String {
    let stem: String = original
        .map(|n| n.rsplit(['/', '\\']).next().unwrap_or(""))
        .map(|n| n.rsplit_once('.').map(|(s, _)| s).unwrap_or(n))
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .take(80)
        .collect();
    let stem = stem.trim();
    if stem.is_empty() {
        format!("scrubbed.{}", extension(kind))
    } else {
        format!("scrubbed-{}.{}", stem, extension(kind))
    }
}

/// Store bytes as an ephemeral (burn-after-reading) download and return its token.
/// Shared with the MCP `scrub_metadata` tool.
pub(crate) async fn store_ephemeral(
    state: &AppState,
    bytes: Vec<u8>,
    filename: String,
    content_type: String,
) -> String {
    state
        .store_ephemeral_download(bytes, filename, content_type, EPHEMERAL_TTL_SECS)
        .await
}

pub async fn scrub_upload(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
    Query(query): Query<ScrubQuery>,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    let mut file: Option<(axum::body::Bytes, Option<String>)> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return Err(multipart_error(e.status())),
        };
        if field.name() == Some("file") && file.is_none() {
            let name = field.file_name().map(|s| s.to_string());
            match field.bytes().await {
                Ok(b) => file = Some((b, name)),
                Err(e) => return Err(multipart_error(e.status())),
            }
        }
    }

    let (data, original_name) = file.ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "Field 'file' tidak ditemukan pada permintaan.",
        )
    })?;

    // CPU-bound: keep the async runtime free. The input buffer is moved into
    // the closure and dropped as soon as scrubbing finishes.
    let output = tokio::task::spawn_blocking(move || scrubber::scrub(&data))
        .await
        .map_err(|_| {
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "File tidak dapat dibaca atau rusak.",
            )
        })?
        .map_err(map_scrub_error)?;

    let kind = output.kind;
    let filename = output_filename(original_name.as_deref(), kind);
    let ctype = content_type(kind);
    let report = output.report;

    match query.mode {
        ScrubMode::Direct => {
            let report_json = serde_json::to_vec(&report).map_err(|_| {
                api_error(StatusCode::INTERNAL_SERVER_ERROR, "Gagal menyusun laporan.")
            })?;
            let report_b64 = BASE64_URL_SAFE_NO_PAD.encode(report_json);

            let mut response = output.bytes.into_response();
            let headers = response.headers_mut();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(ctype));
            headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
            {
                headers.insert(CONTENT_DISPOSITION, v);
            }
            if let Ok(v) = HeaderValue::from_str(&report_b64) {
                headers.insert("x-scrub-report", v);
            }
            Ok(response)
        }
        ScrubMode::Ephemeral => {
            let token =
                store_ephemeral(&state, output.bytes, filename.clone(), ctype.to_string()).await;
            let body = ScrubEphemeralResponse {
                download_url: format!("/api/apps/scrubber/download/{}", token),
                token,
                filename,
                report,
                expires_in_secs: EPHEMERAL_TTL_SECS,
            };
            let mut response = Json(body).into_response();
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            Ok(response)
        }
    }
}

fn multipart_error(status: StatusCode) -> ApiError {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        map_scrub_error(ScrubError::TooLarge)
    } else {
        api_error(StatusCode::BAD_REQUEST, "Permintaan multipart tidak valid.")
    }
}

/// Reuses the pandoc ephemeral download handler and adds `Cache-Control: no-store`.
pub async fn download_scrubber_ephemeral(
    path: Path<String>,
    state: State<AppState>,
) -> Result<Response, ApiError> {
    let mut response = super::pandoc::download_pandoc_ephemeral(path, state).await?;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// Layer for the upload route (26 MiB body limit). Registered in task 7.4.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/apps/scrubber",
            post(scrub_upload).layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/apps/scrubber/download/:token",
            get(download_scrubber_ephemeral),
        )
}

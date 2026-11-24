//! HTTP adapter for the PDF to Markdown mini app. Core logic lives in `pdf2md`.
//!
//! Nothing user-supplied (file name, content, options, converted text) is ever logged here.
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use disposable_common::{PdfApiError, PdfConversionResult, PdfLimits};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthenticatedSession;
use crate::pdf2md::config::{DOWNLOAD_TTL_SECS, MAX_PAGES};
use crate::pdf2md::convert::{convert, Conversion, ConvertInput};
use crate::pdf2md::error::{ConvertError, ErrorCode};
use crate::state::AppState;

/// Path prefix shared by the three routes.
const PATH_PREFIX: &str = "/api/apps/pdf-to-markdown";
/// Maximum size of the `options` multipart part (Requirement 1.4).
const OPTIONS_MAX_BYTES: usize = 4_096;
/// Fixed download file name; never derived from the upload.
const DOWNLOAD_FILENAME: &str = "document.md";
const MARKDOWN_CONTENT_TYPE: &str = "text/markdown; charset=utf-8";

#[derive(Debug, Deserialize)]
pub struct ConvertQuery {
    #[serde(default)]
    pub download: Option<String>,
}

/// JSON error body `{success:false, code, message}` for a given code and message.
pub(crate) fn error_body(code: ErrorCode, message: &str) -> Response {
    let body = PdfApiError {
        success: false,
        code: code.code_str().to_string(),
        message: message.to_string(),
    };
    (code.http_status(), Json(body)).into_response()
}

/// Maps a `ConvertError` to its HTTP response (status, `code`, static message).
pub(crate) fn convert_error_response(err: &ConvertError) -> Response {
    error_body(err.code(), &err.message())
}

/// Builds the REST result from a conversion (shared with the MCP tool).
pub(crate) fn conversion_to_result(c: Conversion, download_token: Option<String>) -> PdfConversionResult {
    PdfConversionResult {
        markdown: c.markdown,
        pages: c.pages,
        headings: c.headings,
        warnings: c.warnings,
        download_token,
    }
}

fn multipart_error(status: StatusCode, max_bytes: u64) -> ConvertError {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        ConvertError::FileTooLarge { max_bytes }
    } else {
        ConvertError::BadRequest
    }
}

pub async fn convert_pdf(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
    Query(query): Query<ConvertQuery>,
    mut multipart: Multipart,
) -> Response {
    let max_bytes = state.pdf.config.max_bytes;
    let mut file: Option<Vec<u8>> = None;
    let mut options: Option<Vec<u8>> = None;
    let mut extra_file_parts = 0usize;

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return convert_error_response(&multipart_error(e.status(), max_bytes)),
        };
        match field.name() {
            Some("file") if file.is_none() => {
                let mut buf: Vec<u8> = Vec::new();
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            buf.extend_from_slice(&chunk);
                            if buf.len() as u64 > max_bytes {
                                return convert_error_response(&ConvertError::FileTooLarge {
                                    max_bytes,
                                });
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            return convert_error_response(&multipart_error(e.status(), max_bytes))
                        }
                    }
                }
                file = Some(buf);
            }
            Some("file") => {
                // Drain without storing.
                extra_file_parts += 1;
                loop {
                    match field.chunk().await {
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(e) => {
                            return convert_error_response(&multipart_error(e.status(), max_bytes))
                        }
                    }
                }
            }
            Some("options") if options.is_none() => {
                let mut buf: Vec<u8> = Vec::new();
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            // Keep at most limit + 1 bytes so parse_options can reject oversize input.
                            if buf.len() <= OPTIONS_MAX_BYTES {
                                let room = OPTIONS_MAX_BYTES + 1 - buf.len();
                                buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            return convert_error_response(&multipart_error(e.status(), max_bytes))
                        }
                    }
                }
                options = Some(buf);
            }
            _ => {}
        }
    }

    let Some(file) = file else {
        return convert_error_response(&ConvertError::FileRequired);
    };

    let download = query.download.as_deref() == Some("true");
    let input = ConvertInput { file, options_raw: options, extra_file_parts };
    match convert(&state.pdf, input).await {
        Ok(conversion) => {
            let token = if download {
                Some(
                    state
                        .store_ephemeral_download(
                            conversion.markdown.clone().into_bytes(),
                            DOWNLOAD_FILENAME.to_string(),
                            MARKDOWN_CONTENT_TYPE.to_string(),
                            DOWNLOAD_TTL_SECS,
                        )
                        .await,
                )
            } else {
                None
            };
            let mut response = Json(conversion_to_result(conversion, token)).into_response();
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(e) => convert_error_response(&e),
    }
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(PdfApiError {
            success: false,
            code: "not_found".to_string(),
            message: "Unduhan tidak ditemukan, sudah kedaluwarsa, atau sudah dipakai.".to_string(),
        }),
    )
        .into_response()
}

pub async fn download_markdown(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    if Uuid::parse_str(&token).is_err() {
        return not_found();
    }
    let Some(download) = state
        .take_ephemeral_download_if(&token, |d| d.content_type.starts_with("text/markdown"))
        .await
    else {
        return not_found();
    };
    let mut response = Response::new(Body::from(download.data));
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(MARKDOWN_CONTENT_TYPE));
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"document.md\""),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub async fn get_limits(_auth: AuthenticatedSession, State(state): State<AppState>) -> Response {
    Json(PdfLimits { max_bytes: state.pdf.config.max_bytes, max_pages: MAX_PAGES }).into_response()
}

/// Sets `Cache-Control: no-store` on every PDF route response and normalizes the
/// 401/503/413 bodies produced outside the handlers to `{success:false, code, message}`.
pub async fn finalize_pdf_response(req: Request, next: Next) -> Response {
    let is_pdf = req.uri().path().starts_with(PATH_PREFIX);
    let response = next.run(req).await;
    if !is_pdf {
        return response;
    }
    let status = response.status();
    let normalized = match status {
        StatusCode::UNAUTHORIZED => Some((ErrorCode::Unauthorized, "Autentikasi diperlukan.".to_string())),
        StatusCode::SERVICE_UNAVAILABLE => {
            Some((ErrorCode::BurnInProgress, "Workspace sedang dihancurkan.".to_string()))
        }
        StatusCode::PAYLOAD_TOO_LARGE => Some((
            ErrorCode::FileTooLarge,
            "Ukuran file melebihi batas yang diizinkan.".to_string(),
        )),
        _ => None,
    };
    let mut response = match normalized {
        Some((code, message)) => {
            let (parts, body) = response.into_parts();
            let bytes = axum::body::to_bytes(body, 64 * 1024).await.unwrap_or_default();
            let has_code = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .map(|v| v.get("code").is_some())
                .unwrap_or(false);
            if has_code {
                Response::from_parts(parts, Body::from(bytes))
            } else {
                let mut r = error_body(code, &message);
                // Keep the original status (e.g. engine_unavailable is also 503).
                *r.status_mut() = status;
                r
            }
        }
        None => response,
    };
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Router for the three routes. The body limit applies only to the POST route.
pub fn router(state: &AppState) -> Router<AppState> {
    let limit = state.pdf.config.body_limit();
    Router::new()
        .route(
            PATH_PREFIX,
            post(convert_pdf).layer(DefaultBodyLimit::max(limit)),
        )
        .route(&format!("{PATH_PREFIX}/download/:token"), get(download_markdown))
        .route(&format!("{PATH_PREFIX}/limits"), get(get_limits))
}

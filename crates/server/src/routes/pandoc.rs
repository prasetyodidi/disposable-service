use axum::extract::Multipart;
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use disposable_common::{ApiResponse, PandocConvertRequest};
use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;
use uuid::Uuid;

use crate::auth::AuthenticatedSession;

struct TempDirGuard {
    path: PathBuf,
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

pub async fn convert_pandoc_json(
    _auth: AuthenticatedSession,
    Json(req): Json<PandocConvertRequest>,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    if req.markdown.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                message: Some("Markdown content is empty".to_string()),
                data: None,
            }),
        ));
    }

    let filename = req.filename.unwrap_or_else(|| "document.pdf".to_string());
    let pdf_filename = if filename.ends_with(".pdf") {
        filename
    } else {
        format!("{}.pdf", filename.trim_end_matches(".md"))
    };

    execute_pandoc_conversion(&req.markdown, &pdf_filename).await
}

pub async fn convert_pandoc_multipart(
    _auth: AuthenticatedSession,
    mut multipart: Multipart,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    let mut markdown_content = String::new();
    let mut original_filename = "document.pdf".to_string();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" || name == "markdown_file" {
            if let Some(f_name) = field.file_name() {
                original_filename = format!("{}.pdf", f_name.trim_end_matches(".md"));
            }
            if let Ok(bytes) = field.bytes().await {
                markdown_content = String::from_utf8_lossy(&bytes).to_string();
            }
        } else if name == "markdown" {
            if let Ok(text) = field.text().await {
                markdown_content = text;
            }
        }
    }

    if markdown_content.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                message: Some("No markdown content or file was uploaded".to_string()),
                data: None,
            }),
        ));
    }

    execute_pandoc_conversion(&markdown_content, &original_filename).await
}

async fn execute_pandoc_conversion(
    markdown: &str,
    output_filename: &str,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    let job_id = Uuid::new_v4().to_string();
    let temp_dir_path = std::env::temp_dir().join(format!("pandoc-job-{}", job_id));

    if let Err(e) = fs::create_dir_all(&temp_dir_path).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                message: Some(format!("Failed to create temporary workspace: {}", e)),
                data: None,
            }),
        ));
    }

    let _guard = TempDirGuard {
        path: temp_dir_path.clone(),
    };

    let input_path = temp_dir_path.join("input.md");
    let output_path = temp_dir_path.join("output.pdf");

    if let Err(e) = fs::write(&input_path, markdown).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                message: Some(format!("Failed to write input markdown: {}", e)),
                data: None,
            }),
        ));
    }

    let pdf_engine = std::env::var("PANDOC_PDF_ENGINE").unwrap_or_else(|_| "pdflatex".to_string());
    let pandoc_bin = std::env::var("PANDOC_BIN").unwrap_or_else(|_| "pandoc".to_string());

    // Execute Pandoc with parameters specified in PRD Req 4.3.2
    let mut cmd = Command::new(&pandoc_bin);
    cmd.arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg(format!("--pdf-engine={}", pdf_engine))
        .arg("-V")
        .arg("papersize=a4")
        .arg("-V")
        .arg("geometry:top=1.8cm, bottom=1.8cm, left=2cm, right=2cm")
        .arg("-V")
        .arg("fontsize=10.5pt")
        .arg("-V")
        .arg("lineheight=1.25")
        .arg("-V")
        .arg("colorlinks=true")
        .arg("-V")
        .arg("linkcolor=navy");

    // Optional font override
    if let Ok(font) = std::env::var("PANDOC_MAIN_FONT") {
        cmd.arg("-V").arg(format!("mainfont={}", font));
    }

    let output = match cmd.output().await {
        Ok(out) => out,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    message: Some(format!(
                        "Failed to execute pandoc binary (is pandoc installed?): {}",
                        e
                    )),
                    data: None,
                }),
            ));
        }
    };

    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                message: Some(format!("Pandoc conversion failed: {}", stderr_str)),
                data: None,
            }),
        ));
    }

    let pdf_bytes = match fs::read(&output_path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    message: Some(format!("Failed to read generated PDF: {}", e)),
                    data: None,
                }),
            ));
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    let disposition = format!("attachment; filename=\"{}\"", output_filename);
    if let Ok(val) = HeaderValue::from_str(&disposition) {
        headers.insert(CONTENT_DISPOSITION, val);
    }

    // Explicit early cleanup before returning stream
    if temp_dir_path.exists() {
        let _ = fs::remove_dir_all(&temp_dir_path).await;
    }

    Ok((headers, pdf_bytes).into_response())
}

use axum::extract::{Multipart, Path, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use disposable_common::{ApiResponse, PandocConvertRequest};
use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;
use uuid::Uuid;

use crate::auth::AuthenticatedSession;
use crate::state::AppState;

pub struct TempDirGuard {
    pub path: PathBuf,
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

fn binary_available(bin: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

#[allow(dead_code)]
pub async fn generate_pdf_base64(
    markdown: &str,
    output_filename: &str,
) -> Result<String, (StatusCode, String)> {
    let bytes = generate_pandoc_pdf_bytes(markdown, output_filename).await?;
    Ok(BASE64_STANDARD.encode(&bytes))
}

pub async fn generate_pandoc_pdf_bytes(
    markdown: &str,
    _output_filename: &str,
) -> Result<Vec<u8>, (StatusCode, String)> {
    let job_id = Uuid::new_v4().to_string();
    let temp_dir_path = std::env::temp_dir().join(format!("pandoc-job-{}", job_id));

    if let Err(e) = fs::create_dir_all(&temp_dir_path).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create temporary workspace: {}", e),
        ));
    }

    let _guard = TempDirGuard {
        path: temp_dir_path.clone(),
    };

    let input_md_path = temp_dir_path.join("input.md");
    let input_typ_path = temp_dir_path.join("input.typ");
    let output_path = temp_dir_path.join("output.pdf");

    if let Err(e) = fs::write(&input_md_path, markdown).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write input markdown: {}", e),
        ));
    }
    let _ = fs::write(&input_typ_path, markdown).await;

    let doc_engine = std::env::var("DOC_ENGINE").unwrap_or_default().to_lowercase();
    let pdf_engine = std::env::var("PANDOC_PDF_ENGINE").unwrap_or_else(|_| "pdflatex".to_string());
    let pandoc_bin = std::env::var("PANDOC_BIN").unwrap_or_else(|_| "pandoc".to_string());
    let typst_bin = std::env::var("TYPST_BIN").unwrap_or_else(|_| "typst".to_string());

    let is_typst_engine = doc_engine == "typst" || pdf_engine == "typst";
    let has_pandoc = binary_available(&pandoc_bin);
    let has_typst = binary_available(&typst_bin);

    let output = if is_typst_engine && !has_pandoc && has_typst {
        // Direct typst compile
        let mut cmd = Command::new(&typst_bin);
        cmd.current_dir(&temp_dir_path)
            .env("HOME", &temp_dir_path)
            .env("XDG_DATA_HOME", &temp_dir_path)
            .env("XDG_CACHE_HOME", &temp_dir_path)
            .arg("compile")
            .arg(&input_typ_path)
            .arg(&output_path);
        cmd.output().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to execute typst binary: {}", e),
            )
        })?
    } else if is_typst_engine && has_pandoc {
        // Pandoc with typst engine
        let mut cmd = Command::new(&pandoc_bin);
        cmd.current_dir(&temp_dir_path)
            .env("HOME", &temp_dir_path)
            .env("XDG_DATA_HOME", &temp_dir_path)
            .env("XDG_CACHE_HOME", &temp_dir_path)
            .arg(&input_md_path)
            .arg("-o")
            .arg(&output_path)
            .arg("--pdf-engine=typst");

        if let Ok(font) = std::env::var("PANDOC_MAIN_FONT") {
            cmd.arg("-V").arg(format!("mainfont={}", font));
        }

        let pandoc_res = cmd.output().await;
        let should_fallback = match &pandoc_res {
            Ok(out) => !out.status.success() && has_typst,
            Err(_) => has_typst,
        };

        if should_fallback {
            // First convert markdown to typst format using pandoc -t typst if possible
            let _ = Command::new(&pandoc_bin)
                .current_dir(&temp_dir_path)
                .env("HOME", &temp_dir_path)
                .arg(&input_md_path)
                .arg("-t")
                .arg("typst")
                .arg("-o")
                .arg(&input_typ_path)
                .output()
                .await;

            let mut typst_cmd = Command::new(&typst_bin);
            typst_cmd
                .current_dir(&temp_dir_path)
                .env("HOME", &temp_dir_path)
                .arg("compile")
                .arg(&input_typ_path)
                .arg(&output_path);
            typst_cmd.output().await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to execute fallback typst binary: {}", e),
                )
            })?
        } else {
            pandoc_res.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to execute pandoc with typst engine: {}", e),
                )
            })?
        }
    } else if has_pandoc {
        // Pandoc with LaTeX (default)
        let mut cmd = Command::new(&pandoc_bin);
        cmd.current_dir(&temp_dir_path)
            .env("HOME", &temp_dir_path)
            .env("XDG_DATA_HOME", &temp_dir_path)
            .env("XDG_CACHE_HOME", &temp_dir_path)
            .arg(&input_md_path)
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

        if let Ok(font) = std::env::var("PANDOC_MAIN_FONT") {
            cmd.arg("-V").arg(format!("mainfont={}", font));
        }

        cmd.output().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Failed to execute pandoc binary (is pandoc installed?): {}",
                    e
                ),
            )
        })?
    } else if has_typst {
        // Fallback to typst if pandoc is missing
        let mut cmd = Command::new(&typst_bin);
        cmd.current_dir(&temp_dir_path)
            .env("HOME", &temp_dir_path)
            .arg("compile")
            .arg(&input_typ_path)
            .arg(&output_path);
        cmd.output().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to execute typst binary: {}", e),
            )
        })?
    } else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Neither pandoc nor typst binary is available on the host".to_string(),
        ));
    };

    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Document conversion failed: {}", stderr_str),
        ));
    }

    let pdf_bytes = match fs::read(&output_path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read generated PDF: {}", e),
            ));
        }
    };

    // Explicit early cleanup before returning
    if temp_dir_path.exists() {
        let _ = fs::remove_dir_all(&temp_dir_path).await;
    }

    Ok(pdf_bytes)
}

async fn execute_pandoc_conversion(
    markdown: &str,
    output_filename: &str,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    let pdf_bytes = generate_pandoc_pdf_bytes(markdown, output_filename)
        .await
        .map_err(|(status, msg)| {
            (
                status,
                Json(ApiResponse {
                    success: false,
                    message: Some(msg),
                    data: None,
                }),
            )
        })?;

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    let disposition = format!("attachment; filename=\"{}\"", output_filename);
    if let Ok(val) = HeaderValue::from_str(&disposition) {
        headers.insert(CONTENT_DISPOSITION, val);
    }

    Ok((headers, pdf_bytes).into_response())
}

pub async fn download_pandoc_ephemeral(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    let download = state.take_ephemeral_download(&token).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                message: Some("Download token is invalid or has expired".to_string()),
                data: None,
            }),
        )
    })?;

    let mut headers = HeaderMap::new();
    let content_type = if let Ok(val) = HeaderValue::from_str(&download.content_type) {
        val
    } else {
        HeaderValue::from_static("application/pdf")
    };
    headers.insert(CONTENT_TYPE, content_type);

    let safe_filename = download.filename.replace(['"', '\r', '\n'], "");
    let disposition = format!("attachment; filename=\"{}\"", safe_filename);
    if let Ok(val) = HeaderValue::from_str(&disposition) {
        headers.insert(CONTENT_DISPOSITION, val);
    }

    Ok((headers, download.data).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_download_pandoc_ephemeral_lifecycle() {
        let state = AppState::new();
        let pdf_data = b"%PDF-1.4 test pdf content".to_vec();
        let token = state
            .store_ephemeral_download(
                pdf_data.clone(),
                "test-report.pdf".to_string(),
                "application/pdf".to_string(),
                300,
            )
            .await;

        // 1. First download succeeds
        let res = download_pandoc_ephemeral(Path(token.clone()), State(state.clone())).await;
        assert!(res.is_ok());
        let response = res.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/pdf"
        );
        assert_eq!(
            response.headers().get(CONTENT_DISPOSITION).unwrap(),
            "attachment; filename=\"test-report.pdf\""
        );

        // 2. Second download with the same token fails (404 NOT FOUND, burn-after-reading)
        let res2 = download_pandoc_ephemeral(Path(token.clone()), State(state.clone())).await;
        assert!(res2.is_err());
        let (status, body) = res2.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body.0.success, false);

        // 3. Nonexistent token returns 404
        let res3 = download_pandoc_ephemeral(
            Path("non-existent-token".to_string()),
            State(state.clone()),
        )
        .await;
        assert!(res3.is_err());
        assert_eq!(res3.unwrap_err().0, StatusCode::NOT_FOUND);
    }
}

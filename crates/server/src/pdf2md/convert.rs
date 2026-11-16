//! Orchestration of one PDF to Markdown conversion: validation, job directory,
//! extraction, structure building and response shaping.
//!
//! Order of checks follows Requirement 2.12: 413, 415, 400, 422, 504.
//! Nothing here logs file names, content, paths or subprocess output.

use super::engine::ExtractRequest;
use super::error::ConvertError;
use super::options::parse_options;
use super::proc::Deadline;
use super::structure::{self, BuildOptions};
use super::PdfRuntime;
use crate::routes::pandoc::TempDirGuard;
use disposable_common::{PdfConversionResult, PdfMcpResult};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// MCP results longer than this many Unicode characters are not inlined.
pub const MCP_INLINE_MAX_CHARS: usize = 100_000;
/// Number of leading characters returned as preview for truncated MCP results.
pub const MCP_PREVIEW_CHARS: usize = 2_000;

pub struct ConvertInput {
    pub file: Vec<u8>,
    pub options_raw: Option<Vec<u8>>,
    /// Number of multipart `file` parts beyond the first (ignored with a warning).
    pub extra_file_parts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    pub markdown: String,
    pub pages: u32,
    pub headings: u32,
    pub warnings: Vec<String>,
}

impl From<&Conversion> for PdfConversionResult {
    fn from(c: &Conversion) -> Self {
        PdfConversionResult {
            markdown: c.markdown.clone(),
            pages: c.pages,
            headings: c.headings,
            warnings: c.warnings.clone(),
            download_token: None,
        }
    }
}

/// Creates `<root>/pdf2md-job-<uuid>` wrapped in a guard that removes it on drop.
/// The guard is built before the directory so partial creation is cleaned up too.
pub fn make_job_dir(root: &Path) -> std::io::Result<TempDirGuard> {
    let guard = TempDirGuard {
        path: root.join(format!("pdf2md-job-{}", Uuid::new_v4())),
    };
    std::fs::create_dir_all(&guard.path)?;
    Ok(guard)
}

/// Serialized size of the response body, used for the `max_response_bytes` check.
fn response_len(c: &Conversion) -> u64 {
    serde_json::to_vec(&PdfConversionResult::from(c))
        .map(|v| v.len() as u64)
        .unwrap_or(u64::MAX)
}

pub async fn convert(rt: &PdfRuntime, input: ConvertInput) -> Result<Conversion, ConvertError> {
    let ConvertInput { file, options_raw, extra_file_parts } = input;

    // 413
    if file.len() as u64 > rt.config.max_bytes {
        return Err(ConvertError::FileTooLarge { max_bytes: rt.config.max_bytes });
    }
    // 415 (also for empty input)
    if !file.starts_with(b"%PDF-") {
        return Err(ConvertError::NotAPdf);
    }
    // 400
    let options = parse_options(options_raw.as_deref()).map_err(|e| ConvertError::BadOptions {
        field: e.field,
        reason: e.reason.to_string(),
    })?;

    // Job directory; removed when `guard` drops, on every exit path.
    let guard = make_job_dir(&rt.job_root).map_err(|_| ConvertError::Internal)?;
    let pdf_path = guard.path.join("input.pdf");
    {
        let mut f = tokio::fs::File::create(&pdf_path)
            .await
            .map_err(|_| ConvertError::Internal)?;
        f.write_all(&file).await.map_err(|_| ConvertError::Internal)?;
        f.flush().await.map_err(|_| ConvertError::Internal)?;
        drop(f);
    }
    drop(file);

    let deadline = Deadline::new(rt.config.subprocess_timeout);
    let doc = rt
        .engine
        .extract(&ExtractRequest {
            pdf_path: &pdf_path,
            work_dir: &guard.path,
            page_from: options.page_from,
            page_to: options.page_to,
            max_pages: rt.config.max_pages,
            deadline,
            max_output_bytes: rt.config.max_output_bytes,
        })
        .await
        .map_err(ConvertError::from)?;

    let has_text = doc
        .pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .any(|l| l.spans.iter().any(|s| s.text.chars().any(|c| !c.is_whitespace())));
    if !has_text {
        return Err(ConvertError::NoTextLayer);
    }

    let mut warnings = Vec::new();
    if extra_file_parts > 0 {
        warnings.push("File tambahan diabaikan; hanya file pertama diproses.".to_string());
    }
    if let Some(req_to) = options.page_to {
        if req_to > doc.last_page {
            warnings.push(format!("page_to disesuaikan menjadi {}", doc.last_page));
        }
    }

    let build_opts = BuildOptions {
        page_breaks: options.page_breaks,
        strip_headers_footers: options.strip_headers_footers,
    };
    let pages = doc.pages.len() as u32;
    let out = tokio::task::spawn_blocking(move || structure::build(&doc, &build_opts))
        .await
        .map_err(|_| ConvertError::Internal)?;

    if out.word_count == 0 {
        return Err(ConvertError::NoTextLayer);
    }
    warnings.extend(out.warnings);

    let result = Conversion { markdown: out.markdown, pages, headings: out.headings, warnings };
    if let Some(cap) = rt.config.max_response_bytes {
        if response_len(&result) > cap {
            return Err(ConvertError::OutputTooLarge);
        }
    }
    Ok(result)
}

/// Shapes a conversion for MCP. `download_url` is only called when the result
/// is truncated, so callers store an Ephemeral_Download only in that case.
pub fn shape_mcp_result(
    c: &Conversion,
    download_url: impl FnOnce() -> Option<String>,
) -> PdfMcpResult {
    let truncated = c.markdown.chars().count() > MCP_INLINE_MAX_CHARS;
    if truncated {
        PdfMcpResult {
            markdown: None,
            preview: Some(c.markdown.chars().take(MCP_PREVIEW_CHARS).collect()),
            download_url: download_url(),
            pages: c.pages,
            headings: c.headings,
            warnings: c.warnings.clone(),
            truncated: true,
        }
    } else {
        PdfMcpResult {
            markdown: Some(c.markdown.clone()),
            preview: None,
            download_url: None,
            pages: c.pages,
            headings: c.headings,
            warnings: c.warnings.clone(),
            truncated: false,
        }
    }
}

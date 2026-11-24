//! Unit tests for `PopplerEngine`. The first test needs no poppler; the rest use
//! `skip_unless_tool!` and run real `pdfinfo`/`pdftohtml`.
//! Requirements: 7.7, 8.1, 8.7, 16.1, 16.12.
use super::gen_pdf::{
    corrupt_pdf, encrypted_pdf, mixed_font_pdf, scanned_like_pdf, text_pdf,
};
use super::helpers::{pdf_bytes, skip_unless_tool, tmp_listing_of};
use crate::pdf2md::config::PdfConfig;
use crate::pdf2md::convert::{convert, ConvertInput};
use crate::pdf2md::engine::{ExtractRequest, ExtractionEngine};
use crate::pdf2md::error::{ConvertError, EngineError};
use crate::pdf2md::model::IntermediateDoc;
use crate::pdf2md::poppler::PopplerEngine;
use crate::pdf2md::proc::Deadline;
use crate::pdf2md::PdfRuntime;
use std::sync::Arc;
use std::time::Duration;

/// Writes `bytes` to `<tmp>/input.pdf` and runs the real engine on it.
async fn extract(
    bytes: &[u8],
    page_from: u32,
    page_to: Option<u32>,
) -> Result<IntermediateDoc, EngineError> {
    let dir = tempfile::tempdir().unwrap();
    let pdf = dir.path().join("input.pdf");
    std::fs::write(&pdf, bytes).unwrap();
    let req = ExtractRequest {
        pdf_path: &pdf,
        work_dir: dir.path(),
        page_from,
        page_to,
        max_pages: 300,
        deadline: Deadline::new(Duration::from_secs(30)),
        max_output_bytes: 10 * 1024 * 1024,
    };
    PopplerEngine::default().extract(&req).await
}

fn all_text(doc: &IntermediateDoc) -> String {
    doc.pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| l.text())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn missing_binaries_give_engine_unavailable_and_clean_job_dir() {
    let root = tempfile::tempdir().unwrap();
    let engine: Arc<dyn ExtractionEngine> =
        Arc::new(PopplerEngine::with_binaries("tidak-ada-xyz", "tidak-ada-xyz"));
    let rt = PdfRuntime {
        config: PdfConfig::defaults(),
        engine,
        job_root: root.path().to_path_buf(),
    };
    let err = convert(
        &rt,
        ConvertInput { file: pdf_bytes(64), options_raw: None, extra_file_parts: 0 },
    )
    .await
    .expect_err("must fail");
    assert_eq!(err, ConvertError::EngineUnavailable);
    assert_eq!(err.http_status().as_u16(), 503);
    assert_eq!(err.code_str(), "engine_unavailable");
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn real_pdf_is_extracted() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    let doc = extract(&text_pdf(3, "Running Header", "Running Footer"), 1, None)
        .await
        .expect("extraction succeeds");
    assert_eq!((doc.total_pages, doc.first_page, doc.last_page), (3, 1, 3));
    assert_eq!(doc.pages.len(), 3);
    assert!(doc.invariant_holds());
    let text = all_text(&doc);
    for needle in ["Heading 1", "Body line one of page 2", "Body line two of page 3", "Running Header"] {
        assert!(text.contains(needle), "missing {needle:?} in {text:?}");
    }
}

#[tokio::test]
async fn real_pdf_reports_font_attributes() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    let doc = extract(&mixed_font_pdf(), 1, None).await.expect("extraction succeeds");
    let lines: Vec<_> = doc.pages.iter().flat_map(|p| p.lines.iter()).collect();
    let title = lines.iter().find(|l| l.text().contains("Main Title")).expect("title line");
    let code = lines.iter().find(|l| l.text().contains("fn main")).expect("code line");
    let body = lines.iter().find(|l| l.text().contains("Plain paragraph")).expect("body line");
    assert!(code.mono && !body.mono);
    assert!(title.size > body.size);
}

#[tokio::test]
async fn encrypted_pdf_is_reported_as_encrypted() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    let r = extract(&encrypted_pdf(), 1, None).await;
    assert_eq!(r.unwrap_err(), EngineError::Encrypted);
}

#[tokio::test]
async fn truncated_pdf_is_reported_as_corrupt() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    let r = extract(&corrupt_pdf(), 1, None).await;
    assert_eq!(r.unwrap_err(), EngineError::Corrupt);
}

#[tokio::test]
async fn pdf_without_text_gives_no_text_layer() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    let root = tempfile::tempdir().unwrap();
    let rt = PdfRuntime {
        config: PdfConfig::defaults(),
        engine: Arc::new(PopplerEngine::default()),
        job_root: root.path().to_path_buf(),
    };
    let err = convert(
        &rt,
        ConvertInput { file: scanned_like_pdf(), options_raw: None, extra_file_parts: 0 },
    )
    .await
    .expect_err("must fail");
    assert_eq!(err, ConvertError::NoTextLayer);
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn page_range_limits_extracted_pages() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    let pdf = text_pdf(4, "H", "F");
    let doc = extract(&pdf, 2, Some(3)).await.expect("extraction succeeds");
    assert_eq!((doc.total_pages, doc.first_page, doc.last_page), (4, 2, 3));
    assert_eq!(doc.pages.iter().map(|p| p.number).collect::<Vec<_>>(), vec![2, 3]);
    let text = all_text(&doc);
    assert!(text.contains("Heading 2") && text.contains("Heading 3"));
    assert!(!text.contains("Heading 1") && !text.contains("Heading 4"));

    // page_to beyond the end is clamped to the last page.
    let doc = extract(&pdf, 3, Some(99)).await.expect("extraction succeeds");
    assert_eq!((doc.first_page, doc.last_page), (3, 4));
}

#[tokio::test]
async fn page_from_beyond_total_is_out_of_range() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    let r = extract(&text_pdf(3, "H", "F"), 5, None).await;
    assert_eq!(r.unwrap_err(), EngineError::PageFromOutOfRange { total_pages: 3 });
}

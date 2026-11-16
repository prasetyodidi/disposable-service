//! Unit tests for `convert` error mapping with `MockEngine`. No poppler needed.
//! Requirements: 2.4, 2.5, 2.6, 2.7, 2.8, 8.5, 8.6, 16.1.
use super::helpers::{pdf_bytes, tmp_listing_of};
use super::mock_engine::MockEngine;
use crate::pdf2md::config::PdfConfig;
use crate::pdf2md::convert::{convert, ConvertInput};
use crate::pdf2md::engine::ExtractionEngine;
use crate::pdf2md::error::{ConvertError, EngineError};
use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::PdfRuntime;
use std::sync::Arc;

fn doc(pages: u32) -> IntermediateDoc {
    let pages_v: Vec<Page> = (1..=pages)
        .map(|n| Page {
            number: n,
            width: 918,
            height: 1188,
            lines: vec![TextLine::new(
                vec![Span { text: format!("halo dunia {n}"), bold: false, italic: false, mono: false, size: 12 }],
                12,
                50,
                200,
                100,
                16,
            )],
        })
        .collect();
    IntermediateDoc { total_pages: pages, first_page: 1, last_page: pages, pages: pages_v }
}

fn runtime(engine: Arc<MockEngine>, root: &std::path::Path) -> PdfRuntime {
    let engine: Arc<dyn ExtractionEngine> = engine;
    PdfRuntime { config: PdfConfig::defaults(), engine, job_root: root.to_path_buf() }
}

fn input() -> ConvertInput {
    ConvertInput { file: pdf_bytes(64), options_raw: None, extra_file_parts: 0 }
}

async fn run_err(e: EngineError) -> (ConvertError, bool) {
    let root = tempfile::tempdir().unwrap();
    let rt = runtime(MockEngine::err(e), root.path());
    let r = convert(&rt, input()).await;
    let clean = tmp_listing_of(root.path()).is_empty();
    (r.expect_err("must fail"), clean)
}

async fn expect(e: EngineError, status: u16, code: &str) {
    let (err, clean) = run_err(e.clone()).await;
    assert_eq!(err.http_status().as_u16(), status, "{e:?}");
    assert_eq!(err.code_str(), code, "{e:?}");
    assert!(clean, "job directory left behind for {e:?}");
}

#[tokio::test]
async fn corrupt_maps_to_422_corrupt_pdf() {
    expect(EngineError::Corrupt, 422, "corrupt_pdf").await;
}

#[tokio::test]
async fn encrypted_maps_to_422_encrypted_pdf() {
    expect(EngineError::Encrypted, 422, "encrypted_pdf").await;
}

#[tokio::test]
async fn no_text_layer_maps_to_422() {
    expect(EngineError::NoTextLayer, 422, "no_text_layer").await;
}

#[tokio::test]
async fn timeout_maps_to_504() {
    expect(EngineError::Timeout, 504, "timeout").await;
}

#[tokio::test]
async fn too_many_pages_maps_to_422() {
    expect(EngineError::TooManyPages, 422, "too_many_pages").await;
}

#[tokio::test]
async fn unavailable_maps_to_503() {
    expect(EngineError::Unavailable, 503, "engine_unavailable").await;
}

#[tokio::test]
async fn output_too_large_maps_to_422() {
    expect(EngineError::OutputTooLarge, 422, "output_too_large").await;
}

#[tokio::test]
async fn page_from_out_of_range_maps_to_400_with_total() {
    let (err, clean) = run_err(EngineError::PageFromOutOfRange { total_pages: 7 }).await;
    assert_eq!(err, ConvertError::PageFromOutOfRange { total_pages: 7 });
    assert_eq!(err.http_status().as_u16(), 400);
    assert_eq!(err.code_str(), "page_from_out_of_range");
    assert!(err.message().contains('7'));
    assert!(clean);
}

#[tokio::test]
async fn exactly_300_pages_is_accepted() {
    let root = tempfile::tempdir().unwrap();
    let rt = runtime(MockEngine::doc(doc(300)), root.path());
    let c = convert(&rt, input()).await.expect("300 pages accepted");
    assert_eq!(c.pages, 300);
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn page_301_is_too_many_pages() {
    // The engine enforces `max_pages` (300); the mock reports it for a 301-page request.
    let root = tempfile::tempdir().unwrap();
    assert_eq!(PdfConfig::defaults().max_pages, 300);
    let rt = runtime(MockEngine::err(EngineError::TooManyPages), root.path());
    let err = convert(&rt, input()).await.unwrap_err();
    assert_eq!(err, ConvertError::TooManyPages);
    assert_eq!(err.http_status().as_u16(), 422);
    assert_eq!(err.code_str(), "too_many_pages");
    assert!(err.message().contains("300"));
}

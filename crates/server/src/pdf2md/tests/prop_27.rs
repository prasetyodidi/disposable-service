//! Property 27: Round trip Ephemeral_Download.
//!
//! A conversion with `download=true` yields a token. The first download returns the exact
//! UTF-8 bytes of the `markdown` field with the fixed headers; the second returns 404.

// Feature: pdf-to-markdown, Property 27: Round trip Ephemeral_Download
// **Validates: Requirements 10.1, 10.4, 10.10, 10.11**

use axum::http::{header, StatusCode};
use proptest::prelude::*;

use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::tests::gen_doc::arb_intermediate_doc;
use crate::pdf2md::tests::helpers::{authed_pdf_app, pdf_bytes, test_runtime};
use crate::pdf2md::tests::mock_engine::MockEngine;
use crate::test_support::http::{get, post_multipart_file, send};

const URI: &str = "/api/apps/pdf-to-markdown";

fn unicode_doc(text: String) -> IntermediateDoc {
    let span = Span { text, bold: false, italic: false, mono: false, size: 12 };
    let line = TextLine::new(vec![span], 12, 80, 150, 400, 16);
    IntermediateDoc {
        total_pages: 1,
        first_page: 1,
        last_page: 1,
        pages: vec![Page { number: 1, width: 918, height: 1188, lines: vec![line] }],
    }
}

fn arb_doc() -> impl Strategy<Value = IntermediateDoc> {
    prop_oneof![
        arb_intermediate_doc(),
        "[\\p{L}\\p{N}éüñ中文日本語😀 ]{1,40}".prop_filter("non-blank", |s| !s.trim().is_empty()).prop_map(unicode_doc),
    ]
}

fn run_case(doc: IntermediateDoc) -> Result<(), TestCaseError> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let job_root = tempfile::tempdir().unwrap();
    rt.block_on(async {
        let (app, _state, token) = authed_pdf_app(test_runtime(MockEngine::doc(doc), job_root.path())).await;
        let uri = format!("{URI}?download=true");
        let (status, _h, body) = send(&app, post_multipart_file(&uri, &token, "a.pdf", &pdf_bytes(64))).await;
        if status != StatusCode::OK {
            // Documents without extractable text are rejected (422); no token exists then.
            prop_assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            return Ok(());
        }
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let markdown = json["markdown"].as_str().unwrap().to_string();
        let dl = json["download_token"].as_str().expect("download_token present").to_string();

        let dl_uri = format!("{URI}/download/{dl}");
        let (s1, h1, b1) = send(&app, get(&dl_uri, Some(&token))).await;
        prop_assert_eq!(s1, StatusCode::OK);
        prop_assert_eq!(b1, markdown.as_bytes().to_vec());
        prop_assert_eq!(h1.get(header::CONTENT_TYPE).unwrap(), "text/markdown; charset=utf-8");
        let cd = h1.get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap().to_string();
        prop_assert!(cd.starts_with("attachment") && cd.contains("document.md"), "cd = {}", cd);
        prop_assert_eq!(h1.get(header::CACHE_CONTROL).unwrap(), "no-store");

        let (s2, _h2, _b2) = send(&app, get(&dl_uri, Some(&token))).await;
        prop_assert_eq!(s2, StatusCode::NOT_FOUND);
        Ok(())
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

    #[test]
    fn prop_ephemeral_download_round_trip(doc in arb_doc()) {
        run_case(doc)?;
    }
}

// Feature: pdf-to-markdown, Property 4: Masukan non-PDF ditolak tanpa menjalankan mesin
// **Validates: Requirements 2.3, 16.5**
use proptest::prelude::*;

use super::helpers::{authed_pdf_app, test_runtime, tmp_listing_of};
use super::mock_engine::MockEngine;
use crate::pdf2md::model::IntermediateDoc;
use crate::test_support::http::{post_multipart_file, send};

/// 0..=4096 arbitrary bytes that do not start with `%PDF-`. Near-miss prefixes
/// are mixed in so the boundary of the signature check is exercised.
fn arb_non_pdf() -> impl Strategy<Value = Vec<u8>> {
    let near_miss = (
        prop_oneof![
            Just(b"%PDF".to_vec()),
            Just(b"%pdf-".to_vec()),
            Just(b" %PDF-".to_vec()),
            Just(b"%PD".to_vec()),
            Just(b"%PDF/".to_vec()),
            Just(b"\xEF\xBB\xBF%PDF-".to_vec()),
        ],
        prop::collection::vec(any::<u8>(), 0..=64),
    )
        .prop_map(|(mut p, rest)| {
            p.extend(rest);
            p
        });
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..=4096),
        near_miss,
    ]
    .prop_filter("must not start with %PDF-", |b| !b.starts_with(b"%PDF-"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn non_pdf_rejected_without_engine(bytes in arb_non_pdf()) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let job_root = tempfile::tempdir().unwrap();
            let engine = MockEngine::doc(IntermediateDoc::default());
            let runtime = test_runtime(engine.clone(), job_root.path());
            let (app, _state, token) = authed_pdf_app(runtime).await;

            let before = tmp_listing_of(job_root.path());
            let req = post_multipart_file("/api/apps/pdf-to-markdown", &token, "x.pdf", &bytes);
            let (status, _h, body) = send(&app, req).await;

            prop_assert_eq!(status.as_u16(), 415);
            let v: serde_json::Value = serde_json::from_slice(&body).expect("json body");
            prop_assert_eq!(v["code"].as_str(), Some("not_a_pdf"));
            prop_assert_eq!(engine.calls(), 0);
            prop_assert_eq!(tmp_listing_of(job_root.path()), before);
            Ok(())
        })?;
    }
}

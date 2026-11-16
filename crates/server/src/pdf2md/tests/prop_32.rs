//! Property 32: shape of `Conversion_Result`.
//!
//! Feature: pdf-to-markdown, Property 32: for any valid `IntermediateDoc` with text, `convert`
//! returns non-empty `markdown`, `1 <= pages <= 300`, `headings` equal to the number of lines
//! starting with `#`, and `warnings` that do not contain document text. Uses `MockEngine`, so no
//! poppler is needed.
//!
//! **Validates: Requirements 1.1**
use super::gen_doc::arb_intermediate_doc;
use super::mock_engine::MockEngine;
use crate::pdf2md::config::PdfConfig;
use crate::pdf2md::convert::{convert, ConvertInput};
use crate::pdf2md::model::IntermediateDoc;
use crate::pdf2md::PdfRuntime;
use proptest::prelude::*;

fn has_text(doc: &IntermediateDoc) -> bool {
    doc.pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .any(|l| l.spans.iter().any(|s| s.text.chars().any(|c| !c.is_whitespace())))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn conversion_result_has_expected_shape(doc in arb_intermediate_doc(), strip in any::<bool>()) {
        prop_assume!(has_text(&doc));
        let texts: Vec<String> = doc
            .pages
            .iter()
            .flat_map(|p| p.lines.iter())
            .flat_map(|l| l.spans.iter().map(|s| s.text.clone()))
            .collect();
        let expected_pages = doc.pages.len() as u32;
        let root = tempfile::tempdir().unwrap();
        let rt = PdfRuntime {
            config: PdfConfig::defaults(),
            engine: MockEngine::doc(doc),
            job_root: root.path().to_path_buf(),
        };
        let opts = format!("{{\"strip_headers_footers\":{strip}}}");
        let tokio_rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        std::fs::create_dir_all(&rt.job_root).unwrap();
        let result = tokio_rt.block_on(convert(
            &rt,
            ConvertInput {
                file: super::helpers::pdf_bytes(64),
                options_raw: Some(opts.into_bytes()),
                extra_file_parts: 0,
            },
        ));
        // Stripping may legitimately remove all text (e.g. only header/footer lines): that is
        // reported as an error, not a Conversion_Result.
        let c = match result {
            Ok(c) => c,
            Err(e) => {
                prop_assert!(strip, "unexpected error without stripping: {:?}", e);
                return Ok(());
            }
        };

        prop_assert!(!c.markdown.trim().is_empty());
        prop_assert!((1..=300).contains(&c.pages));
        prop_assert_eq!(c.pages, expected_pages);
        let heading_lines = c.markdown.lines().filter(|l| l.starts_with('#')).count() as u32;
        prop_assert_eq!(c.headings, heading_lines);
        for w in &c.warnings {
            let digits = w.chars().take_while(char::is_ascii_digit).count();
            prop_assert!(digits > 0 && &w[digits..] == " baris header/footer dihapus", "warning: {}", w);
            prop_assert!(!texts.iter().any(|t| t.chars().count() > 3 && w.contains(t.as_str())));
        }
    }
}

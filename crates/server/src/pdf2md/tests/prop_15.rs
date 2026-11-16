// Feature: pdf-to-markdown, Property 15: Paragraf menurut jarak vertikal
//! **Validates: Requirements 3.3, 3.4**
use proptest::prelude::*;

use super::gen_doc::{arb_paragraph_gaps, line_height, BODY_SIZE};
use crate::pdf2md::structure::{build, BuildOptions};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn paragraphs_follow_vertical_gaps(g in arb_paragraph_gaps()) {
        let h = line_height(BODY_SIZE) as i64;
        let expected = 1 + g.pitches.iter().filter(|&&p| 2 * (p as i64) > 3 * h).count();

        let out = build(
            &g.doc,
            &BuildOptions { page_breaks: false, strip_headers_footers: false },
        );
        let md = out.markdown;
        prop_assert!(md.ends_with('\n') && !md.ends_with("\n\n"));
        prop_assert!(!md.contains("\n\n\n"));
        let body = md.trim_end_matches('\n');
        let paragraphs: Vec<&str> = body.split("\n\n").collect();
        prop_assert_eq!(paragraphs.len(), expected, "markdown: {:?}, pitches: {:?}", md, g.pitches);
        // Each paragraph is a single line of text (lines inside one are joined by a space).
        prop_assert!(paragraphs.iter().all(|p| !p.trim().is_empty() && !p.contains('\n')));
    }
}

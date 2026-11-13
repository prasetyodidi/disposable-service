//! Property 4: PDF scrubbing keeps structure and content. Validates Requirements 2.1, 2.2.

use super::gen_scrub::arb_pdf_spec;
use crate::privacy::scrubber::scrub;
use lopdf::Document;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: privacy-micro-apps-and-burn-workspace, Property 4: Scrub PDF mempertahankan struktur dan konten
    // **Validates: Requirements 2.1, 2.2**
    #[test]
    fn scrub_pdf_preserves_structure_and_content(spec in arb_pdf_spec()) {
        let input = spec.build();
        let out = scrub(&input).expect("valid synthetic PDF must scrub");

        let before = Document::load_mem(&input).expect("input loads");
        let after = Document::load_mem(&out.bytes).expect("output must load with lopdf");

        // Same page count.
        let pages_before = before.get_pages();
        let pages_after = after.get_pages();
        prop_assert_eq!(pages_before.len(), spec.pages.len());
        prop_assert_eq!(pages_after.len(), pages_before.len());

        // Same extracted text per page.
        for n in pages_before.keys() {
            let a = before.extract_text(&[*n]).unwrap_or_default();
            let b = after.extract_text(&[*n]).unwrap_or_default();
            prop_assert_eq!(a, b, "text differs on page {}", n);
        }

        // No Info dictionary and no XMP streams remain.
        prop_assert!(after.trailer.get(b"Info").is_err(), "trailer still has /Info");
        let catalog = after.catalog().expect("catalog");
        prop_assert!(catalog.get(b"Metadata").is_err(), "catalog still has /Metadata");
        for id in pages_after.values() {
            let page = after.get_dictionary(*id).expect("page dict");
            prop_assert!(page.get(b"Metadata").is_err(), "page still has /Metadata");
        }
        for obj in after.objects.values() {
            if let Ok(s) = obj.as_stream() {
                prop_assert!(
                    s.dict.get(b"Type").ok().and_then(|t| t.as_name().ok()) != Some(b"Metadata".as_slice()),
                    "XMP metadata stream still present"
                );
            }
        }
        for v in spec.info.iter().map(|(_, v)| v) {
            prop_assert!(!out.bytes.windows(v.len()).any(|w| w == v.as_bytes()), "leaked {}", v);
        }
    }
}

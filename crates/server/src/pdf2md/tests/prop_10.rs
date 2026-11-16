// Feature: pdf-to-markdown, Property 10: Round trip penguraian XML pdftohtml
// **Validates: Requirements 6.4, 6.5, 8.1**
use proptest::prelude::*;
use super::gen_doc::arb_intermediate_doc;
use super::gen_xml::to_pdftohtml_xml;
use crate::pdf2md::xml::parse_pdftohtml_xml;
proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
    /// Parsing the serialized document yields the same text, styles, coordinates, sizes and mono flags.
    #[test]
    fn xml_round_trip_preserves_document(doc in arb_intermediate_doc()) {
        let parsed = parse_pdftohtml_xml(&to_pdftohtml_xml(&doc)).unwrap();
        prop_assert_eq!(parsed.pages.len(), doc.pages.len());
        for (p, q) in parsed.pages.iter().zip(&doc.pages) {
            prop_assert_eq!(p.number, q.number);
            prop_assert_eq!(p.lines.len(), q.lines.len());
            for (a, b) in p.lines.iter().zip(&q.lines) {
                prop_assert_eq!(&a.spans, &b.spans);
                prop_assert_eq!((a.x, a.y, a.w, a.h, a.size), (b.x, b.y, b.w, b.h, b.size));
                prop_assert_eq!(a.mono, b.mono);
            }
        }
        prop_assert_eq!(parsed, doc);
    }
}

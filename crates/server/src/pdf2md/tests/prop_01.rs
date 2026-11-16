// Feature: pdf-to-markdown, Property 1: Opsi valid diterapkan dan bawaan diisi
// **Validates: Requirements 1.2, 1.3**
use proptest::prelude::*;

use super::gen_options::arb_valid_options;
use crate::pdf2md::options::{parse_options, ConversionOptions};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Given fields are applied verbatim; omitted fields take defaults.
    #[test]
    fn valid_options_applied_and_defaults_filled(v in arb_valid_options()) {
        let parsed = parse_options(Some(&v.raw)).unwrap();
        prop_assert_eq!(&parsed, &v.expected);

        let obj: serde_json::Value = serde_json::from_slice(&v.raw).unwrap();
        let d = ConversionOptions::default();
        if obj.get("page_from").is_none() { prop_assert_eq!(parsed.page_from, d.page_from); }
        if obj.get("page_to").is_none() { prop_assert_eq!(parsed.page_to, d.page_to); }
        if obj.get("page_breaks").is_none() { prop_assert_eq!(parsed.page_breaks, d.page_breaks); }
        if obj.get("strip_headers_footers").is_none() {
            prop_assert_eq!(parsed.strip_headers_footers, d.strip_headers_footers);
        }
    }
}

/// Without an `options` part the result is all defaults.
#[test]
fn absent_options_yield_defaults() {
    let d = ConversionOptions { page_from: 1, page_to: None, page_breaks: false, strip_headers_footers: true };
    assert_eq!(parse_options(None).unwrap(), d);
    assert_eq!(parse_options(Some(b"")).unwrap(), d);
}

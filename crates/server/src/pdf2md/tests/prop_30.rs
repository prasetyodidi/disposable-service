// Feature: pdf-to-markdown, Property 30: Ambang pemotongan hasil MCP
// **Validates: Requirements 11.3, 11.4**
use proptest::prelude::*;
use std::cell::Cell;

use crate::pdf2md::convert::{shape_mcp_result, Conversion, MCP_INLINE_MAX_CHARS, MCP_PREVIEW_CHARS};

fn arb_len() -> impl Strategy<Value = usize> {
    prop_oneof![
        0usize..3_000,
        (MCP_INLINE_MAX_CHARS - 2)..=(MCP_INLINE_MAX_CHARS + 2),
        (MCP_INLINE_MAX_CHARS + 3)..130_000,
    ]
}

/// A string of exactly `n` Unicode chars (mixed 1 to 4 byte chars), built by cycling a random seed.
fn arb_markdown() -> impl Strategy<Value = String> {
    let ch = prop_oneof![
        proptest::char::range('a', 'z'),
        Just('\n'),
        Just('é'),
        Just('ж'),
        Just('日'),
        Just('😀'),
    ];
    (arb_len(), proptest::collection::vec(ch, 1..40))
        .prop_map(|(n, seed)| seed.iter().cycle().take(n).collect::<String>())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(120))]

    #[test]
    fn mcp_result_truncates_only_above_threshold(md in arb_markdown(), pages in 1u32..50, headings in 0u32..20) {
        let n = md.chars().count();
        let c = Conversion { markdown: md.clone(), pages, headings, warnings: vec!["w".to_string()] };
        let calls = Cell::new(0u32);
        let r = shape_mcp_result(&c, || {
            calls.set(calls.get() + 1);
            Some("/dl/token".to_string())
        });

        prop_assert_eq!(r.truncated, n > 100_000);
        prop_assert_eq!(r.pages, pages);
        prop_assert_eq!(r.headings, headings);
        prop_assert_eq!(&r.warnings, &c.warnings);
        if r.truncated {
            let expected: String = md.chars().take(2_000).collect();
            prop_assert_eq!(r.preview.as_deref(), Some(expected.as_str()));
            prop_assert_eq!(r.preview.as_ref().unwrap().chars().count(), MCP_PREVIEW_CHARS);
            prop_assert!(r.markdown.is_none());
            prop_assert!(r.download_url.is_some());
            prop_assert_eq!(calls.get(), 1);
        } else {
            prop_assert_eq!(r.markdown.as_deref(), Some(md.as_str()));
            prop_assert!(r.download_url.is_none());
            prop_assert!(r.preview.is_none());
            prop_assert_eq!(calls.get(), 0);
        }
    }
}

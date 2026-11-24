// Feature: pdf-to-markdown, Property 18: Round trip escape Markdown
// **Validates: Requirements 3.8**
use proptest::prelude::*;

use crate::pdf2md::structure::inline::{escape_markdown, unescape_markdown};

/// Strings biased toward Markdown-special characters.
fn arb_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            3 => proptest::sample::select(vec!['\\', '`', '*', '_', '[', ']', '#', '>']),
            3 => proptest::char::range('a', 'z'),
            1 => Just(' '),
            1 => proptest::char::range('\u{00A1}', '\u{024F}'),
        ],
        0..40,
    )
    .prop_map(|v| v.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn escape_round_trips_and_escapes_specials(s in arb_text(), at_line_start in any::<bool>()) {
        let escaped = escape_markdown(&s, at_line_start);
        prop_assert_eq!(unescape_markdown(&escaped), s.clone());

        // Walk the output: every backslash introduces an escaped pair, and every
        // special character appears only as the second half of such a pair.
        let chars: Vec<char> = escaped.chars().collect();
        let first_src = s.chars().next();
        let mut i = 0;
        let mut out_pos = 0; // index of the source char being produced
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' {
                prop_assert!(i + 1 < chars.len(), "dangling backslash");
                let n = chars[i + 1];
                prop_assert!(matches!(n, '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '>'));
                if n == '#' || n == '>' {
                    prop_assert!(at_line_start && out_pos == 0, "#/> escaped away from start");
                }
                i += 2;
            } else {
                prop_assert!(!matches!(c, '`' | '*' | '_' | '[' | ']'), "unescaped special {:?}", c);
                if (c == '#' || c == '>') && at_line_start && out_pos == 0 {
                    prop_assert!(false, "#/> at start not escaped");
                }
                i += 1;
            }
            out_pos += 1;
        }
        prop_assert_eq!(out_pos, s.chars().count());
        let _ = first_src;
    }
}

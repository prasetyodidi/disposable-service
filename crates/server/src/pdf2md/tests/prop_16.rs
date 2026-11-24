// Feature: pdf-to-markdown, Property 16: Hyphenation join
//! **Validates: Requirements 3.5**
use proptest::prelude::*;

use crate::pdf2md::model::Span;
use crate::pdf2md::structure::blocks::join_lines;

fn span(text: &str) -> Span {
    Span { text: text.to_string(), bold: false, italic: false, mono: false, size: 10 }
}

fn text_of(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Head of the first line: a stem ending in a letter, digit, or punctuation before the `-`.
fn arb_head() -> impl Strategy<Value = String> {
    (
        "[a-zA-Z]{0,6}",
        prop_oneof![
            4 => "[a-zA-Zé]".prop_map(String::from),
            1 => "[0-9]".prop_map(String::from),
            1 => prop::sample::select(vec![".", ",", ")", "-", "_"]).prop_map(String::from),
        ],
    )
        .prop_map(|(stem, last)| format!("{stem}{last}"))
}

/// Start of the second line: lowercase, uppercase, digit or punctuation, then a tail.
fn arb_tail() -> impl Strategy<Value = String> {
    (
        prop_oneof![
            4 => "[a-zé]".prop_map(String::from),
            2 => "[A-ZÉ]".prop_map(String::from),
            1 => "[0-9]".prop_map(String::from),
            1 => prop::sample::select(vec!["(", "\"", "-"]).prop_map(String::from),
        ],
        "[a-z ]{0,8}",
    )
        .prop_map(|(first, rest)| format!("{first}{rest}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn hyphenation_join_rule(head in arb_head(), tail in arb_tail(), split in any::<bool>()) {
        let first = format!("{head}-");
        // Optionally split the first line over two spans, as a styled run would be.
        let mut prev = if split && first.chars().count() > 1 {
            let cut = first.char_indices().nth(1).map(|(i, _)| i).unwrap();
            vec![span(&first[..cut]), span(&first[cut..])]
        } else {
            vec![span(&first)]
        };

        let last_before = head.chars().last().unwrap();
        let next_first = tail.chars().next().unwrap();
        let expect_join = last_before.is_alphabetic() && next_first.is_lowercase();

        let joined = join_lines(&mut prev, vec![span(&tail)]);
        let out = text_of(&prev);

        prop_assert_eq!(joined, expect_join);
        if expect_join {
            prop_assert_eq!(out, format!("{head}{tail}"));
        } else {
            prop_assert_eq!(out, format!("{head}- {tail}"));
        }
    }
}

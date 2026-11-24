// Feature: pdf-to-markdown, Property 22: Pemisah halaman dan penyambungan antar halaman
// **Validates: Requirements 5.5, 5.6**
use proptest::collection::vec;
use proptest::prelude::*;

use super::gen_doc::arb_intermediate_doc;
use crate::pdf2md::model::Span;
use crate::pdf2md::structure::blocks::{refresh_paragraph, Block, CodeLine, Item};
use crate::pdf2md::structure::{build, join_pages, BuildOptions};

#[derive(Debug, Clone)]
enum Kind {
    Para(String, bool),
    Heading(String),
    List(String),
    Code(String),
}

fn span(text: &str) -> Span {
    Span { text: text.to_string(), bold: false, italic: false, mono: false, size: 12 }
}

fn to_block(k: &Kind) -> Block {
    match k {
        Kind::Para(t, ends) => {
            let text = if *ends { format!("{t}.") } else { t.clone() };
            let mut b = Block::Paragraph { spans: vec![span(&text)], last_line_ends_sentence: false, first_size: 12 };
            refresh_paragraph(&mut b);
            b
        }
        Kind::Heading(t) => Block::Heading { level: 1, spans: vec![span(t)] },
        Kind::List(t) => Block::ListItem { ordered: None, level: 1, spans: vec![span(t)] },
        Kind::Code(t) => Block::Code { lines: vec![CodeLine { indent: 0, text: t.clone() }] },
    }
}

fn arb_kind() -> impl Strategy<Value = Kind> {
    // Letters and spaces only: no hyphenation joins, no sentence enders unless added.
    let text = "[a-z]{1,8}( [a-z]{1,8}){0,2}";
    prop_oneof![
        4 => (text, any::<bool>()).prop_map(|(t, e)| Kind::Para(t, e)),
        1 => text.prop_map(Kind::Heading),
        1 => text.prop_map(Kind::List),
        1 => text.prop_map(Kind::Code),
    ]
}

fn arb_pages() -> impl Strategy<Value = Vec<Vec<Kind>>> {
    vec(vec(arb_kind(), 1..=4), 1..=6)
}

fn is_para(k: &Kind) -> bool {
    matches!(k, Kind::Para(..))
}

/// Text of a block for comparison with the oracle.
fn text_of(b: &Block) -> String {
    match b {
        Block::Paragraph { spans, .. } | Block::Heading { spans, .. } | Block::ListItem { spans, .. } => {
            spans.iter().map(|s| s.text.as_str()).collect()
        }
        Block::Code { lines } => lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>().join("\n"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn page_break_count_is_pages_minus_one(pages in arb_pages()) {
        let blocks: Vec<Vec<Block>> = pages.iter().map(|p| p.iter().map(to_block).collect()).collect();
        let items = join_pages(blocks, true);
        let breaks: Vec<usize> =
            items.iter().enumerate().filter(|(_, i)| **i == Item::PageBreak).map(|(n, _)| n).collect();
        prop_assert_eq!(breaks.len(), pages.len() - 1);
        // Never first or last, never adjacent (each is between content of two pages).
        prop_assert!(!matches!(items.first(), Some(Item::PageBreak)));
        prop_assert!(!matches!(items.last(), Some(Item::PageBreak)));
        // With breaks on, nothing is merged: all blocks survive.
        prop_assert_eq!(items.len(), pages.iter().map(Vec::len).sum::<usize>() + pages.len() - 1);
    }

    #[test]
    fn no_break_and_merge_rule_without_page_breaks(pages in arb_pages()) {
        let blocks: Vec<Vec<Block>> = pages.iter().map(|p| p.iter().map(to_block).collect()).collect();
        let items = join_pages(blocks, false);
        prop_assert!(items.iter().all(|i| *i != Item::PageBreak));

        // Oracle: merge a page's first paragraph into the previous page's trailing
        // paragraph iff that one does not end a sentence. Code+Code merging is excluded by
        // comparing only when no code block meets another across a boundary.
        let boundary_code = pages.windows(2).any(|w| {
            matches!(w[0].last(), Some(Kind::Code(_))) && matches!(w[1].first(), Some(Kind::Code(_)))
        });
        prop_assume!(!boundary_code);
        let mut expected: Vec<(bool, String, bool)> = Vec::new(); // (is_para, text, ends_sentence)
        for page in &pages {
            for (n, k) in page.iter().enumerate() {
                let b = to_block(k);
                let t = text_of(&b);
                let ends = matches!(&b, Block::Paragraph { last_line_ends_sentence: true, .. });
                if n == 0 && is_para(k) {
                    if let Some(last) = expected.last_mut() {
                        if last.0 && !last.2 {
                            last.1 = format!("{} {}", last.1, t);
                            last.2 = ends;
                            continue;
                        }
                    }
                }
                expected.push((is_para(k), t, ends));
            }
        }
        let actual: Vec<(bool, String)> = items
            .iter()
            .map(|i| match i {
                Item::Block(b) => (matches!(b, Block::Paragraph { .. }), text_of(b)),
                Item::PageBreak => unreachable!(),
            })
            .collect();
        let expected: Vec<(bool, String)> = expected.into_iter().map(|(p, t, _)| (p, t)).collect();
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn rendered_separator_lines_match_page_count(doc in arb_intermediate_doc(), breaks in any::<bool>()) {
        prop_assume!(doc.pages.iter().all(|p| !p.lines.is_empty()));
        let out = build(&doc, &BuildOptions { page_breaks: breaks, strip_headers_footers: false });
        let lines: Vec<&str> = out.markdown.lines().collect();
        let seps: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| **l == "---").map(|(i, _)| i).collect();
        let expected = if breaks { doc.pages.len() - 1 } else { 0 };
        prop_assert_eq!(seps.len(), expected);
        for i in seps {
            prop_assert!(i > 0 && i + 1 < lines.len());
            prop_assert!(lines[i - 1].is_empty() && lines[i + 1].is_empty());
        }
    }
}

// Feature: pdf-to-markdown, Property 13: Urutan baca
//!
//! For any single-column page whose text lines sit at random positions and are given in
//! random order, the output word order equals the word order of the lines sorted by
//! `(y, x)` ascending; lines with equal `y` appear left to right.
//!
//! **Validates: Requirements 6.3**
use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::structure::{build, rows, BuildOptions};

const SIZE: u32 = 12;
const H: i32 = 16;

/// A positioned line with its words (letters only, so no Markdown syntax is added).
#[derive(Debug, Clone)]
struct Seg {
    y: i32,
    x: i32,
    words: Vec<String>,
}

fn arb_words() -> impl Strategy<Value = Vec<String>> {
    vec("[a-z]{1,8}", 1..=3)
}

/// Rows at distinct `y` (pitch 40, no vertical merging, one paragraph-free flow), each with
/// 1 to 3 segments at distinct `x`; segments of a row share `y`. Returned in random order.
fn arb_segments() -> impl Strategy<Value = Vec<Seg>> {
    vec(vec(arb_words(), 1..=3), 1..=10)
        .prop_map(|rows| {
            let mut segs = Vec::new();
            for (i, row) in rows.into_iter().enumerate() {
                for (j, words) in row.into_iter().enumerate() {
                    segs.push(Seg { y: 150 + i as i32 * 40, x: 50 + j as i32 * 200, words });
                }
            }
            segs
        })
        .prop_shuffle()
}

fn to_line(s: &Seg) -> TextLine {
    let text = s.words.join(" ");
    let span = Span { text: text.clone(), bold: false, italic: false, mono: false, size: SIZE };
    TextLine::new(vec![span], SIZE, s.x, s.y, text.chars().count() as i32 * 7, H)
}

fn page_of(segs: &[Seg]) -> Page {
    Page { number: 1, width: 918, height: 1188, lines: segs.iter().map(to_line).collect() }
}

fn expected_words(segs: &[Seg]) -> Vec<String> {
    let mut sorted = segs.to_vec();
    sorted.sort_by_key(|s| (s.y, s.x));
    sorted.into_iter().flat_map(|s| s.words).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Full pipeline: output word order equals the `(y, x)`-sorted source word order.
    #[test]
    fn output_words_follow_reading_order(segs in arb_segments(), strip in any::<bool>()) {
        let doc = IntermediateDoc {
            total_pages: 1,
            first_page: 1,
            last_page: 1,
            pages: vec![page_of(&segs)],
        };
        let out = build(&doc, &BuildOptions { page_breaks: false, strip_headers_footers: strip });
        let got: Vec<&str> = out.markdown.split_whitespace().collect();
        let want = expected_words(&segs);
        prop_assert_eq!(got, want.iter().map(String::as_str).collect::<Vec<_>>());
    }

    /// `order_and_merge`: rows come out in ascending `y`, and the words inside a row run left to right.
    #[test]
    fn order_and_merge_sorts_by_y_then_x(segs in arb_segments()) {
        let out = rows::order_and_merge(page_of(&segs));
        prop_assert!(out.lines.windows(2).all(|w| (w[0].y, w[0].x) <= (w[1].y, w[1].x)));
        let got: Vec<String> = out
            .lines
            .iter()
            .flat_map(|l| l.text().split_whitespace().map(String::from).collect::<Vec<_>>())
            .collect();
        prop_assert_eq!(got, expected_words(&segs));
    }
}

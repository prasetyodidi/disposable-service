//! proptest strategies for `IntermediateDoc` (`pdf2md/model.rs`).
//!
//! All coordinates are integers. Generated lines keep every span at the line's font size and
//! a uniform `mono` flag, and adjacent spans always differ in (bold, italic). These two
//! constraints make every generated document survive an XML round trip unchanged
//! (`gen_xml::to_pdftohtml_xml` then `xml::parse_pdftohtml_xml`).
#![allow(dead_code)]
use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};

pub(crate) const BODY_SIZE: u32 = 12;
pub(crate) const HEADING_SIZES: [u32; 3] = [24, 20, 16];
pub(crate) const PAGE_WIDTH: i32 = 918;
pub(crate) const PAGE_HEIGHT: i32 = 1188;
/// Content zone: clear of the header/footer bands used by the stripper.
const CONTENT_TOP: i32 = 150;
const HEADER_Y: i32 = 20;
const FOOTER_Y: i32 = 1150;

/// Line height for a font size.
pub(crate) fn line_height(size: u32) -> i32 {
    size as i32 + 4
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// One word: never empty, no whitespace, and never starting with a list marker character.
fn arb_word() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[a-zA-Z]{1,10}",
        2 => "[a-zA-Z]{1,5}[éèàüöçñÉÜ][a-z]{1,5}",
        2 => "[а-яА-Я]{1,8}",
        2 => "[\u{4e00}-\u{9fa5}]{1,6}",
        // XML-special and Markdown-special characters embedded between letters.
        3 => ("[a-z]{1,4}", prop::sample::select(vec!["&", "<", ">", "\"", "'", "*", "_", "`", "#", "[", "]", "\\", "&amp;", "&lt;", "<b>"]), "[a-z]{1,4}")
            .prop_map(|(a, s, b)| format!("{a}{s}{b}")),
    ]
}

/// Non-blank text with single spaces between words, no leading or trailing whitespace.
pub(crate) fn arb_text() -> impl Strategy<Value = String> {
    vec(arb_word(), 1..=4).prop_map(|w| w.join(" "))
}

// ---------------------------------------------------------------------------
// Span, line, page
// ---------------------------------------------------------------------------

/// A span with arbitrary text and emphasis flags at the given size.
pub(crate) fn arb_span(size: u32, mono: bool) -> impl Strategy<Value = Span> {
    (arb_text(), any::<bool>(), any::<bool>())
        .prop_map(move |(text, bold, italic)| Span { text, bold, italic, mono, size })
}

/// Builds spans whose adjacent members always differ in (bold, italic).
fn distinct_neighbours(mut spans: Vec<Span>) -> Vec<Span> {
    for i in 1..spans.len() {
        if (spans[i].bold, spans[i].italic) == (spans[i - 1].bold, spans[i - 1].italic) {
            spans[i].bold = !spans[i].bold;
        }
    }
    spans
}

fn arb_spans(size: u32, mono: bool, max_spans: usize) -> impl Strategy<Value = Vec<Span>> {
    vec(arb_span(size, mono), 1..=max_spans).prop_map(distinct_neighbours)
}

fn make_line(spans: Vec<Span>, size: u32, x: i32, y: i32) -> TextLine {
    let chars: usize = spans.iter().map(|s| s.text.chars().count()).sum();
    let w = (chars as i32 * (size as i32 / 2 + 1)).max(1);
    TextLine::new(spans, size, x, y, w, line_height(size))
}

/// A line at `y = 0` (placed by the page builder). Size comes from `font_sizes`.
pub(crate) fn arb_line(font_sizes: Vec<u32>) -> impl Strategy<Value = TextLine> {
    assert!(!font_sizes.is_empty());
    (prop::sample::select(font_sizes), any::<bool>(), 50..300i32).prop_flat_map(|(size, mono, x)| {
        arb_spans(size, mono, 3).prop_map(move |spans| make_line(spans, size, x, 0))
    })
}

/// Stacks lines top to bottom with a pitch of one line height, starting in the content zone.
fn place(lines: Vec<TextLine>) -> Vec<TextLine> {
    let mut y = CONTENT_TOP;
    lines
        .into_iter()
        .map(|mut l| {
            l.y = y;
            y += l.h;
            l
        })
        .collect()
}

fn page_with(number: u32, lines: Vec<TextLine>) -> Page {
    Page { number, width: PAGE_WIDTH, height: PAGE_HEIGHT, lines }
}

/// A page (number 1) with body/heading lines in the content zone and an optional
/// header and footer. `arb_intermediate_doc` renumbers pages.
pub(crate) fn arb_page() -> impl Strategy<Value = Page> {
    let sizes = vec![BODY_SIZE, BODY_SIZE, BODY_SIZE, HEADING_SIZES[1]];
    (vec(arb_line(sizes), 0..=15), prop::option::of(arb_text()), prop::option::of(arb_text())).prop_map(
        |(body, header, footer)| {
            let mut lines = Vec::new();
            if let Some(t) = header {
                lines.push(single_span_line(&t, BODY_SIZE, 80, HEADER_Y));
            }
            lines.extend(place(body));
            if let Some(t) = footer {
                lines.push(single_span_line(&t, BODY_SIZE, 80, FOOTER_Y));
            }
            page_with(1, lines)
        },
    )
}

fn single_span_line(text: &str, size: u32, x: i32, y: i32) -> TextLine {
    let span = Span { text: text.to_string(), bold: false, italic: false, mono: false, size };
    make_line(vec![span], size, x, y)
}

/// Numbers pages 1..=n; `total_pages == last_page == n`.
pub(crate) fn doc_from_pages(mut pages: Vec<Page>) -> IntermediateDoc {
    for (i, p) in pages.iter_mut().enumerate() {
        p.number = i as u32 + 1;
    }
    let n = pages.len() as u32;
    IntermediateDoc { total_pages: n, first_page: 1, last_page: n, pages }
}

/// A document of 1 to 8 pages.
pub(crate) fn arb_intermediate_doc() -> impl Strategy<Value = IntermediateDoc> {
    vec(arb_page(), 1..=8).prop_map(doc_from_pages)
}

// ---------------------------------------------------------------------------
// Variants
// ---------------------------------------------------------------------------

fn plain_lines(sizes: Vec<u32>, mono: bool, n: std::ops::RangeInclusive<usize>) -> impl Strategy<Value = Vec<TextLine>> {
    vec(
        (prop::sample::select(sizes), 50..300i32).prop_flat_map(move |(size, x)| {
            arb_spans(size, mono, 2).prop_map(move |spans| make_line(spans, size, x, 0))
        }),
        n,
    )
}

fn single_page_doc(lines: Vec<TextLine>) -> IntermediateDoc {
    doc_from_pages(vec![page_with(1, place(lines))])
}

/// One page with many body lines and a few lines at heading sizes.
pub(crate) fn arb_heading_doc() -> impl Strategy<Value = IntermediateDoc> {
    (plain_lines(vec![BODY_SIZE], false, 8..=20), plain_lines(HEADING_SIZES.to_vec(), false, 1..=4)).prop_map(
        |(body, heads)| {
            // Headings sit between body runs so that each is preceded by body text.
            let mut lines = body;
            for (i, h) in heads.into_iter().enumerate() {
                let at = ((i + 1) * 3).min(lines.len());
                lines.insert(at, h);
            }
            single_page_doc(lines)
        },
    )
}

/// One page whose lines are mostly list items (`• `, `- `, `N. `) mixed with body lines.
pub(crate) fn arb_list_doc() -> impl Strategy<Value = IntermediateDoc> {
    let item = (arb_text(), prop::sample::select(vec!["• ", "- ", "1. ", "2) ", "◦ "]), 0..3i32).prop_map(
        |(t, marker, depth)| {
            let text = format!("{marker}{t}");
            single_span_line(&text, BODY_SIZE, 80 + depth * 24, 0)
        },
    );
    (vec(item, 2..=10), plain_lines(vec![BODY_SIZE], false, 2..=6)).prop_map(|(items, body)| {
        let mut lines = body;
        lines.extend(items);
        single_page_doc(lines)
    })
}

/// One page whose code lines are monospace, surrounded by body lines.
pub(crate) fn arb_code_doc() -> impl Strategy<Value = IntermediateDoc> {
    (plain_lines(vec![BODY_SIZE], false, 2..=6), plain_lines(vec![BODY_SIZE - 1], true, 1..=8)).prop_map(
        |(body, code)| {
            let mut lines = body;
            lines.extend(code);
            single_page_doc(lines)
        },
    )
}

/// A single page of body lines with the vertical pitch before each line.
#[derive(Debug, Clone)]
pub(crate) struct GapDoc {
    pub doc: IntermediateDoc,
    /// `pitches[i]` is the distance from line `i` to line `i + 1`.
    pub pitches: Vec<i32>,
}

/// Pitches between lines from 1.0 to 3.0 line heights, which straddles the 1.5 threshold.
pub(crate) fn arb_paragraph_gaps() -> impl Strategy<Value = GapDoc> {
    let h = line_height(BODY_SIZE);
    (plain_lines(vec![BODY_SIZE], false, 2..=12), vec(h..=3 * h, 11)).prop_map(move |(mut lines, pitches)| {
        let mut y = CONTENT_TOP;
        let mut used = Vec::new();
        for (i, l) in lines.iter_mut().enumerate() {
            l.y = y;
            if i + 1 < 12 {
                let p = pitches[i];
                used.push(p);
                y += p;
            }
        }
        used.truncate(lines.len() - 1);
        GapDoc { doc: doc_from_pages(vec![page_with(1, lines)]), pitches: used }
    })
}

#[cfg(test)]
mod self_checks {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn text_is_non_blank_and_trimmed(t in arb_text()) {
            prop_assert!(!t.trim().is_empty());
            prop_assert_eq!(t.trim(), t.as_str());
            prop_assert!(!t.chars().any(char::is_control));
        }

        #[test]
        fn docs_hold_model_invariant(d in arb_intermediate_doc()) {
            prop_assert!(d.invariant_holds());
            prop_assert!((1..=8).contains(&d.pages.len()));
            prop_assert_eq!(d.total_pages as usize, d.pages.len());
        }

        #[test]
        fn lines_have_uniform_size_and_distinct_neighbours(l in arb_line(vec![10, 12, 20])) {
            prop_assert!(l.invariant_holds());
            prop_assert!(l.spans.iter().all(|s| s.size == l.size));
            prop_assert!(l.spans.windows(2).all(|w| (w[0].bold, w[0].italic) != (w[1].bold, w[1].italic)));
        }

        #[test]
        fn variants_hold_invariant(
            h in arb_heading_doc(), l in arb_list_doc(), c in arb_code_doc(), g in arb_paragraph_gaps()
        ) {
            for d in [&h, &l, &c, &g.doc] {
                prop_assert!(d.invariant_holds());
                prop_assert_eq!(d.pages.len(), 1);
                prop_assert!(!d.pages[0].lines.is_empty());
            }
            prop_assert!(c.pages[0].lines.iter().any(|x| x.mono));
            prop_assert!(h.pages[0].lines.iter().any(|x| HEADING_SIZES.contains(&x.size)));
            prop_assert_eq!(g.pitches.len() + 1, g.doc.pages[0].lines.len());
            let lh = line_height(BODY_SIZE);
            prop_assert!(g.pitches.iter().all(|p| (lh..=3 * lh).contains(p)));
        }
    }
}

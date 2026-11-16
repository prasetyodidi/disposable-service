// Feature: pdf-to-markdown, Property 14: Heading menurut ukuran font
// **Validates: Requirements 3.1, 3.2, 3.9**
use std::collections::BTreeSet;

use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::structure::blocks::{body_font_size, heading_levels};
use crate::pdf2md::structure::{build, BuildOptions};

#[derive(Debug, Clone)]
struct Case {
    body: u32,
    /// (size, mono) of each extra line; line `i` has the token `token(i)`.
    extra: Vec<(u32, bool)>,
}

fn token(i: usize) -> String {
    format!("Zq{}{}", (b'a' + (i / 26) as u8) as char, (b'a' + (i % 26) as u8) as char)
}

fn is_heading(size: u32, body: u32) -> bool {
    size as u64 * 100 >= body as u64 * 115
}

fn arb_case() -> impl Strategy<Value = Case> {
    (6u32..=24).prop_flat_map(|body| {
        let near = (body * 115).div_ceil(100);
        let size = prop_oneof![
            3 => 1u32..=(body * 3),
            2 => (near.saturating_sub(1).max(1))..=(near + 1),
            1 => Just(body),
        ];
        (Just(body), vec((size, prop::bool::weighted(0.25)), 0..=12))
            .prop_map(|(body, extra)| Case { body, extra })
    })
}

fn span(text: String, size: u32, mono: bool) -> Span {
    Span { text, bold: false, italic: false, mono, size }
}

fn make_doc(c: &Case) -> IntermediateDoc {
    // Far-apart lines so that no two lines merge into one block.
    let mut lines = Vec::new();
    let mut y = 100;
    for (i, (size, mono)) in c.extra.iter().enumerate() {
        let t = token(i);
        let w = t.len() as i32 * 6;
        lines.push(TextLine::new(vec![span(t, *size, *mono)], *size, 50, y, w, 12));
        y += 100;
    }
    // Dominant body line: far more characters than all extra lines together.
    let body_text = "kata ".repeat(200).trim().to_string();
    lines.push(TextLine::new(vec![span(body_text, c.body, false)], c.body, 50, y, 600, 12));
    let page = Page { number: 1, width: 900, height: y + 200, lines };
    IntermediateDoc { total_pages: 1, first_page: 1, last_page: 1, pages: vec![page] }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn heading_levels_follow_font_size(c in arb_case()) {
        let doc = make_doc(&c);
        let pages = &doc.pages;
        let body = body_font_size(pages);
        prop_assert_eq!(body, c.body);
        let levels = heading_levels(pages, body);

        // Heading sizes: exactly the non-monospace sizes at or above the 1.15 threshold.
        let expected: BTreeSet<u32> = c.extra.iter()
            .filter(|(s, m)| !*m && is_heading(*s, body))
            .map(|(s, _)| *s)
            .collect();
        let got: BTreeSet<u32> = levels.keys().copied().collect();
        prop_assert_eq!(&got, &expected);

        // Levels in 1..=6; the six largest sizes get 1..=6 in order, the rest get 6.
        let desc: Vec<u32> = expected.iter().rev().copied().collect();
        for (i, s) in desc.iter().enumerate() {
            let lv = levels[s];
            prop_assert!((1..=6).contains(&lv));
            prop_assert_eq!(lv as usize, (i + 1).min(6));
        }
        // Monotonic: larger size never gets a larger level.
        for a in &desc {
            for b in &desc {
                if a > b {
                    prop_assert!(levels[a] <= levels[b]);
                }
            }
        }

        // End to end: heading count and rendered levels.
        let out = build(&doc, &BuildOptions { page_breaks: false, strip_headers_footers: false });
        let head_lines: Vec<(usize, u32)> = c.extra.iter().enumerate()
            .filter(|(_, (s, m))| !*m && is_heading(*s, body))
            .map(|(i, (s, _))| (i, *s))
            .collect();
        prop_assert_eq!(out.headings as usize, head_lines.len());
        if expected.is_empty() {
            prop_assert_eq!(out.headings, 0);
        }
        let md_lines: Vec<&str> = out.markdown.lines().collect();
        for (i, s) in &head_lines {
            let want = format!("{} {}", "#".repeat(levels[s] as usize), token(*i));
            prop_assert!(md_lines.contains(&want.as_str()), "missing {:?} in {:?}", want, out.markdown);
        }
        // Non-heading lines are never rendered as headings.
        for (i, (s, m)) in c.extra.iter().enumerate() {
            if *m || !is_heading(*s, body) {
                prop_assert!(!md_lines.iter().any(|l| l.starts_with('#') && l.ends_with(&token(i))));
            }
        }
    }
}

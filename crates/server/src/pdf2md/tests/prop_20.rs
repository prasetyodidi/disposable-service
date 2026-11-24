// Feature: pdf-to-markdown, Property 20: Blok kode
// **Validates: Requirements 4.6, 4.7, 4.8, 4.9, 4.10**
use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::structure::{build, BuildOptions};

const SIZE: u32 = 12;
const CHAR_W: i32 = 6;

/// One code line: text (trimmed, non-blank), indent units, and an optional style split point.
#[derive(Debug, Clone)]
struct CodeSrc {
    text: String,
    indent: usize,
    split: Option<usize>,
}

fn arb_code_line() -> impl Strategy<Value = CodeSrc> {
    let prefix = prop::sample::select(vec!["", "", "- ", "* ", "• ", "◦ ", "1. ", "2) ", "# ", "```", "``", "*a* "]);
    let body = vec(
        prop::sample::select(vec!['`', '`', '*', '#', '-', '_', 'a', 'b', 'Z', '1', '.', ')', ' ', '[', '<', '&', '\\', 'é']),
        0..=12,
    )
    .prop_map(|c| c.into_iter().collect::<String>());
    (prefix, body, 0usize..6, prop::option::of(1usize..8)).prop_map(|(p, b, indent, split)| {
        let mut text = format!("{p}{b}").trim().to_string();
        if text.is_empty() {
            text = "x".into();
        }
        CodeSrc { text, indent, split }
    })
}

fn arb_groups() -> impl Strategy<Value = Vec<Vec<CodeSrc>>> {
    vec(vec(arb_code_line(), 1..=5), 1..=4)
}

fn span(text: String, bold: bool, italic: bool, mono: bool) -> Span {
    Span { text, bold, italic, mono, size: SIZE }
}

fn code_line(c: &CodeSrc, x_base: i32, y: i32) -> TextLine {
    let n = c.text.chars().count();
    let spans = match c.split {
        Some(k) if k < n => {
            let a: String = c.text.chars().take(k).collect();
            let b: String = c.text.chars().skip(k).collect();
            vec![span(a, true, false, true), span(b, false, true, true)]
        }
        _ => vec![span(c.text.clone(), false, false, true)],
    };
    let x = x_base + c.indent as i32 * CHAR_W;
    TextLine::new(spans, SIZE, x, y, n as i32 * CHAR_W, 12)
}

fn body_line(t: &str, y: i32) -> TextLine {
    TextLine::new(vec![span(t.into(), false, false, false)], SIZE, 50, y, t.len() as i32 * CHAR_W, 12)
}

fn make_doc(groups: &[Vec<CodeSrc>], lead: bool) -> IntermediateDoc {
    let mut lines = Vec::new();
    let mut y = 100;
    if lead {
        lines.push(body_line("Pembuka", y));
        y += 100;
    }
    for (gi, g) in groups.iter().enumerate() {
        if gi > 0 {
            lines.push(body_line("Pemisah", y));
            y += 100;
        }
        for c in g {
            lines.push(code_line(c, 50, y));
            y += 14; // within 1.5 line heights: contiguous
        }
        y += 100 - 14;
    }
    let page = Page { number: 1, width: 900, height: y + 200, lines };
    IntermediateDoc { total_pages: 1, first_page: 1, last_page: 1, pages: vec![page] }
}

fn longest_run(s: &str) -> usize {
    let (mut best, mut cur) = (0, 0);
    for c in s.chars() {
        if c == '`' {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    best
}

fn expected_block(g: &[CodeSrc]) -> String {
    let min = g.iter().map(|c| c.indent).min().unwrap();
    let rows: Vec<String> = g.iter().map(|c| format!("{}{}", " ".repeat(c.indent - min), c.text)).collect();
    let longest = rows.iter().map(|r| longest_run(r)).max().unwrap();
    let fence = "`".repeat(if longest >= 3 { longest + 1 } else { 3 });
    format!("{fence}\n{}\n{fence}", rows.join("\n"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn monospace_groups_become_fenced_raw_blocks(groups in arb_groups(), lead in any::<bool>()) {
        let doc = make_doc(&groups, lead);
        prop_assert!(doc.invariant_holds());
        let out = build(&doc, &BuildOptions { page_breaks: false, strip_headers_footers: false });

        let mut parts: Vec<String> = Vec::new();
        if lead {
            parts.push("Pembuka".into());
        }
        for (gi, g) in groups.iter().enumerate() {
            if gi > 0 {
                parts.push("Pemisah".into());
            }
            parts.push(expected_block(g));
        }
        let expected = format!("{}\n", parts.join("\n\n"));
        prop_assert_eq!(&out.markdown, &expected);

        // Exact equality above implies one block per group, own-line fences, and no list items.
        prop_assert_eq!(out.headings, 0);
        prop_assert_eq!(out.hyphen_joins, 0);
    }
}

// Feature: pdf-to-markdown, Property 19: Daftar dan nesting
//! **Validates: Requirements 4.1, 4.2, 4.3, 4.4, 4.5**
use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::structure::{build, BuildOptions};

const SIZE: u32 = 12;
const H: i32 = 16;

#[derive(Debug, Clone)]
struct Item {
    marker: String,
    /// `Some(n)` for ordered markers (`n.` or `n)`).
    ordered: Option<u32>,
    /// Indentation step; x = 50 + 30 * step (well beyond the 6 unit tolerance).
    step: i32,
    text: String,
    /// Continuation lines (no marker, same x range, within 1.5 line heights).
    cont: Vec<String>,
}

fn arb_words() -> impl Strategy<Value = String> {
    vec("[a-z]{1,8}", 1..=3).prop_map(|w| w.join(" "))
}

fn arb_item() -> impl Strategy<Value = Item> {
    let marker = prop_oneof![
        prop::sample::select(vec!["•", "◦", "▪", "-", "*"]).prop_map(|m| (m.to_string(), None)),
        (1u32..100, prop::sample::select(vec![".", ")"])).prop_map(|(n, s)| (format!("{n}{s}"), Some(n))),
    ];
    (marker, 0..7i32, arb_words(), vec(arb_words(), 0..=2)).prop_map(|((marker, ordered), step, text, cont)| Item {
        marker,
        ordered,
        step,
        text,
        cont,
    })
}

fn line(text: &str, x: i32, y: i32) -> TextLine {
    let span = Span { text: text.to_string(), bold: false, italic: false, mono: false, size: SIZE };
    let w = (text.chars().count() as i32 * 7).max(1);
    TextLine::new(vec![span], SIZE, x, y, w, H)
}

/// Reference model: level stack of (x, level), levels in 1..=4.
fn expected_levels(items: &[Item]) -> Vec<u8> {
    let mut stack: Vec<(i32, u8)> = Vec::new();
    let mut out = Vec::new();
    for it in items {
        let x = it.step;
        let level = match stack.last().copied() {
            None => {
                stack.push((x, 1));
                1
            }
            Some((tx, tl)) if x > tx => {
                let l = (tl + 1).min(4);
                stack.push((x, l));
                l
            }
            Some((tx, tl)) if x == tx => tl,
            Some(_) => {
                while stack.last().map_or(false, |(tx, _)| *tx > x) {
                    stack.pop();
                }
                match stack.last() {
                    Some((_, tl)) => *tl,
                    None => {
                        stack.push((x, 1));
                        1
                    }
                }
            }
        };
        out.push(level);
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn lists_and_nesting(items in vec(arb_item(), 1..=12)) {
        let mut lines = Vec::new();
        let mut y = 150;
        for it in &items {
            let x = 50 + 30 * it.step;
            lines.push(line(&format!("{} {}", it.marker, it.text), x, y));
            y += H;
            for c in &it.cont {
                lines.push(line(c, x, y));
                y += H;
            }
        }
        let doc = IntermediateDoc {
            total_pages: 1,
            first_page: 1,
            last_page: 1,
            pages: vec![Page { number: 1, width: 918, height: 1188, lines }],
        };
        let out = build(&doc, &BuildOptions { page_breaks: false, strip_headers_footers: false });
        let md = out.markdown;

        let levels = expected_levels(&items);
        let expected: Vec<String> = items
            .iter()
            .zip(&levels)
            .map(|(it, l)| {
                let indent = " ".repeat(2 * (*l as usize - 1));
                let mut text = it.text.clone();
                for c in &it.cont {
                    text.push(' ');
                    text.push_str(c);
                }
                match it.ordered {
                    Some(n) => format!("{indent}{n}. {text}"),
                    None => format!("{indent}- {text}"),
                }
            })
            .collect();

        // Tight list: one item per line, no blank lines, single trailing newline.
        prop_assert_eq!(md.clone(), format!("{}\n", expected.join("\n")), "levels: {:?}", levels);
    }
}

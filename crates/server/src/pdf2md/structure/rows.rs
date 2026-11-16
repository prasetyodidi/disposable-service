//! Reading order and visual-row merging (design section 10.1).
//!
//! `pdftohtml` splits one visual row into several `<text>` elements when the font changes
//! (inline code) and separates bullet markers from item text. This step restores the row.

use crate::pdf2md::model::{Page, Span, TextLine};

/// How many most-recent rows are considered as merge candidates for a line.
const LOOKBACK: usize = 8;

/// Sorts lines by `(y, x)` (stable) and merges lines that share a visual row.
pub fn order_and_merge(mut page: Page) -> Page {
    let mut lines = std::mem::take(&mut page.lines);
    lines.sort_by_key(|l| (l.y, l.x));

    let mut rows: Vec<TextLine> = Vec::with_capacity(lines.len());
    for line in lines {
        let start = rows.len().saturating_sub(LOOKBACK);
        let target = (start..rows.len()).rev().find(|&i| same_row(&rows[i], &line));
        match target {
            Some(i) => {
                let row = rows.swap_remove(i);
                // swap_remove disturbs order; restore the row at index i.
                let merged = merge(row, line);
                rows.push(merged);
                let last = rows.len() - 1;
                rows.swap(i, last);
            }
            None => rows.push(line),
        }
    }

    // Merging can change `y`/`x`; re-establish reading order.
    rows.sort_by_key(|l| (l.y, l.x));
    page.lines = rows;
    page
}

/// True when the bottom edges are aligned (baseline proxy). Either line may be the left one.
fn same_row(a: &TextLine, b: &TextLine) -> bool {
    let tol = std::cmp::max(1, a.h.min(b.h) / 4);
    ((a.y + a.h) - (b.y + b.h)).abs() <= tol
}

fn merge(a: TextLine, b: TextLine) -> TextLine {
    // The more left line goes first; stable for equal x (a was seen earlier).
    let (mut left, right) = if b.x < a.x { (b, a) } else { (a, b) };

    let gap = right.x - (left.x + left.w);
    let need_space = gap > 0 && (gap as i64) * 5 >= left.h.max(right.h) as i64;
    if need_space {
        let ends_ws = left.spans.last().is_some_and(|s| s.text.ends_with(char::is_whitespace));
        let starts_ws = right.spans.first().is_some_and(|s| s.text.starts_with(char::is_whitespace));
        if !ends_ws && !starts_ws {
            if let Some(last) = left.spans.last_mut() {
                last.text.push(' ');
            }
        }
    }

    let top = left.y.min(right.y);
    let bottom = (left.y + left.h).max(right.y + right.h);
    let x = left.x;
    let end = (left.x + left.w).max(right.x + right.w);

    let mut spans = left.spans;
    spans.extend(right.spans);
    let size = dominant_size(&spans).unwrap_or_else(|| left.size.max(right.size));
    TextLine::new(spans, size, x, top, end - x, bottom - top)
}

/// Size of the span group with the most characters; ties go to the larger size.
fn dominant_size(spans: &[Span]) -> Option<u32> {
    let mut counts: Vec<(u32, usize)> = Vec::new();
    for s in spans {
        let n = s.text.chars().count();
        match counts.iter_mut().find(|(sz, _)| *sz == s.size) {
            Some(e) => e.1 += n,
            None => counts.push((s.size, n)),
        }
    }
    counts.into_iter().max_by_key(|&(sz, n)| (n, sz)).map(|(sz, _)| sz)
}

#[cfg(test)]
mod rows_tests {
    use super::*;

    fn sp(t: &str, size: u32, mono: bool) -> Span {
        Span { text: t.into(), bold: false, italic: false, mono, size }
    }

    fn line(t: &str, size: u32, mono: bool, x: i32, y: i32, w: i32, h: i32) -> TextLine {
        TextLine::new(vec![sp(t, size, mono)], size, x, y, w, h)
    }

    fn page(lines: Vec<TextLine>) -> Page {
        Page { number: 1, width: 600, height: 800, lines }
    }

    #[test]
    fn sorts_top_to_bottom_then_left_to_right() {
        let p = page(vec![
            line("c", 10, false, 10, 200, 10, 10),
            line("b", 10, false, 10, 100, 10, 10),
        ]);
        let out = order_and_merge(p);
        let texts: Vec<_> = out.lines.iter().map(|l| l.text()).collect();
        assert_eq!(texts, ["b", "c"]);
    }

    #[test]
    fn merges_same_row_with_space_on_gap() {
        let p = page(vec![
            line("world", 10, false, 60, 100, 50, 12),
            line("Hello", 10, false, 10, 100, 40, 12),
        ]);
        let out = order_and_merge(p);
        assert_eq!(out.lines.len(), 1);
        let l = &out.lines[0];
        assert_eq!(l.text(), "Hello world");
        assert_eq!((l.x, l.w), (10, 100));
    }

    #[test]
    fn no_space_when_adjacent() {
        let p = page(vec![
            line("(", 10, false, 10, 100, 5, 12),
            line("x)", 10, false, 15, 100, 10, 12),
        ]);
        assert_eq!(order_and_merge(p).lines[0].text(), "(x)");
    }

    #[test]
    fn keeps_different_rows_separate_and_tolerates_small_offset() {
        let p = page(vec![
            line("a", 10, false, 10, 100, 10, 12),
            line("b", 10, false, 30, 101, 10, 11),
            line("c", 10, false, 10, 130, 10, 12),
        ]);
        let out = order_and_merge(p);
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].text(), "a b");
        assert_eq!(out.lines[1].text(), "c");
    }

    #[test]
    fn dominant_size_and_mono_from_merge() {
        let p = page(vec![
            line("Use ", 10, false, 10, 100, 30, 12),
            line("foo()", 10, true, 41, 100, 30, 12),
        ]);
        let l = &order_and_merge(p).lines[0];
        assert!(!l.mono && l.invariant_holds());
        let q = page(vec![
            line("ab", 8, true, 10, 100, 10, 12),
            line("cdef", 12, true, 20, 100, 10, 12),
        ]);
        let m = &order_and_merge(q).lines[0];
        assert_eq!(m.size, 12);
        assert!(m.mono);
    }
}

//! Line classification and block grouping (heading, paragraph, list, code).
//!
//! Pure functions over the intermediate model; no engine-specific imports.

use std::collections::BTreeMap;

use crate::pdf2md::model::{Page, Span, TextLine};

pub const HEADING_NUM: u32 = 115;
pub const HEADING_DEN: u32 = 100;
const MAX_LIST_LEVEL: usize = 4;

/// One line of a code block: relative indentation (in spaces) and raw text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLine {
    pub indent: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, spans: Vec<Span> },
    Paragraph { spans: Vec<Span>, last_line_ends_sentence: bool, first_size: u32 },
    ListItem { ordered: Option<u32>, level: u8, spans: Vec<Span> },
    Code { lines: Vec<CodeLine> },
}

/// Element of the cross-page stream: a block or a page separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Block(Block),
    PageBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LineKind {
    Code,
    Heading(u8),
    Bullet { rest_start: usize },
    Ordered { n: u32, rest_start: usize },
    Text,
}

/// Dominant size by non-whitespace character count; ties go to the smaller size.
pub fn body_font_size(pages: &[Page]) -> u32 {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for line in pages.iter().flat_map(|p| p.lines.iter()) {
        for span in &line.spans {
            let n = span.text.chars().filter(|c| !c.is_whitespace()).count();
            if n > 0 {
                *counts.entry(span.size).or_insert(0) += n;
            }
        }
    }
    let mut best: Option<(u32, usize)> = None;
    for (size, n) in counts {
        match best {
            Some((_, bn)) if n <= bn => {}
            _ => best = Some((size, n)),
        }
    }
    best.map(|(s, _)| s).unwrap_or(0)
}

pub fn is_heading_size(size: u32, body: u32) -> bool {
    (size as u64) * (HEADING_DEN as u64) >= (body as u64) * (HEADING_NUM as u64)
}

/// Maps heading font sizes to levels 1..=6 (monotonic; monospace lines never count).
pub fn heading_levels(pages: &[Page], body: u32) -> BTreeMap<u32, u8> {
    let mut levels = BTreeMap::new();
    if body == 0 {
        return levels;
    }
    let mut sizes: Vec<u32> = pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .filter(|l| !l.mono && !l.text().trim().is_empty() && is_heading_size(l.size, body))
        .map(|l| l.size)
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes.dedup();
    for (i, s) in sizes.into_iter().enumerate() {
        levels.insert(s, (i + 1).min(6) as u8);
    }
    levels
}

fn trim_spans(mut s: Vec<Span>) -> Vec<Span> {
    while let Some(f) = s.first_mut() {
        let t = f.text.trim_start().to_string();
        if t.is_empty() {
            s.remove(0);
        } else {
            f.text = t;
            break;
        }
    }
    while let Some(l) = s.last_mut() {
        let t = l.text.trim_end().to_string();
        if t.is_empty() {
            s.pop();
        } else {
            l.text = t;
            break;
        }
    }
    s
}

fn spans_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Removes the first `n` bytes of text (must fall on a char boundary) across spans.
fn strip_prefix_bytes(spans: &mut Vec<Span>, mut n: usize) {
    while n > 0 && !spans.is_empty() {
        let len = spans[0].text.len();
        if len <= n {
            n -= len;
            spans.remove(0);
        } else {
            spans[0].text = spans[0].text[n..].to_string();
            n = 0;
        }
    }
}

fn detect_marker(text: &str) -> Option<LineKind> {
    let mut chars = text.chars();
    let first = chars.next()?;
    let spaces_after = |from: usize| -> Option<usize> {
        let n = text[from..].chars().take_while(|c| *c == ' ').count();
        if n >= 1 { Some(from + n) } else { None }
    };
    if matches!(first, '•' | '◦' | '▪' | '-' | '*') {
        return spaces_after(first.len_utf8()).map(|rest_start| LineKind::Bullet { rest_start });
    }
    let digits = text.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits) {
        let after = text[digits..].chars().next();
        if matches!(after, Some('.') | Some(')')) {
            let n: u32 = text[..digits].parse().ok()?;
            return spaces_after(digits + 1).map(|rest_start| LineKind::Ordered { n, rest_start });
        }
    }
    None
}

fn classify(line: &TextLine, text: &str, body: u32, levels: &BTreeMap<u32, u8>) -> LineKind {
    if line.mono {
        return LineKind::Code;
    }
    if is_heading_size(line.size, body) {
        if let Some(l) = levels.get(&line.size) {
            return LineKind::Heading(*l);
        }
    }
    detect_marker(text).unwrap_or(LineKind::Text)
}

#[derive(Clone, Copy)]
struct Prev {
    size: u32,
    y: i32,
    h: i32,
}

/// `2 * pitch <= 3 * prev.h` (vertical distance at most 1.5 line heights).
fn near(prev: Prev, y: i32) -> bool {
    2 * (y as i64 - prev.y as i64) <= 3 * prev.h as i64
}

fn level_for(x: i32, stack: &mut Vec<i32>, size: u32) -> u8 {
    let tol = (size as i32 / 2).max(2);
    if stack.is_empty() {
        stack.push(x);
        return 1;
    }
    let top = *stack.last().unwrap();
    if x > top + tol {
        stack.push(x);
        return stack.len().min(MAX_LIST_LEVEL) as u8;
    }
    if (x - top).abs() <= tol {
        return stack.len().min(MAX_LIST_LEVEL) as u8;
    }
    while stack.len() > 1 && *stack.last().unwrap() > x + tol {
        stack.pop();
    }
    let top = *stack.last().unwrap();
    if (x - top).abs() > tol && x < top {
        stack.clear();
        stack.push(x);
        return 1;
    }
    stack.len().min(MAX_LIST_LEVEL) as u8
}

/// Appends `next` after `prev` separated by one space (no hyphenation handling).
fn append_spaced(prev: &mut Vec<Span>, next: Vec<Span>) {
    let prev_ws = spans_text(prev).chars().last().map_or(true, char::is_whitespace);
    let next_ws = next.iter().find_map(|s| s.text.chars().next()).map_or(true, char::is_whitespace);
    if !prev_ws && !next_ws {
        if let Some(last) = prev.last_mut() {
            last.text.push(' ');
        }
    }
    prev.extend(next);
}

/// Joins `next` onto `prev`. Returns true when a hyphenation join occurred
/// (`prev` ends with `-` after a letter and `next` starts with a lowercase letter).
pub fn join_lines(prev: &mut Vec<Span>, next: Vec<Span>) -> bool {
    let ptext = spans_text(prev);
    let mut rev = ptext.chars().rev();
    let hyphen = rev.next() == Some('-') && rev.next().map_or(false, char::is_alphabetic);
    let lower = next
        .iter()
        .find_map(|s| s.text.chars().next())
        .map_or(false, char::is_lowercase);
    if hyphen && lower {
        if let Some(last) = prev.last_mut() {
            last.text.pop();
            if last.text.is_empty() {
                prev.pop();
            }
        }
        prev.extend(next);
        true
    } else {
        append_spaced(prev, next);
        false
    }
}

fn ends_sentence(spans: &[Span]) -> bool {
    matches!(spans_text(spans).trim_end().chars().last(), Some('.' | '!' | '?' | ':'))
}

/// Refreshes the derived `last_line_ends_sentence` flag of a paragraph.
pub fn refresh_paragraph(block: &mut Block) {
    if let Block::Paragraph { spans, last_line_ends_sentence, .. } = block {
        *last_line_ends_sentence = ends_sentence(spans);
    }
}

fn build_code_block(lines: &[(&TextLine, bool)]) -> Block {
    let x_min = lines.iter().map(|(l, _)| l.x).min().unwrap_or(0);
    let total_w: i64 = lines.iter().map(|(l, _)| l.w.max(0) as i64).sum();
    let total_c: i64 = lines.iter().map(|(l, _)| l.text().chars().count() as i64).sum();
    let unit = if total_c > 0 { (total_w / total_c).max(1) } else { 1 };
    let mut out = Vec::new();
    for (l, blank_before) in lines {
        if *blank_before {
            out.push(CodeLine { indent: 0, text: String::new() });
        }
        let dx = (l.x - x_min).max(0) as i64;
        let indent = ((2 * dx + unit) / (2 * unit)) as usize;
        out.push(CodeLine { indent, text: l.text().trim().to_string() });
    }
    Block::Code { lines: out }
}

/// Classifies and groups the lines of one page into blocks.
pub fn classify_and_group(page: &Page, body: u32, levels: &BTreeMap<u32, u8>) -> Vec<Block> {
    classify_and_group_counted(page, body, levels).0
}

/// Same as [`classify_and_group`], also returning the number of hyphenation joins.
pub fn classify_and_group_counted(
    page: &Page,
    body: u32,
    levels: &BTreeMap<u32, u8>,
) -> (Vec<Block>, u32) {
    let mut blocks: Vec<Block> = Vec::new();
    let mut joins = 0u32;
    let mut prev: Option<Prev> = None;
    let mut stack: Vec<i32> = Vec::new();
    let mut code: Vec<(&TextLine, bool)> = Vec::new();

    for line in &page.lines {
        let spans = trim_spans(line.spans.clone());
        let text = spans_text(&spans);
        if text.is_empty() {
            continue;
        }
        let kind = classify(line, &text, body, levels);
        let cur = Prev { size: line.size, y: line.y, h: line.h };
        let contiguous = prev.map_or(false, |p| p.size == line.size && near(p, line.y));

        if kind == LineKind::Code {
            let blank = prev.map_or(false, |p| !near(p, line.y)) && !code.is_empty();
            code.push((line, blank));
            stack.clear();
            prev = Some(cur);
            continue;
        }
        if !code.is_empty() {
            blocks.push(build_code_block(&code));
            code.clear();
        }

        match kind {
            LineKind::Code => unreachable!(),
            LineKind::Heading(level) => {
                stack.clear();
                match blocks.last_mut() {
                    Some(Block::Heading { level: l, spans: s }) if contiguous && *l == level => {
                        append_spaced(s, spans);
                    }
                    _ => blocks.push(Block::Heading { level, spans }),
                }
            }
            LineKind::Bullet { rest_start } | LineKind::Ordered { rest_start, .. } => {
                let ordered = match kind {
                    LineKind::Ordered { n, .. } => Some(n),
                    _ => None,
                };
                let mut spans = spans;
                strip_prefix_bytes(&mut spans, rest_start);
                let level = level_for(line.x, &mut stack, line.size);
                blocks.push(Block::ListItem { ordered, level, spans });
            }
            LineKind::Text => {
                let mut handled = false;
                if contiguous {
                    match blocks.last_mut() {
                        Some(b @ Block::Paragraph { .. }) => {
                            if let Block::Paragraph { spans: s, .. } = b {
                                if join_lines(s, spans.clone()) {
                                    joins += 1;
                                }
                            }
                            handled = true;
                        }
                        Some(Block::ListItem { spans: s, .. }) => {
                            if join_lines(s, spans.clone()) {
                                joins += 1;
                            }
                            handled = true;
                        }
                        _ => {}
                    }
                }
                if !handled {
                    stack.clear();
                    blocks.push(Block::Paragraph {
                        spans,
                        last_line_ends_sentence: false,
                        first_size: line.size,
                    });
                }
            }
        }
        prev = Some(cur);
    }
    if !code.is_empty() {
        blocks.push(build_code_block(&code));
    }
    for b in &mut blocks {
        refresh_paragraph(b);
    }
    (blocks, joins)
}

#[cfg(test)]
mod blocks_tests {
    use super::*;

    fn sp(t: &str, size: u32, mono: bool) -> Span {
        Span { text: t.into(), bold: false, italic: false, mono, size }
    }

    fn line(t: &str, size: u32, x: i32, y: i32, mono: bool) -> TextLine {
        let w = t.chars().count() as i32 * 6;
        TextLine::new(vec![sp(t, size, mono)], size, x, y, w, 12)
    }

    fn page(lines: Vec<TextLine>) -> Page {
        Page { number: 1, width: 600, height: 800, lines }
    }

    fn run(p: &Page) -> (Vec<Block>, u32) {
        let pages = std::slice::from_ref(p);
        let body = body_font_size(pages);
        let levels = heading_levels(pages, body);
        classify_and_group_counted(p, body, &levels)
    }

    fn text_of(b: &Block) -> String {
        match b {
            Block::Heading { spans, .. }
            | Block::Paragraph { spans, .. }
            | Block::ListItem { spans, .. } => spans_text(spans),
            Block::Code { lines } => {
                lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>().join("\n")
            }
        }
    }

    #[test]
    fn heading_threshold_is_integer_exact() {
        assert!(is_heading_size(115, 100));
        assert!(!is_heading_size(114, 100));
        assert!(is_heading_size(23, 20));
        assert!(!is_heading_size(22, 20));
    }

    #[test]
    fn body_size_tie_prefers_smaller() {
        let p = page(vec![line("aaaa", 10, 0, 0, false), line("bbbb", 20, 0, 20, false)]);
        assert_eq!(body_font_size(&[p]), 10);
    }

    #[test]
    fn heading_levels_monotonic_and_capped() {
        let mut lines = vec![line("body text here", 10, 0, 500, false)];
        for (i, s) in [30u32, 28, 26, 24, 22, 20, 18, 16].iter().enumerate() {
            lines.push(line("H", *s, 0, i as i32 * 40, false));
        }
        let p = page(lines);
        let levels = heading_levels(std::slice::from_ref(&p), 10);
        assert_eq!(levels[&30], 1);
        assert_eq!(levels[&20], 6);
        assert_eq!(levels[&16], 6);
        assert!(!levels.contains_key(&10));
    }

    #[test]
    fn mono_line_is_never_heading() {
        let p = page(vec![line("body body body body", 10, 0, 0, false), line("code", 30, 0, 100, true)]);
        let (blocks, _) = run(&p);
        assert!(matches!(blocks.last(), Some(Block::Code { .. })));
    }

    #[test]
    fn paragraph_split_by_gap() {
        let p = page(vec![
            line("one two", 10, 0, 0, false),
            line("three", 10, 0, 14, false),
            line("four", 10, 0, 60, false),
        ]);
        let (blocks, _) = run(&p);
        assert_eq!(blocks.len(), 2);
        assert_eq!(text_of(&blocks[0]), "one two three");
    }

    #[test]
    fn hyphenation_join_counts() {
        let p = page(vec![line("inter-", 10, 0, 0, false), line("national law", 10, 0, 14, false)]);
        let (blocks, joins) = run(&p);
        assert_eq!(joins, 1);
        assert_eq!(text_of(&blocks[0]), "international law");

        let p2 = page(vec![line("well-", 10, 0, 0, false), line("Known", 10, 0, 14, false)]);
        let (b2, j2) = run(&p2);
        assert_eq!(j2, 0);
        assert_eq!(text_of(&b2[0]), "well- Known");
    }

    #[test]
    fn nested_lists_and_continuation() {
        let p = page(vec![
            line("- first", 10, 10, 0, false),
            line("continued", 10, 10, 14, false),
            line("- nested", 10, 40, 28, false),
            line("2) second", 10, 10, 42, false),
        ]);
        let (blocks, _) = run(&p);
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], Block::ListItem { ordered: None, level: 1, .. }));
        assert_eq!(text_of(&blocks[0]), "first continued");
        assert!(matches!(&blocks[1], Block::ListItem { level: 2, .. }));
        assert!(matches!(&blocks[2], Block::ListItem { ordered: Some(2), level: 1, .. }));
        assert_eq!(text_of(&blocks[2]), "second");
    }

    #[test]
    fn list_level_capped_at_four() {
        let lines: Vec<_> = (0..6)
            .map(|i| line("- item", 10, i * 40, i * 14, false))
            .collect();
        let (blocks, _) = run(&page(lines));
        assert!(matches!(&blocks[5], Block::ListItem { level: 4, .. }));
    }

    #[test]
    fn code_wins_over_list_marker_and_keeps_indent() {
        let p = page(vec![
            line("body text", 10, 0, 0, false),
            line("- not a list", 10, 10, 100, true),
            line("indented", 10, 22, 114, true),
        ]);
        let (blocks, _) = run(&p);
        assert_eq!(blocks.len(), 2);
        match &blocks[1] {
            Block::Code { lines } => {
                assert_eq!(lines[0], CodeLine { indent: 0, text: "- not a list".into() });
                assert_eq!(lines[1].indent, 2);
            }
            other => panic!("expected code, got {other:?}"),
        }
    }

    #[test]
    fn sentence_end_flag() {
        let p = page(vec![line("ends here.", 10, 0, 0, false), line("no end", 10, 0, 80, false)]);
        let (blocks, _) = run(&p);
        assert!(matches!(&blocks[0], Block::Paragraph { last_line_ends_sentence: true, .. }));
        assert!(matches!(&blocks[1], Block::Paragraph { last_line_ends_sentence: false, .. }));
    }
}

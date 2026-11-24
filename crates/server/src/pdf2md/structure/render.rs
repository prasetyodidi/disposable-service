//! Renders the block stream to final Markdown text.

use super::blocks::{Block, CodeLine, Item};
use super::inline::render_spans;

/// Longest run of consecutive backticks in `s`.
fn longest_backtick_run(s: &str) -> usize {
    let (mut best, mut cur) = (0usize, 0usize);
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

fn render_code(lines: &[CodeLine]) -> String {
    let body: Vec<String> = lines
        .iter()
        .map(|l| if l.text.is_empty() { String::new() } else { format!("{}{}", " ".repeat(l.indent), l.text) })
        .collect();
    let longest = body.iter().map(|l| longest_backtick_run(l)).max().unwrap_or(0);
    let fence = "`".repeat(if longest >= 3 { longest + 1 } else { 3 });
    let mut out = String::new();
    out.push_str(&fence);
    out.push('\n');
    for l in &body {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(&fence);
    out
}

fn render_list_item(ordered: Option<u32>, level: u8, text: &str) -> String {
    let indent = " ".repeat(2 * (level.clamp(1, 4) as usize - 1));
    match ordered {
        Some(n) => format!("{indent}{n}. {text}"),
        None => format!("{indent}- {text}"),
    }
}

/// Renders one non-page-break block to text (no blank lines at either end).
fn render_block(block: &Block) -> String {
    match block {
        Block::Heading { level, spans } => {
            format!("{} {}", "#".repeat((*level).clamp(1, 6) as usize), render_spans(spans, false))
        }
        Block::Paragraph { spans, .. } => render_spans(spans, false),
        Block::ListItem { ordered, level, spans } => {
            render_list_item(*ordered, *level, &render_spans(spans, false))
        }
        Block::Code { lines } => render_code(lines),
    }
}

/// Renders the item stream: consecutive list items are separated by a single newline,
/// all other items by one blank line; the result ends with exactly one `\n`.
pub fn render(items: &[Item]) -> String {
    let mut out = String::new();
    let mut prev_list = false;
    for (i, item) in items.iter().enumerate() {
        let (text, is_list) = match item {
            Item::PageBreak => ("---".to_string(), false),
            Item::Block(b) => (render_block(b), matches!(b, Block::ListItem { .. })),
        };
        if i > 0 {
            out.push_str(if prev_list && is_list { "\n" } else { "\n\n" });
        }
        out.push_str(&text);
        prev_list = is_list;
    }
    out.push('\n');
    debug_assert!(!out.contains("\n\n\n") && out.ends_with('\n') && !out.ends_with("\n\n") || out == "\n");
    out
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::pdf2md::model::Span;

    fn sp(t: &str, bold: bool) -> Span {
        Span { text: t.into(), bold, italic: false, mono: false, size: 10 }
    }
    fn code(lines: &[(usize, &str)]) -> Item {
        Item::Block(Block::Code {
            lines: lines.iter().map(|(i, t)| CodeLine { indent: *i, text: t.to_string() }).collect(),
        })
    }

    #[test]
    fn heading_list_and_page_break() {
        let items = vec![
            Item::Block(Block::Heading { level: 2, spans: vec![sp("T", true)] }),
            Item::Block(Block::ListItem { ordered: None, level: 1, spans: vec![sp("a", false)] }),
            Item::Block(Block::ListItem { ordered: Some(3), level: 2, spans: vec![sp("b", false)] }),
            Item::PageBreak,
        ];
        assert_eq!(render(&items), "## **T**\n\n- a\n  3. b\n\n---\n");
    }

    #[test]
    fn code_is_raw_with_indent_and_longer_fence() {
        let md = render(&[code(&[(0, "```x"), (2, "*a* # b")])]);
        assert_eq!(md, "````\n```x\n  *a* # b\n````\n");
        let md = render(&[code(&[(0, "a"), (0, "``")])]);
        assert!(md.starts_with("```\n") && md.ends_with("\n```\n"));
    }

    #[test]
    fn code_blank_line_and_single_trailing_newline() {
        let md = render(&[code(&[(0, "a"), (0, ""), (0, "b")])]);
        assert_eq!(md, "```\na\n\nb\n```\n");
        assert!(!md.contains("\n\n\n"));
    }
}

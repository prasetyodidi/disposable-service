//! Inline rendering: Markdown escaping and bold/italic wrapping of spans.

use crate::pdf2md::model::Span;

/// Escapes Markdown-special characters. `\`, `` ` ``, `*`, `_`, `[`, `]` are always
/// escaped; `#` and `>` only at the very start of the text when `at_line_start`.
pub fn escape_markdown(s: &str, at_line_start: bool) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' => {
                out.push('\\');
                out.push(c);
            }
            '#' | '>' if at_line_start && i == 0 => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Inverse of [`escape_markdown`]: drops a backslash that precedes an escapable character.
pub fn unescape_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(&n) = it.peek() {
                if matches!(n, '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '>') {
                    out.push(n);
                    it.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

struct Run {
    text: String,
    bold: bool,
    italic: bool,
}

/// Merges adjacent same-style spans. Whitespace-only spans take the style of the
/// preceding run (or of the following run when first), so separators stay inside it.
fn normalize(spans: &[Span]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut pending_ws = String::new();
    for sp in spans.iter().filter(|s| !s.text.is_empty()) {
        if sp.text.chars().all(char::is_whitespace) {
            match runs.last_mut() {
                Some(last) => last.text.push_str(&sp.text),
                None => pending_ws.push_str(&sp.text),
            }
            continue;
        }
        let text = if pending_ws.is_empty() {
            sp.text.clone()
        } else {
            format!("{}{}", std::mem::take(&mut pending_ws), sp.text)
        };
        match runs.last_mut() {
            Some(last) if last.bold == sp.bold && last.italic == sp.italic => last.text.push_str(&text),
            _ => runs.push(Run { text, bold: sp.bold, italic: sp.italic }),
        }
    }
    if !pending_ws.is_empty() {
        runs.push(Run { text: pending_ws, bold: false, italic: false });
    }
    runs
}

/// Renders spans to Markdown text. In code blocks the raw text is returned unchanged.
/// Escaping applies per run before wrapping; the start of the text counts as line start.
pub fn render_spans(spans: &[Span], in_code: bool) -> String {
    if in_code {
        return spans.iter().map(|s| s.text.as_str()).collect();
    }
    let mut out = String::new();
    for (idx, run) in normalize(spans).iter().enumerate() {
        let escaped = escape_markdown(&run.text, idx == 0);
        let marker = match (run.bold, run.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        let core = escaped.trim();
        if marker.is_empty() || core.is_empty() {
            out.push_str(&escaped);
            continue;
        }
        let lead = &escaped[..escaped.len() - escaped.trim_start().len()];
        let trail = &escaped[escaped.trim_end().len()..];
        out.push_str(lead);
        out.push_str(marker);
        out.push_str(core);
        out.push_str(marker);
        out.push_str(trail);
    }
    out
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    fn sp(t: &str, bold: bool, italic: bool) -> Span {
        Span { text: t.into(), bold, italic, mono: false, size: 10 }
    }

    #[test]
    fn escapes_specials_and_line_start_only() {
        assert_eq!(escape_markdown("a*b_c`d[e]\\", false), "a\\*b\\_c\\`d\\[e\\]\\\\");
        assert_eq!(escape_markdown("#x>", true), "\\#x>");
        assert_eq!(escape_markdown("x#>", true), "x#>");
        assert_eq!(escape_markdown("#x", false), "#x");
    }

    #[test]
    fn unescape_round_trips() {
        for s in ["\\#", "a\\b", "*_`[]\\#>", "\\\\*", ""] {
            for at in [true, false] {
                assert_eq!(unescape_markdown(&escape_markdown(s, at)), s);
            }
        }
    }

    #[test]
    fn wraps_partial_runs_with_edge_spaces_outside() {
        let spans = [sp("plain ", false, false), sp("bold ", true, false), sp("it", false, true), sp(" both", true, true)];
        assert_eq!(render_spans(&spans, false), "plain **bold** *it* ***both***");
    }

    #[test]
    fn whole_block_style_uses_single_pair() {
        let spans = [sp("one ", true, false), sp("two", true, false)];
        assert_eq!(render_spans(&spans, false), "**one two**");
        let spans = [sp("a", true, false), sp(" ", false, false), sp("b", true, false)];
        assert_eq!(render_spans(&spans, false), "**a b**");
    }

    #[test]
    fn code_is_raw() {
        let spans = [sp("*a* #b", true, true)];
        assert_eq!(render_spans(&spans, true), "*a* #b");
    }

    #[test]
    fn marker_chars_in_text_are_escaped_inside_wrapper() {
        let spans = [sp("a*b", true, false)];
        assert_eq!(render_spans(&spans, false), "**a\\*b**");
    }
}

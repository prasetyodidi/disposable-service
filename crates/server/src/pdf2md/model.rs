//! Intermediate document model shared by extraction engines and the Structure_Builder.
//!
//! All coordinates and font sizes are integers so that ratio comparisons use integer
//! arithmetic and output stays deterministic.

/// A whole extracted document (or the requested page range of it).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntermediateDoc {
    /// Page count of the source document (not only the converted range).
    pub total_pages: u32,
    /// First effective page (1-based).
    pub first_page: u32,
    /// Last effective page, already clamped to `total_pages`.
    pub last_page: u32,
    /// Exactly `last_page - first_page + 1` pages, in ascending order.
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Page {
    pub number: u32,
    pub width: i32,
    pub height: i32,
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLine {
    pub spans: Vec<Span>,
    /// Dominant font size (most characters), in engine font units.
    pub size: u32,
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
    /// True iff every span is monospace.
    pub mono: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Decoded Unicode text (no XML entities).
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub size: u32,
}

impl TextLine {
    /// Builds a line and derives `mono` from its spans so the invariant always holds.
    pub fn new(spans: Vec<Span>, size: u32, x: i32, y: i32, w: i32, h: i32) -> Self {
        let mono = !spans.is_empty() && spans.iter().all(|s| s.mono);
        TextLine { spans, size, x, y, w, h, mono }
    }

    /// Concatenated plain text of all spans.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Checks the model invariant `mono == all(span.mono)` with non-empty spans.
    pub fn invariant_holds(&self) -> bool {
        !self.spans.is_empty() && self.mono == self.spans.iter().all(|s| s.mono)
    }
}

impl IntermediateDoc {
    /// Checks page-count and ordering invariants.
    pub fn invariant_holds(&self) -> bool {
        let expected = self.last_page.saturating_sub(self.first_page) as usize + 1;
        self.pages.len() == expected
            && self.pages.windows(2).all(|w| w[0].number < w[1].number)
            && self.pages.iter().all(|p| p.lines.iter().all(TextLine::invariant_holds))
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    fn span(t: &str, mono: bool) -> Span {
        Span { text: t.into(), bold: false, italic: false, mono, size: 10 }
    }

    #[test]
    fn mono_is_derived_from_spans() {
        let all = TextLine::new(vec![span("a", true), span("b", true)], 10, 0, 0, 1, 1);
        assert!(all.mono && all.invariant_holds());
        let mixed = TextLine::new(vec![span("a", true), span("b", false)], 10, 0, 0, 1, 1);
        assert!(!mixed.mono && mixed.invariant_holds());
        assert_eq!(mixed.text(), "ab");
    }
}

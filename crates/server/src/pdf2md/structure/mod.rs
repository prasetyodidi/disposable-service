//! Structure_Builder: pure heuristics that turn an `IntermediateDoc` into Markdown.
//! Must not import `poppler`, `xml`, or `proc`.
pub mod blocks;
pub mod inline;
pub mod render;
pub mod rows;
pub mod strip;

use crate::pdf2md::model::{IntermediateDoc, Page, Span};
use blocks::{Block, Item};

/// Options controlling the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildOptions {
    pub page_breaks: bool,
    pub strip_headers_footers: bool,
}

/// Result of [`build`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuildOutput {
    pub markdown: String,
    pub headings: u32,
    /// At most one entry (header/footer removal, Req 5.4).
    pub warnings: Vec<String>,
    pub removed_lines: u32,
    pub hyphen_joins: u32,
    pub word_count: u32,
}

fn spans_plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

fn block_words(b: &Block) -> usize {
    match b {
        Block::Heading { spans, .. }
        | Block::Paragraph { spans, .. }
        | Block::ListItem { spans, .. } => spans_plain(spans).split_whitespace().count(),
        Block::Code { lines } => lines.iter().map(|l| l.text.split_whitespace().count()).sum(),
    }
}

/// Merges blocks across page boundaries. Returns the item stream and the number of
/// hyphenation joins performed at page boundaries.
pub fn join_pages_counted(pages: Vec<Vec<Block>>, page_breaks: bool) -> (Vec<Item>, u32) {
    let mut out: Vec<Item> = Vec::new();
    let mut joins = 0u32;
    let mut prev_nonempty = false;
    for (i, page) in pages.into_iter().enumerate() {
        if i > 0 && page_breaks {
            out.push(Item::PageBreak);
        }
        let mut iter = page.into_iter();
        let nonempty = {
            let first = iter.next();
            match first {
                None => false,
                Some(first) => {
                    let mut pending = Some(first);
                    if i > 0 && !page_breaks && prev_nonempty {
                        if let (Some(Item::Block(last)), Some(next)) = (out.last_mut(), pending.as_ref()) {
                            match (last, next) {
                                (
                                    Block::Paragraph { spans, last_line_ends_sentence: false, .. },
                                    Block::Paragraph { spans: next_spans, .. },
                                ) => {
                                    if blocks::join_lines(spans, next_spans.clone()) {
                                        joins += 1;
                                    }
                                    pending = None;
                                    if let Some(Item::Block(b)) = out.last_mut() {
                                        blocks::refresh_paragraph(b);
                                    }
                                }
                                (Block::Code { lines }, Block::Code { lines: more }) => {
                                    lines.extend(more.iter().cloned());
                                    pending = None;
                                }
                                _ => {}
                            }
                        }
                    }
                    if let Some(b) = pending {
                        out.push(Item::Block(b));
                    }
                    true
                }
            }
        };
        out.extend(iter.map(Item::Block));
        prev_nonempty = nonempty;
    }
    (out, joins)
}

/// Design signature: stream of blocks and page breaks.
pub fn join_pages(pages: Vec<Vec<Block>>, page_breaks: bool) -> Vec<Item> {
    join_pages_counted(pages, page_breaks).0
}

/// Pure, deterministic conversion of the intermediate document to Markdown.
pub fn build(doc: &IntermediateDoc, opts: &BuildOptions) -> BuildOutput {
    debug_assert!(doc.invariant_holds());
    let mut pages: Vec<Page> = doc.pages.iter().cloned().map(rows::order_and_merge).collect();
    let removed = if opts.strip_headers_footers { strip::strip(&mut pages) } else { 0 };
    let body = blocks::body_font_size(&pages);
    let levels = blocks::heading_levels(&pages, body);

    let mut hyphen_joins = 0u32;
    let per_page: Vec<Vec<Block>> = pages
        .iter()
        .map(|p| {
            let (b, j) = blocks::classify_and_group_counted(p, body, &levels);
            hyphen_joins += j;
            b
        })
        .collect();

    let (items, page_joins) = join_pages_counted(per_page, opts.page_breaks);
    hyphen_joins += page_joins;

    let mut headings = 0u32;
    let mut words = 0usize;
    for item in &items {
        if let Item::Block(b) = item {
            if matches!(b, Block::Heading { .. }) {
                headings += 1;
            }
            words += block_words(b);
        }
    }

    let warnings = if removed >= 1 {
        vec![format!("{removed} baris header/footer dihapus")]
    } else {
        Vec::new()
    };

    BuildOutput {
        markdown: render::render(&items),
        headings,
        warnings,
        removed_lines: removed,
        hyphen_joins,
        word_count: words.min(u32::MAX as usize) as u32,
    }
}

#[cfg(test)]
mod mod_tests {
    use super::*;
    use crate::pdf2md::model::TextLine;

    fn line(t: &str, size: u32, y: i32) -> TextLine {
        let sp = Span { text: t.into(), bold: false, italic: false, mono: false, size };
        TextLine::new(vec![sp], size, 50, y, t.chars().count() as i32 * 6, 12)
    }

    fn doc(pages: Vec<Vec<TextLine>>) -> IntermediateDoc {
        let n = pages.len() as u32;
        IntermediateDoc {
            total_pages: n,
            first_page: 1,
            last_page: n,
            pages: pages
                .into_iter()
                .enumerate()
                .map(|(i, lines)| Page { number: i as u32 + 1, width: 600, height: 800, lines })
                .collect(),
        }
    }

    fn para(t: &str) -> Block {
        let mut b = Block::Paragraph {
            spans: vec![Span { text: t.into(), bold: false, italic: false, mono: false, size: 10 }],
            last_line_ends_sentence: false,
            first_size: 10,
        };
        blocks::refresh_paragraph(&mut b);
        b
    }

    #[test]
    fn heading_paragraph_and_list_tight() {
        let d = doc(vec![vec![
            line("Judul", 20, 50),
            line("Isi paragraf satu.", 10, 100),
            line("- a", 10, 200),
            line("- b", 10, 214),
        ]]);
        let o = build(&d, &BuildOptions { page_breaks: false, strip_headers_footers: true });
        assert_eq!(o.markdown, "# Judul\n\nIsi paragraf satu.\n\n- a\n- b\n");
        assert_eq!(o.headings, 1);
        assert!(o.warnings.is_empty());
        assert_eq!(build(&d, &BuildOptions { page_breaks: false, strip_headers_footers: true }), o);
    }

    #[test]
    fn page_breaks_count_is_pages_minus_one() {
        let items = join_pages(vec![vec![para("a.")], vec![], vec![para("b.")]], true);
        assert_eq!(items.iter().filter(|i| **i == Item::PageBreak).count(), 2);
    }

    #[test]
    fn paragraph_continues_across_pages_unless_sentence_ends() {
        let items = join_pages(vec![vec![para("kata awal")], vec![para("lanjutan.")]], false);
        assert_eq!(items.len(), 1);
        let items = join_pages(vec![vec![para("selesai.")], vec![para("baru")]], false);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn hyphen_join_across_pages_is_counted() {
        let (items, j) = join_pages_counted(vec![vec![para("kon-")], vec![para("versi")]], false);
        assert_eq!((items.len(), j), (1, 1));
    }

    #[test]
    fn adjacent_code_blocks_merge_across_pages() {
        let code = |t: &str| Block::Code { lines: vec![blocks::CodeLine { indent: 0, text: t.into() }] };
        let items = join_pages(vec![vec![code("a")], vec![code("b")]], false);
        assert_eq!(items.len(), 1);
        let items = join_pages(vec![vec![code("a")], vec![code("b")]], true);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn repeated_headers_removed_with_single_warning() {
        let pg = |body: &str| vec![line("Laporan", 10, 10), line(body, 10, 300)];
        let d = doc(vec![pg("satu"), pg("dua"), pg("tiga")]);
        let o = build(&d, &BuildOptions { page_breaks: false, strip_headers_footers: true });
        assert_eq!(o.removed_lines, 3);
        assert_eq!(o.warnings, vec!["3 baris header/footer dihapus".to_string()]);
        assert!(!o.markdown.contains("Laporan"));
        let o = build(&d, &BuildOptions { page_breaks: false, strip_headers_footers: false });
        assert!(o.warnings.is_empty() && o.markdown.contains("Laporan"));
    }
}

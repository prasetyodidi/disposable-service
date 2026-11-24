//! Property 12: content preservation.
// Feature: pdf-to-markdown, Property 12: Preservasi konten
//!
//! **Validates: Requirements 6.1, 6.2, 6.4, 16.3**
use proptest::prelude::*;

use super::gen_doc::{arb_code_doc, arb_heading_doc, arb_intermediate_doc, arb_list_doc};
use crate::pdf2md::model::IntermediateDoc;
use crate::pdf2md::structure::{build, BuildOptions};

/// Characters that Structure_Builder adds or escapes (`#`, `*`, `_`, `` ` ``, `[`, `]`, `\`).
const SYNTAX: [char; 7] = ['#', '*', '_', '`', '[', ']', '\\'];

fn is_list_marker(tok: &str) -> bool {
    matches!(tok, "•" | "-" | "◦")
        || tok
            .strip_suffix(['.', ')'])
            .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
}

/// Words with Markdown syntax characters removed; tokens left empty are dropped.
fn normalize(words: impl Iterator<Item = String>) -> Vec<String> {
    words
        .map(|w| w.chars().filter(|c| !SYNTAX.contains(c)).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Source words in reading order (page order, then top to bottom), without list markers.
fn source_words(doc: &IntermediateDoc) -> Vec<String> {
    let mut out = Vec::new();
    for page in &doc.pages {
        let mut lines: Vec<_> = page.lines.iter().collect();
        lines.sort_by_key(|l| (l.y, l.x));
        for l in lines {
            let text = l.text();
            let mut toks = text.split_whitespace().peekable();
            if toks.peek().is_some_and(|t| is_list_marker(t)) {
                toks.next();
            }
            out.extend(toks.map(str::to_string));
        }
    }
    normalize(out.into_iter())
}

/// Output words after dropping `---`, list markers and syntax characters.
fn output_words(md: &str) -> Vec<String> {
    normalize(
        md.split_whitespace()
            .filter(|t| !t.chars().all(|c| c == '-') && !is_list_marker(t))
            .map(str::to_string),
    )
}

fn check(doc: &IntermediateDoc, page_breaks: bool) -> Result<(), TestCaseError> {
    let out = build(doc, &BuildOptions { page_breaks, strip_headers_footers: false });
    let src = source_words(doc);
    let got = output_words(&out.markdown);
    prop_assert_eq!(out.removed_lines, 0);
    if out.hyphen_joins == 0 {
        prop_assert_eq!(&got, &src);
    }
    prop_assert_eq!(out.word_count as usize + out.hyphen_joins as usize, count_words(doc));
    Ok(())
}

/// Source word count as the builder counts it (list markers excluded).
fn count_words(doc: &IntermediateDoc) -> usize {
    doc.pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .map(|l| {
            let text = l.text();
            let n = text.split_whitespace().count();
            if text.split_whitespace().next().is_some_and(is_list_marker) {
                n - 1
            } else {
                n
            }
        })
        .sum()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn general_docs_preserve_words(doc in arb_intermediate_doc(), page_breaks in any::<bool>()) {
        check(&doc, page_breaks)?;
    }

    #[test]
    fn variant_docs_preserve_words(
        d in prop_oneof![arb_heading_doc(), arb_list_doc(), arb_code_doc()],
        page_breaks in any::<bool>(),
    ) {
        check(&d, page_breaks)?;
    }
}

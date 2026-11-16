// Feature: pdf-to-markdown, Property 21: Penghapusan header, footer, dan nomor halaman
// **Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.8**
use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::pdf2md::structure::{build, BuildOptions};

const PAGE_H: i32 = 1000;
const HEADER_BASE: &str = "Hdr Laporan";
const FOOTER_BASE: &str = "Ftr Rahasia";

fn line(text: &str, y: i32) -> TextLine {
    let s = Span { text: text.into(), bold: false, italic: false, mono: false, size: 10 };
    TextLine::new(vec![s], 10, 50, y, text.chars().count() as i32 * 6, 12)
}

/// Page number text in one of the recognised styles.
fn page_number_text(style: usize, n: u32, total: u32) -> String {
    const ROMAN: [&str; 8] = ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii"];
    match style {
        0 => n.to_string(),
        1 => ROMAN[(n - 1) as usize % 8].to_string(),
        2 => format!("Page {n}"),
        3 => format!("Halaman {n}"),
        _ => format!("{n} of {total}"),
    }
}

#[derive(Debug, Clone)]
struct Case {
    total: usize,
    header: Vec<bool>,
    footer: Vec<bool>,
    pagenum: Vec<bool>,
    style: usize,
    strip: bool,
}

fn arb_case() -> impl Strategy<Value = Case> {
    (1usize..=8, vec(any::<bool>(), 8), vec(any::<bool>(), 8), vec(any::<bool>(), 8), 0usize..5, any::<bool>())
        .prop_map(|(total, header, footer, pagenum, style, strip)| Case {
            total,
            header: header[..total].to_vec(),
            footer: footer[..total].to_vec(),
            pagenum: pagenum[..total].to_vec(),
            style,
            strip,
        })
}

fn make_doc(c: &Case) -> IntermediateDoc {
    let total = c.total as u32;
    let pages = (0..c.total)
        .map(|i| {
            let n = i as u32 + 1;
            let mut lines = Vec::new();
            if c.header[i] {
                lines.push(line(&format!("{HEADER_BASE} {n}"), 10));
            }
            // Body area: unique text, plus identical text to the header/footer (never removed).
            lines.push(line(&format!("Teks isi unik {}", "a".repeat(i + 1)), 400));
            lines.push(line(&format!("{HEADER_BASE} {n}"), 450));
            lines.push(line(&format!("{FOOTER_BASE} {n}"), 500));
            if c.footer[i] {
                lines.push(line(&format!("{FOOTER_BASE} {n}"), 900));
            }
            if c.pagenum[i] {
                lines.push(line(&page_number_text(c.style, n, total), 960));
            }
            Page { number: n, width: 600, height: PAGE_H, lines }
        })
        .collect();
    IntermediateDoc { total_pages: total, first_page: 1, last_page: total, pages }
}

/// Whether k occurrences on `total` pages are removed as repeated lines.
fn repeated(k: usize, total: usize) -> bool {
    total >= 3 && k >= 3 && 2 * k >= total
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn headers_footers_and_page_numbers_are_stripped_exactly(c in arb_case()) {
        let doc = make_doc(&c);
        prop_assert!(doc.invariant_holds());
        let out = build(&doc, &BuildOptions { page_breaks: false, strip_headers_footers: c.strip });

        let kh = c.header.iter().filter(|b| **b).count();
        let kf = c.footer.iter().filter(|b| **b).count();
        let kp = c.pagenum.iter().filter(|b| **b).count();

        let (exp_h, exp_f, exp_p) = if c.strip {
            (if repeated(kh, c.total) { kh } else { 0 }, if repeated(kf, c.total) { kf } else { 0 }, kp)
        } else {
            (0, 0, 0)
        };
        let expected_removed = (exp_h + exp_f + exp_p) as u32;
        prop_assert_eq!(out.removed_lines, expected_removed);

        // Content-area duplicates survive; zone lines survive unless removed.
        prop_assert_eq!(out.markdown.matches(HEADER_BASE).count(), c.total + kh - exp_h);
        prop_assert_eq!(out.markdown.matches(FOOTER_BASE).count(), c.total + kf - exp_f);

        // Exactly one warning iff something was removed; it carries only the count.
        if expected_removed >= 1 {
            prop_assert_eq!(out.warnings.len(), 1);
            prop_assert!(out.warnings[0].contains(&expected_removed.to_string()));
            prop_assert!(!out.warnings[0].contains(HEADER_BASE));
            prop_assert!(!out.warnings[0].contains(FOOTER_BASE));
        } else {
            prop_assert!(out.warnings.is_empty());
        }
    }
}

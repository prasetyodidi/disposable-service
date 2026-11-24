//! Unit tests for the Structure_Builder driven by hand-written `pdftohtml -xml` (task 2.26).
//!
//! Requirements: 5.5, 5.6.
use crate::pdf2md::model::{IntermediateDoc, Page};
use crate::pdf2md::structure::{build, BuildOptions};
use crate::pdf2md::xml::parse_pdftohtml_xml;

const FONTS: &str = r##"<fontspec id="0" size="10" family="Times" color="#000000"/>
<fontspec id="1" size="20" family="Times-Bold" color="#000000"/>
<fontspec id="2" size="10" family="Courier" color="#000000"/>
"##;

fn t(top: i32, left: i32, font: u32, text: &str) -> String {
    let w = text.chars().count() as i32 * 6;
    format!(r#"<text top="{top}" left="{left}" width="{w}" height="12" font="{font}">{text}</text>"#)
}

fn page(n: u32, body: &[String]) -> String {
    let mut s = format!(r#"<page number="{n}" position="absolute" top="0" left="0" height="1000" width="600">{FONTS}"#);
    s.push_str(&t(20, 50, 0, "Laporan Tahunan"));
    for b in body {
        s.push_str(b);
    }
    s.push_str(&t(960, 50, 0, &format!("Halaman {n}")));
    s.push_str("</page>\n");
    s
}

fn xml(pages: &[String]) -> String {
    format!("<?xml version=\"1.0\"?>\n<pdf2xml>\n{}</pdf2xml>", pages.concat())
}

fn sample() -> String {
    xml(&[
        page(
            1,
            &[
                t(100, 50, 1, "<b>Judul Besar</b>"),
                t(150, 50, 0, "Paragraf pertama dengan <b>tebal</b> dan"),
                t(164, 50, 0, "<i>miring</i> di sini."),
                t(200, 60, 0, "- satu"),
                t(214, 100, 0, "- bersarang"),
                t(228, 60, 0, "- dua"),
                t(300, 60, 2, "int main() {"),
                t(314, 72, 2, "return 0;"),
                t(328, 60, 2, "}"),
            ],
        ),
        page(2, &[t(150, 50, 0, "Isi halaman dua.")]),
        page(3, &[t(150, 50, 0, "Isi halaman tiga.")]),
    ])
}

fn run(x: &str, page_breaks: bool, strip: bool) -> crate::pdf2md::structure::BuildOutput {
    let doc = parse_pdftohtml_xml(x.as_bytes()).unwrap();
    build(&doc, &BuildOptions { page_breaks, strip_headers_footers: strip })
}

#[test]
fn heading_paragraph_nested_list_code_and_inline_styles() {
    let o = run(&sample(), false, true);
    let md = &o.markdown;
    assert!(md.starts_with("# **Judul Besar**\n\n"), "{md}");
    assert!(md.contains("Paragraf pertama dengan **tebal** dan *miring* di sini."), "{md}");
    assert!(md.contains("- satu\n  - bersarang\n- dua\n"), "{md}");
    assert!(md.contains("```\nint main() {\n  return 0;\n}\n```"), "{md}");
    assert_eq!(o.headings, 1);
    assert!(md.ends_with('\n') && !md.ends_with("\n\n") && !md.contains("\n\n\n"));
}

#[test]
fn repeated_headers_and_footers_are_removed_with_one_warning() {
    let o = run(&sample(), false, true);
    assert!(!o.markdown.contains("Laporan Tahunan"));
    assert!(!o.markdown.contains("Halaman 1"));
    assert!(o.removed_lines >= 3);
    assert_eq!(o.warnings.len(), 1);

    let kept = run(&sample(), false, false);
    assert!(kept.markdown.contains("Laporan Tahunan"));
    assert_eq!(kept.removed_lines, 0);
    assert!(kept.warnings.is_empty());
}

#[test]
fn page_breaks_insert_one_separator_between_pages() {
    let with = run(&sample(), true, true);
    assert_eq!(with.markdown.matches("\n---\n").count(), 2, "{}", with.markdown);
    let without = run(&sample(), false, true);
    assert!(!without.markdown.contains("---"));
}

#[test]
fn output_is_deterministic() {
    assert_eq!(run(&sample(), true, true), run(&sample(), true, true));
}

#[test]
fn fully_bold_heading_is_wrapped_in_bold_markers() {
    let x = xml(&[page(1, &[t(100, 50, 1, "Judul"), t(150, 50, 0, "isi teks biasa di sini.")])]);
    let md = run(&x, false, false).markdown;
    assert!(md.contains("\n\n# **Judul**\n\n"), "{md}");
}

#[test]
fn entity_amp_lt_renders_as_literal_lt() {
    let x = xml(&[page(1, &[t(150, 50, 0, "a &amp;lt; b")])]);
    let md = run(&x, false, false).markdown;
    assert!(md.contains("a &lt; b"), "{md}");
}

#[test]
fn document_without_text_builds_empty_output() {
    let x = xml(&[r#"<page number="1" height="1000" width="600"></page>"#.to_string()]);
    let o = run(&x, false, true);
    assert_eq!(o.markdown, "\n");
    assert_eq!((o.headings, o.word_count, o.removed_lines), (0, 0, 0));
    assert!(o.warnings.is_empty());
}

#[test]
fn hand_built_model_with_empty_page_builds_empty_output() {
    let doc = IntermediateDoc {
        total_pages: 1,
        first_page: 1,
        last_page: 1,
        pages: vec![Page { number: 1, width: 600, height: 800, lines: vec![] }],
    };
    let o = build(&doc, &BuildOptions { page_breaks: false, strip_headers_footers: true });
    assert_eq!(o.markdown, "\n");
}

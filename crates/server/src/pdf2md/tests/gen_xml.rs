//! Test serializer from `IntermediateDoc` to `pdftohtml -xml` output.
//!
//! Round-trip contract: for documents from `gen_doc` (uniform span size and mono flag per
//! line, adjacent spans differing in bold/italic, `total_pages == last_page`, first page 1),
//! `parse_pdftohtml_xml(to_pdftohtml_xml(doc)) == doc`.
#![allow(dead_code)]
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::pdf2md::model::{IntermediateDoc, Span};

/// Escapes the five XML special characters.
pub(crate) fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn span_xml(s: &Span) -> String {
    let mut t = escape_xml(&s.text);
    if s.italic {
        t = format!("<i>{t}</i>");
    }
    if s.bold {
        t = format!("<b>{t}</b>");
    }
    t
}

/// Serializes the document in the `<pdf2xml>/<page>/<fontspec>/<text>` shape.
pub(crate) fn to_pdftohtml_xml(doc: &IntermediateDoc) -> Vec<u8> {
    // One fontspec per distinct (size, mono); ids are global and declared on first use.
    let mut fonts: BTreeMap<(u32, bool), u32> = BTreeMap::new();
    for l in doc.pages.iter().flat_map(|p| &p.lines) {
        let next = fonts.len() as u32;
        fonts.entry((l.size, l.mono)).or_insert(next);
    }

    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<!DOCTYPE pdf2xml SYSTEM \"pdf2xml.dtd\">\n");
    x.push_str("<pdf2xml producer=\"poppler\" version=\"22.12.0\">\n");
    let mut declared = false;
    for p in &doc.pages {
        let _ = writeln!(
            x,
            "<page number=\"{}\" position=\"absolute\" top=\"0\" left=\"0\" height=\"{}\" width=\"{}\">",
            p.number, p.height, p.width
        );
        if !declared {
            for (&(size, mono), id) in &fonts {
                let family = if mono { "Courier" } else { "Times" };
                let _ = writeln!(
                    x,
                    "<fontspec id=\"{id}\" size=\"{size}\" family=\"{family}\" color=\"#000000\"/>"
                );
            }
            declared = true;
        }
        for l in &p.lines {
            let font = fonts[&(l.size, l.mono)];
            let body: String = l.spans.iter().map(span_xml).collect();
            let _ = writeln!(
                x,
                "<text top=\"{}\" left=\"{}\" width=\"{}\" height=\"{}\" font=\"{font}\">{body}</text>",
                l.y, l.x, l.w, l.h
            );
        }
        x.push_str("</page>\n");
    }
    x.push_str("</pdf2xml>\n");
    x.into_bytes()
}

#[cfg(test)]
mod self_checks {
    use super::*;
    use crate::pdf2md::tests::gen_doc::{arb_code_doc, arb_intermediate_doc};
    use crate::pdf2md::xml::parse_pdftohtml_xml;
    use proptest::prelude::*;

    #[test]
    fn escapes_special_characters() {
        assert_eq!(escape_xml("a&b<c>\"d\"'e"), "a&amp;b&lt;c&gt;&quot;d&quot;&apos;e");
    }

    #[test]
    fn empty_page_doc_serializes_and_parses() {
        let doc = crate::pdf2md::tests::gen_doc::doc_from_pages(vec![Default::default()]);
        let parsed = parse_pdftohtml_xml(&to_pdftohtml_xml(&doc)).unwrap();
        assert_eq!(parsed.pages.len(), 1);
        assert!(parsed.pages[0].lines.is_empty());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn serializer_round_trips_through_parser(doc in arb_intermediate_doc()) {
            let parsed = parse_pdftohtml_xml(&to_pdftohtml_xml(&doc)).unwrap();
            prop_assert_eq!(parsed, doc);
        }

        #[test]
        fn code_docs_round_trip(doc in arb_code_doc()) {
            let parsed = parse_pdftohtml_xml(&to_pdftohtml_xml(&doc)).unwrap();
            prop_assert_eq!(parsed, doc);
        }
    }
}

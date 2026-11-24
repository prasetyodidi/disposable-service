//! Unit tests for the `pdftohtml -xml` parser (task 2.26).
//!
//! Requirements: 5.5, 5.6 (and the single-pass entity decoding rule of the design).
use crate::pdf2md::xml::{decode_entities, parse_pdftohtml_xml, XmlError};

fn wrap(pages: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE pdf2xml SYSTEM \"pdf2xml.dtd\">\n\
         <pdf2xml producer=\"poppler\" version=\"22.12.0\">\n{pages}</pdf2xml>\n"
    )
}

const FONTS: &str = r##"<fontspec id="0" size="10" family="Times" color="#000000"/>
<fontspec id="1" size="20" family="Times-Bold" color="#000000"/>
<fontspec id="2" size="10" family="Courier" color="#000000"/>
<fontspec id="3" size="10" family="Helvetica-BoldOblique" color="#000000"/>
"##;

#[test]
fn parses_pages_fonts_and_inline_styles() {
    let xml = wrap(&format!(
        r#"<page number="1" position="absolute" top="0" left="0" height="1000" width="600">
{FONTS}<text top="100" left="50" width="200" height="22" font="1">Judul</text>
<text top="150" left="50" width="300" height="12" font="0">biasa <b>tebal</b> dan <i>miring</i></text>
<text top="170" left="60" width="100" height="12" font="2">x = 1;</text>
<text top="190" left="50" width="100" height="12" font="3">gaya</text>
</page>
<page number="2" position="absolute" top="0" left="0" height="1000" width="600">
<text top="100" left="50" width="100" height="12" font="0">dua</text>
</page>
"#
    ));
    let doc = parse_pdftohtml_xml(xml.as_bytes()).unwrap();
    assert_eq!(doc.total_pages, 2);
    assert_eq!((doc.first_page, doc.last_page), (1, 2));
    assert!(doc.invariant_holds());
    assert_eq!((doc.pages[0].width, doc.pages[0].height), (600, 1000));

    let l = &doc.pages[0].lines;
    assert_eq!(l.len(), 4);
    assert_eq!((l[0].size, l[0].x, l[0].y, l[0].w, l[0].h), (20, 50, 100, 200, 22));
    assert!(l[0].spans[0].bold, "family Times-Bold marks the span bold");
    assert_eq!(l[1].text(), "biasa tebal dan miring");
    let bold: Vec<_> = l[1].spans.iter().filter(|s| s.bold).map(|s| s.text.as_str()).collect();
    let ital: Vec<_> = l[1].spans.iter().filter(|s| s.italic).map(|s| s.text.as_str()).collect();
    assert_eq!(bold, vec!["tebal"]);
    assert_eq!(ital, vec!["miring"]);
    assert!(l[2].mono && !l[2].spans[0].bold);
    assert!(l[3].spans[0].bold && l[3].spans[0].italic && !l[3].mono);
    assert_eq!(doc.pages[1].lines[0].text(), "dua");
}

#[test]
fn entities_are_decoded_exactly_once() {
    let xml = wrap(&format!(
        r#"<page number="1" height="100" width="100">{FONTS}<text top="1" left="1" width="9" height="9" font="0">&amp;lt; &amp;amp; &lt;b&gt; &#65;</text></page>"#
    ));
    let doc = parse_pdftohtml_xml(xml.as_bytes()).unwrap();
    assert_eq!(doc.pages[0].lines[0].text(), "&lt; &amp; <b> A");
    assert_eq!(decode_entities("&amp;lt;"), "&lt;");
}

#[test]
fn whitespace_only_lines_are_dropped() {
    let xml = wrap(&format!(
        r#"<page number="1" height="100" width="100">{FONTS}<text top="1" left="1" width="9" height="9" font="0">   </text><text top="20" left="1" width="9" height="9" font="0">ada</text></page>"#
    ));
    let doc = parse_pdftohtml_xml(xml.as_bytes()).unwrap();
    assert_eq!(doc.pages[0].lines.len(), 1);
    assert_eq!(doc.pages[0].lines[0].text(), "ada");
}

#[test]
fn document_without_text_has_pages_but_no_lines() {
    let xml = wrap(
        r#"<page number="1" height="100" width="100"></page><page number="2" height="100" width="100"/>"#,
    );
    let doc = parse_pdftohtml_xml(xml.as_bytes()).unwrap();
    assert_eq!(doc.pages.len(), 2);
    assert!(doc.pages.iter().all(|p| p.lines.is_empty()));
    assert!(doc.invariant_holds());
}

#[test]
fn malformed_xml_is_rejected() {
    for bad in [
        &b""[..],
        b"bukan xml sama sekali",
        b"<pdf2xml><page number=\"1\">",
        b"<pdf2xml></pdf2xml></pdf2xml>",
        b"<pdf2xml><page number=\"1\" height=\"1\" width=\"1\"><text",
    ] {
        assert_eq!(parse_pdftohtml_xml(bad), Err(XmlError::Malformed), "{:?}", String::from_utf8_lossy(bad));
    }
}

#[test]
fn well_formed_without_pages_is_no_pages() {
    assert_eq!(parse_pdftohtml_xml(wrap("").as_bytes()), Err(XmlError::NoPages));
    assert_eq!(parse_pdftohtml_xml(wrap(FONTS).as_bytes()), Err(XmlError::NoPages));
}

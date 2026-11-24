//! Parser for `pdftohtml -xml` output into the intermediate document model.
//!
//! `quick-xml` is used only for tokenisation. Entities are decoded exactly once, by
//! [`decode_entities`], which is the only decoding path in this module.

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::model::{IntermediateDoc, Page, Span, TextLine};

/// Errors from [`parse_pdftohtml_xml`]. Carries no document content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlError {
    /// The XML could not be tokenised or is structurally broken.
    Malformed,
    /// Well-formed XML that contains no `<page>` element.
    NoPages,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FontInfo {
    size: u32,
    mono: bool,
    bold: bool,
    italic: bool,
}

impl FontInfo {
    fn from_family(size: u32, family: &str) -> Self {
        let f = family.to_lowercase();
        let any = |names: &[&str]| names.iter().any(|n| f.contains(n));
        FontInfo {
            size,
            mono: any(&[
                "courier",
                "consolas",
                "menlo",
                "monaco",
                "mono",
                "cmtt",
                "typewriter",
                "lucida console",
                "inconsolata",
            ]),
            bold: any(&["bold", "black", "heavy", "semibold"]),
            italic: any(&["italic", "oblique"]),
        }
    }
}

/// Decodes XML entities in a single left-to-right pass.
///
/// Handles `&amp; &lt; &gt; &quot; &apos;`, `&#N;` and `&#xH;`. Numeric references that
/// are zero, a surrogate, out of range, or a C0 control (other than TAB/LF/CR) become
/// U+FFFD. A stray `&` and unknown entities stay literal. Output is never re-scanned,
/// so `&amp;lt;` becomes `&lt;`.
pub fn decode_entities(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < bytes.len() {
        // Copy everything up to the next '&' in one go ('&' is ASCII, so this is a char boundary).
        if bytes[i] != b'&' {
            let next = bytes[i..].iter().position(|&b| b == b'&').map_or(bytes.len(), |p| i + p);
            out.push_str(&raw[i..next]);
            i = next;
            continue;
        }
        let window_end = bytes.len().min(i + 13);
        let semi = bytes[i + 1..window_end].iter().position(|&b| b == b';').map(|p| i + 1 + p);
        let Some(j) = semi else {
            out.push('&');
            i += 1;
            continue;
        };
        let name = &raw[i + 1..j];
        match decode_reference(name) {
            Some(c) => {
                out.push(c);
                i = j + 1;
            }
            None => {
                // Unknown entity or stray '&': keep literal and continue right after the '&'.
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

fn decode_reference(name: &str) -> Option<char> {
    match name {
        "amp" => return Some('&'),
        "lt" => return Some('<'),
        "gt" => return Some('>'),
        "quot" => return Some('"'),
        "apos" => return Some('\''),
        _ => {}
    }
    let digits = name.strip_prefix('#')?;
    let (radix, digits) = match digits.strip_prefix(['x', 'X']) {
        Some(h) => (16, h),
        None => (10, digits),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let cp = match u32::from_str_radix(digits, radix) {
        Ok(v) => v,
        Err(_) => return Some('\u{FFFD}'),
    };
    match char::from_u32(cp) {
        Some(c) if cp == 0 || (cp < 0x20 && !matches!(c, '\t' | '\n' | '\r')) => {
            let _ = c;
            Some('\u{FFFD}')
        }
        Some(c) => Some(c),
        None => Some('\u{FFFD}'),
    }
}

fn attr_map(e: &BytesStart<'_>) -> Result<Vec<(Vec<u8>, String)>, XmlError> {
    let mut v = Vec::new();
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(|_| XmlError::Malformed)?;
        let val = String::from_utf8_lossy(&a.value);
        v.push((a.key.as_ref().to_vec(), decode_entities(&val)));
    }
    Ok(v)
}

fn get<'a>(attrs: &'a [(Vec<u8>, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == key.as_bytes()).map(|(_, v)| v.as_str())
}

fn num_i32(attrs: &[(Vec<u8>, String)], key: &str) -> i32 {
    get(attrs, key).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}

fn num_u32(attrs: &[(Vec<u8>, String)], key: &str) -> u32 {
    get(attrs, key).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}

/// State of the `<text>` element being read.
struct LineState {
    font: FontInfo,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    spans: Vec<Span>,
}

fn finish_line(line: LineState, page: &mut Option<Page>) {
    let blank = line.spans.iter().all(|s| s.text.trim().is_empty());
    if blank {
        return;
    }
    if let Some(p) = page.as_mut() {
        p.lines.push(TextLine::new(line.spans, line.font.size, line.x, line.y, line.w, line.h));
    }
}

/// Parses `pdftohtml -xml` output.
///
/// The returned document has `total_pages == pages.len()` and `first_page`/`last_page`
/// taken from the first and last page numbers; callers (the engine) overwrite them.
pub fn parse_pdftohtml_xml(xml: &[u8]) -> Result<IntermediateDoc, XmlError> {
    let mut reader = Reader::from_reader(xml);
    {
        let cfg = reader.config_mut();
        cfg.trim_text(false);
        cfg.check_end_names = false;
    }

    let mut fonts: BTreeMap<u32, FontInfo> = BTreeMap::new();
    let mut pages: Vec<Page> = Vec::new();
    let mut page: Option<Page> = None;
    let mut line: Option<LineState> = None;
    let mut b_depth = 0u32;
    let mut i_depth = 0u32;
    let mut depth = 0usize;
    let mut saw_element = false;

    loop {
        let ev = reader.read_event().map_err(|_| XmlError::Malformed)?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(ev, Event::Empty(_));
                saw_element = true;
                let name = e.local_name();
                match name.as_ref() {
                    b"fontspec" => {
                        let a = attr_map(e)?;
                        if let Some(id) = get(&a, "id").and_then(|v| v.trim().parse::<u32>().ok()) {
                            let info = FontInfo::from_family(
                                num_u32(&a, "size"),
                                get(&a, "family").unwrap_or(""),
                            );
                            fonts.insert(id, info);
                        }
                    }
                    b"page" => {
                        if let Some(p) = page.take() {
                            pages.push(p);
                        }
                        let a = attr_map(e)?;
                        let number = num_u32(&a, "number");
                        let p = Page {
                            number,
                            width: num_i32(&a, "width"),
                            height: num_i32(&a, "height"),
                            lines: Vec::new(),
                        };
                        if is_empty {
                            pages.push(p);
                        } else {
                            page = Some(p);
                        }
                    }
                    b"text" => {
                        let a = attr_map(e)?;
                        let font = get(&a, "font")
                            .and_then(|v| v.trim().parse::<u32>().ok())
                            .and_then(|id| fonts.get(&id).copied())
                            .unwrap_or_default();
                        let st = LineState {
                            font,
                            x: num_i32(&a, "left"),
                            y: num_i32(&a, "top"),
                            w: num_i32(&a, "width"),
                            h: num_i32(&a, "height"),
                            spans: Vec::new(),
                        };
                        b_depth = 0;
                        i_depth = 0;
                        if !is_empty {
                            line = Some(st);
                        }
                    }
                    b"b" if line.is_some() && !is_empty => b_depth += 1,
                    b"i" if line.is_some() && !is_empty => i_depth += 1,
                    _ => {}
                }
                if !is_empty {
                    depth += 1;
                }
            }
            Event::End(e) => {
                if depth == 0 {
                    return Err(XmlError::Malformed);
                }
                depth -= 1;
                match e.local_name().as_ref() {
                    b"text" => {
                        if let Some(l) = line.take() {
                            finish_line(l, &mut page);
                        }
                    }
                    b"b" => b_depth = b_depth.saturating_sub(1),
                    b"i" => i_depth = i_depth.saturating_sub(1),
                    b"page" => {
                        if let Some(p) = page.take() {
                            pages.push(p);
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(ref t) => {
                if let Some(l) = line.as_mut() {
                    let raw = String::from_utf8_lossy(t.as_ref());
                    push_span(l, &decode_entities(&raw), b_depth, i_depth);
                }
            }
            Event::CData(ref c) => {
                if let Some(l) = line.as_mut() {
                    let raw = String::from_utf8_lossy(c.as_ref()).into_owned();
                    push_span(l, &raw, b_depth, i_depth);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if !saw_element || depth != 0 {
        return Err(XmlError::Malformed);
    }
    if let Some(p) = page.take() {
        pages.push(p);
    }
    if pages.is_empty() {
        return Err(XmlError::NoPages);
    }
    let first = pages.first().map_or(0, |p| p.number);
    let last = pages.last().map_or(0, |p| p.number);
    Ok(IntermediateDoc { total_pages: pages.len() as u32, first_page: first, last_page: last, pages })
}

fn push_span(l: &mut LineState, text: &str, b_depth: u32, i_depth: u32) {
    if text.is_empty() {
        return;
    }
    l.spans.push(Span {
        text: text.to_string(),
        bold: b_depth > 0 || l.font.bold,
        italic: i_depth > 0 || l.font.italic,
        mono: l.font.mono,
        size: l.font.size,
    });
}

#[cfg(test)]
mod xml_tests {
    use super::*;

    #[test]
    fn decode_named_and_numeric() {
        assert_eq!(decode_entities("a &amp; b &lt;c&gt; &quot;d&quot; &apos;"), "a & b <c> \"d\" '");
        assert_eq!(decode_entities("&#65;&#x42;&#X43;"), "ABC");
    }

    #[test]
    fn decode_is_single_pass() {
        assert_eq!(decode_entities("&amp;lt;"), "&lt;");
        assert_eq!(decode_entities("&amp;amp;"), "&amp;");
    }

    #[test]
    fn decode_invalid_references() {
        assert_eq!(decode_entities("&#0;"), "\u{FFFD}");
        assert_eq!(decode_entities("&#1;"), "\u{FFFD}");
        assert_eq!(decode_entities("&#9;"), "\t");
        assert_eq!(decode_entities("&#xD800;"), "\u{FFFD}");
        assert_eq!(decode_entities("&#x110000;"), "\u{FFFD}");
        assert_eq!(decode_entities("&#99999999;"), "\u{FFFD}");
    }

    #[test]
    fn decode_stray_and_unknown_stay_literal() {
        assert_eq!(decode_entities("a & b"), "a & b");
        assert_eq!(decode_entities("&foo; &#;"), "&foo; &#;");
        assert_eq!(decode_entities("&a&amp;"), "&a&");
        assert_eq!(decode_entities("é&amp;ü"), "é&ü");
        assert_eq!(decode_entities("&averyveryverylong;"), "&averyveryverylong;");
    }

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE pdf2xml SYSTEM "pdf2xml.dtd">
<pdf2xml producer="poppler" version="22.12.0">
<page number="1" position="absolute" top="0" left="0" height="1188" width="918">
  <fontspec id="0" size="14" family="Times" color="#000000"/>
  <fontspec id="1" size="11" family="Courier" color="#000000"/>
  <fontspec id="2" size="12" family="Helvetica-BoldOblique" color="#000000"/>
  <text top="100" left="80" width="300" height="16" font="0"><b>Judul</b></text>
  <text top="130" left="80" width="400" height="14" font="0">A &amp; B &amp;lt; <i>miring</i></text>
  <text top="150" left="80" width="400" height="14" font="1">let x = 1;</text>
  <text top="170" left="80" width="400" height="14" font="2">gaya</text>
  <text top="190" left="80" width="400" height="14" font="0">   </text>
</page>
<page number="2" position="absolute" top="0" left="0" height="1188" width="918">
</page>
</pdf2xml>
"##;

    #[test]
    fn parses_sample() {
        let doc = parse_pdftohtml_xml(SAMPLE.as_bytes()).unwrap();
        assert_eq!(doc.pages.len(), 2);
        assert_eq!(doc.pages[0].number, 1);
        assert_eq!(doc.pages[0].width, 918);
        assert!(doc.pages[1].lines.is_empty());
        let lines = &doc.pages[0].lines;
        assert_eq!(lines.len(), 4);
        assert!(lines[0].spans[0].bold && !lines[0].spans[0].italic);
        assert_eq!(lines[0].size, 14);
        assert_eq!(lines[1].text(), "A & B &lt; miring");
        assert!(lines[1].spans[1].italic && !lines[1].spans[0].italic);
        assert!(lines[2].mono && lines[2].size == 11);
        assert!(lines[3].spans[0].bold && lines[3].spans[0].italic && !lines[3].mono);
        assert!(doc.invariant_holds());
    }

    #[test]
    fn tolerates_misordered_inline_close() {
        let xml = r#"<pdf2xml><page number="1" height="10" width="10"><fontspec id="0" size="10" family="X"/><text top="1" left="1" width="1" height="1" font="0"><b><i>ab</b>cd</i></text></page></pdf2xml>"#;
        let doc = parse_pdftohtml_xml(xml.as_bytes()).unwrap();
        assert_eq!(doc.pages[0].lines[0].text(), "abcd");
    }

    #[test]
    fn errors() {
        assert_eq!(parse_pdftohtml_xml(b"not xml at all"), Err(XmlError::Malformed));
        assert_eq!(parse_pdftohtml_xml(b""), Err(XmlError::Malformed));
        assert_eq!(parse_pdftohtml_xml(b"<pdf2xml><page number=\"1\">"), Err(XmlError::Malformed));
        assert_eq!(parse_pdftohtml_xml(b"<pdf2xml></pdf2xml></x>"), Err(XmlError::Malformed));
        assert_eq!(parse_pdftohtml_xml(b"<pdf2xml></pdf2xml>"), Err(XmlError::NoPages));
    }
}

//! Minimal real-PDF builder on top of `lopdf` for tests that need poppler, plus the
//! small fixtures in `pdf2md/tests/fixtures/` (`encrypted.pdf`, `corrupt.pdf`,
//! `scanned-like.pdf`). The self-tests assert on the lopdf structure only and never
//! need poppler.
#![allow(dead_code)]
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use std::path::{Path, PathBuf};

pub(crate) const PAGE_W: f32 = 612.0;
pub(crate) const PAGE_H: f32 = 792.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PdfFont {
    Helvetica,
    HelveticaBold,
    Courier,
    TimesBold,
}

impl PdfFont {
    pub(crate) fn base_font(self) -> &'static str {
        match self {
            PdfFont::Helvetica => "Helvetica",
            PdfFont::HelveticaBold => "Helvetica-Bold",
            PdfFont::Courier => "Courier",
            PdfFont::TimesBold => "Times-Bold",
        }
    }

    fn resource_name(self) -> &'static str {
        match self {
            PdfFont::Helvetica => "F1",
            PdfFont::Courier => "F2",
            PdfFont::TimesBold => "F3",
            PdfFont::HelveticaBold => "F4",
        }
    }

    const ALL: [PdfFont; 4] = [
        PdfFont::Helvetica,
        PdfFont::Courier,
        PdfFont::TimesBold,
        PdfFont::HelveticaBold,
    ];
}

/// One line of text placed at an absolute position (PDF origin: bottom left).
#[derive(Clone, Debug)]
pub(crate) struct TextLine {
    pub text: String,
    pub font: PdfFont,
    pub size: f32,
    pub x: f32,
    pub y: f32,
}

impl TextLine {
    pub(crate) fn new(text: &str, font: PdfFont, size: f32, x: f32, y: f32) -> Self {
        Self { text: text.to_string(), font, size, x, y }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PageSpec {
    pub lines: Vec<TextLine>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PdfBuilder {
    pages: Vec<PageSpec>,
}

impl PdfBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn page(mut self, page: PageSpec) -> Self {
        self.pages.push(page);
        self
    }

    pub(crate) fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Builds the lopdf document (deterministic: no ID, no timestamps).
    pub(crate) fn build_document(&self) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let mut fonts = Dictionary::new();
        for f in PdfFont::ALL {
            let id = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => f.base_font(),
            });
            fonts.set(f.resource_name(), id);
        }
        let resources_id = doc.add_object(dictionary! { "Font" => fonts });

        let mut kids: Vec<Object> = Vec::new();
        for page in &self.pages {
            let mut ops: Vec<Operation> = Vec::new();
            for l in &page.lines {
                ops.push(Operation::new("BT", vec![]));
                ops.push(Operation::new(
                    "Tf",
                    vec![l.font.resource_name().into(), l.size.into()],
                ));
                ops.push(Operation::new("Td", vec![l.x.into(), l.y.into()]));
                ops.push(Operation::new(
                    "Tj",
                    vec![Object::String(l.text.clone().into_bytes(), StringFormat::Literal)],
                ));
                ops.push(Operation::new("ET", vec![]));
            }
            let data = Content { operations: ops }.encode().expect("encode content");
            let content_id = doc.add_object(Stream::new(dictionary! {}, data));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            });
            kids.push(page_id.into());
        }

        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => count,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    pub(crate) fn build(&self) -> Vec<u8> {
        save(&mut self.build_document())
    }
}

fn save(doc: &mut Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

/// Document with `pages` pages. Each page has a repeated header line at the top, a
/// repeated footer line at the bottom, a heading (18 pt bold) and two 11 pt Helvetica
/// body lines whose text is unique per page.
pub(crate) fn text_pdf(pages: usize, header: &str, footer: &str) -> Vec<u8> {
    let mut b = PdfBuilder::new();
    for n in 1..=pages {
        b = b.page(PageSpec {
            lines: vec![
                TextLine::new(header, PdfFont::Helvetica, 9.0, 72.0, 760.0),
                TextLine::new(&format!("Heading {n}"), PdfFont::HelveticaBold, 18.0, 72.0, 700.0),
                TextLine::new(&format!("Body line one of page {n}"), PdfFont::Helvetica, 11.0, 72.0, 660.0),
                TextLine::new(&format!("Body line two of page {n}"), PdfFont::Helvetica, 11.0, 72.0, 644.0),
                TextLine::new(footer, PdfFont::Helvetica, 9.0, 72.0, 40.0),
            ],
        });
    }
    b.build()
}

/// One page mixing a large heading, Helvetica body text and Courier code lines.
pub(crate) fn mixed_font_pdf() -> Vec<u8> {
    PdfBuilder::new()
        .page(PageSpec {
            lines: vec![
                TextLine::new("Main Title", PdfFont::TimesBold, 24.0, 72.0, 720.0),
                TextLine::new("Plain paragraph text", PdfFont::Helvetica, 11.0, 72.0, 680.0),
                TextLine::new("fn main() {}", PdfFont::Courier, 10.0, 72.0, 650.0),
                TextLine::new("    println!(1);", PdfFont::Courier, 10.0, 72.0, 638.0),
            ],
        })
        .build()
}

/// Pages that draw a rectangle only, so the text layer is empty.
pub(crate) fn scanned_like_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let ops = vec![
        Operation::new("re", vec![72.into(), 72.into(), 400.into(), 600.into()]),
        Operation::new("f", vec![]),
    ];
    let data = Content { operations: ops }.encode().expect("encode content");
    let content_id = doc.add_object(Stream::new(dictionary! {}, data));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "Resources" => dictionary! {},
            "MediaBox" => vec![0.into(), 0.into(), PAGE_W.into(), PAGE_H.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    save(&mut doc)
}

/// A PDF whose trailer carries a Standard security handler `/Encrypt` dictionary
/// (dummy O/U values). Poppler refuses it without a valid password.
pub(crate) fn encrypted_pdf() -> Vec<u8> {
    let mut doc = PdfBuilder::new()
        .page(PageSpec {
            lines: vec![TextLine::new("secret", PdfFont::Helvetica, 11.0, 72.0, 700.0)],
        })
        .build_document();
    let enc_id = doc.add_object(dictionary! {
        "Filter" => "Standard",
        "V" => 1,
        "R" => 2,
        "O" => Object::String(vec![0x11; 32], StringFormat::Hexadecimal),
        "U" => Object::String(vec![0x22; 32], StringFormat::Hexadecimal),
        "P" => -4,
    });
    doc.trailer.set("Encrypt", enc_id);
    doc.trailer.set(
        "ID",
        vec![
            Object::String(vec![0x33; 16], StringFormat::Hexadecimal),
            Object::String(vec![0x33; 16], StringFormat::Hexadecimal),
        ],
    );
    save(&mut doc)
}

/// A valid PDF cut off in the middle (no xref table, no `%%EOF`).
pub(crate) fn corrupt_pdf() -> Vec<u8> {
    let full = text_pdf(2, "Header", "Footer");
    full[..full.len() / 2].to_vec()
}

pub(crate) fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pdf2md/tests/fixtures")
}

/// Writes the three fixtures into `dir`.
pub(crate) fn write_fixtures(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("encrypted.pdf"), encrypted_pdf())?;
    std::fs::write(dir.join("corrupt.pdf"), corrupt_pdf())?;
    std::fs::write(dir.join("scanned-like.pdf"), scanned_like_pdf())?;
    Ok(())
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod self_tests {
    use super::*;

    fn page_texts(doc: &Document) -> Vec<String> {
        let mut pages: Vec<_> = doc.get_pages().into_iter().collect();
        pages.sort();
        pages
            .into_iter()
            .map(|(n, _)| doc.extract_text(&[n]).unwrap_or_default())
            .collect()
    }

    #[test]
    fn text_pdf_has_pages_header_footer_and_heading_sizes() {
        let bytes = text_pdf(3, "Running Header", "Running Footer");
        assert!(bytes.starts_with(b"%PDF-"));
        let doc = Document::load_mem(&bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
        for (_, page_id) in doc.get_pages() {
            let content = doc.get_and_decode_page_content(page_id).unwrap();
            let sizes: Vec<f32> = content
                .operations
                .iter()
                .filter(|o| o.operator == "Tf")
                .map(|o| o.operands[1].as_float().unwrap())
                .collect();
            assert!(sizes.contains(&18.0) && sizes.contains(&11.0) && sizes.contains(&9.0));
            let text: Vec<Vec<u8>> = content
                .operations
                .iter()
                .filter(|o| o.operator == "Tj")
                .map(|o| o.operands[0].as_str().unwrap().to_vec())
                .collect();
            assert!(text.contains(&b"Running Header".to_vec()));
            assert!(text.contains(&b"Running Footer".to_vec()));
        }
    }

    #[test]
    fn fonts_helvetica_and_courier_are_declared() {
        let doc = Document::load_mem(&mixed_font_pdf()).unwrap();
        let names: Vec<String> = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| d.get(b"Type").and_then(|t| t.as_name()).ok() == Some(b"Font"))
            .filter_map(|d| d.get(b"BaseFont").and_then(|b| b.as_name()).ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect();
        assert!(names.iter().any(|n| n == "Helvetica"));
        assert!(names.iter().any(|n| n == "Courier"));
        let _ = page_texts(&doc);
    }

    #[test]
    fn scanned_like_pdf_has_a_page_without_text_operators() {
        let doc = Document::load_mem(&scanned_like_pdf()).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        let (_, page_id) = doc.get_pages().into_iter().next().unwrap();
        let content = doc.get_and_decode_page_content(page_id).unwrap();
        assert!(!content.operations.iter().any(|o| o.operator == "Tj" || o.operator == "TJ"));
    }

    #[test]
    fn encrypted_pdf_declares_encrypt_dictionary() {
        let bytes = encrypted_pdf();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(contains(&bytes, b"/Encrypt"));
        assert!(contains(&bytes, b"/Standard"));
    }

    #[test]
    fn corrupt_pdf_is_truncated() {
        let bytes = corrupt_pdf();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(!contains(&bytes, b"%%EOF"));
        assert!(!contains(&bytes, b"startxref"));
    }

    #[test]
    fn committed_fixtures_match_generators() {
        let dir = fixtures_dir();
        if std::env::var("PDF_MD_WRITE_FIXTURES").as_deref() == Ok("1") {
            write_fixtures(&dir).unwrap();
        }
        assert_eq!(std::fs::read(dir.join("encrypted.pdf")).unwrap(), encrypted_pdf());
        assert_eq!(std::fs::read(dir.join("corrupt.pdf")).unwrap(), corrupt_pdf());
        assert_eq!(std::fs::read(dir.join("scanned-like.pdf")).unwrap(), scanned_like_pdf());
    }
}

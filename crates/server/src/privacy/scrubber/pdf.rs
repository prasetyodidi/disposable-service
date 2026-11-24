//! PDF scrubber: removes the Info dictionary and XMP `/Metadata` streams.
//!
//! Only metadata field *names* are recorded, never their values. Serialization is
//! deterministic: the trailer is rebuilt by hand (only `/Root` and an already
//! existing `/ID`), the xref is a classic table, and objects are written in id order,
//! so scrubbing the output again yields identical bytes.
use super::{FileKind, ScrubError, ScrubOutput};
use disposable_common::{ScrubCategoryCount, ScrubReport};
use lopdf::xref::XrefType;
use lopdf::{Dictionary, Document, Object, ObjectId};

/// Info dictionary fields reported by name, in report order.
const INFO_FIELDS: [&str; 8] = [
    "Author",
    "Creator",
    "Producer",
    "Title",
    "Subject",
    "Keywords",
    "CreationDate",
    "ModDate",
];

const NOT_PDF: &str = "bukan PDF valid";

/// What was found in a document: Info field names and XMP stream count.
struct Found {
    info_fields: Vec<&'static str>,
    xmp: usize,
}

impl Found {
    fn into_report(self) -> ScrubReport {
        let mut removed: Vec<ScrubCategoryCount> = self
            .info_fields
            .into_iter()
            .map(|f| ScrubCategoryCount {
                category: format!("pdf_info:{f}"),
                count: 1,
            })
            .collect();
        if self.xmp > 0 {
            removed.push(ScrubCategoryCount {
                category: "xmp".to_string(),
                count: self.xmp,
            });
        }
        ScrubReport {
            file_kind: FileKind::Pdf.as_str().to_string(),
            removed,
        }
    }
}

fn load(bytes: &[u8]) -> Result<Document, ScrubError> {
    if !bytes.starts_with(b"%PDF-") {
        return Err(ScrubError::Unreadable(NOT_PDF));
    }
    let doc = Document::load_mem(bytes).map_err(|e| match e {
        lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword => ScrubError::EncryptedPdf,
        _ => ScrubError::Unreadable(NOT_PDF),
    })?;
    if doc.is_encrypted() || doc.was_encrypted() || doc.trailer.has(b"Encrypt") {
        return Err(ScrubError::EncryptedPdf);
    }
    if doc.catalog().is_err() {
        return Err(ScrubError::Unreadable(NOT_PDF));
    }
    Ok(doc)
}

/// The Info dictionary's object id (if indirect) and its content.
fn info_dict(doc: &Document) -> (Option<ObjectId>, Option<&Dictionary>) {
    match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => (Some(*id), doc.get_dictionary(*id).ok()),
        Ok(Object::Dictionary(d)) => (None, Some(d)),
        _ => (None, None),
    }
}

fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().values().copied().collect()
}

fn find(doc: &Document) -> Found {
    let info_fields = match info_dict(doc).1 {
        Some(d) => INFO_FIELDS.iter().copied().filter(|f| d.has(f.as_bytes())).collect(),
        None => Vec::new(),
    };
    let mut xmp = usize::from(doc.catalog().map(|c| c.has(b"Metadata")).unwrap_or(false));
    for id in page_ids(doc) {
        if doc.get_dictionary(id).map(|d| d.has(b"Metadata")).unwrap_or(false) {
            xmp += 1;
        }
    }
    Found { info_fields, xmp }
}

pub(super) fn scrub(bytes: &[u8]) -> Result<ScrubOutput, ScrubError> {
    let mut doc = load(bytes)?;
    let report = find(&doc).into_report();

    // Info: drop from trailer and delete its object.
    let (info_id, _) = info_dict(&doc);
    doc.trailer.remove(b"Info");
    if let Some(id) = info_id {
        doc.objects.remove(&id);
    }

    // XMP: drop /Metadata from the catalog and from page objects.
    if let Ok(cat) = doc.catalog_mut() {
        cat.remove(b"Metadata");
    }
    for id in page_ids(&doc) {
        if let Ok(page) = doc.get_dictionary_mut(id) {
            page.remove(b"Metadata");
        }
    }

    // Rebuild the trailer by hand: only /Root and an existing /ID (never a new one).
    let mut trailer = Dictionary::new();
    match doc.trailer.get(b"Root") {
        Ok(root) => trailer.set("Root", root.clone()),
        Err(_) => return Err(ScrubError::Unreadable(NOT_PDF)),
    }
    if let Ok(id) = doc.trailer.get(b"ID") {
        trailer.set("ID", id.clone());
    }
    doc.trailer = trailer;
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

    doc.prune_objects();
    // Normalize max_id so /Size matches what a reload would compute (idempotence).
    doc.max_id = doc.objects.keys().map(|(id, _)| *id).max().unwrap_or(0);

    let mut out = Vec::with_capacity(bytes.len());
    doc.save_to(&mut out)
        .map_err(|_| ScrubError::Unreadable("file tidak dapat dibaca"))?;
    Ok(ScrubOutput {
        kind: FileKind::Pdf,
        bytes: out,
        report,
    })
}

pub(super) fn scan(bytes: &[u8]) -> Result<ScrubReport, ScrubError> {
    let doc = load(bytes)?;
    Ok(find(&doc).into_report())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn sample(with_meta: bool) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = doc.add_object(Stream::new(dictionary! {}, b"BT ET".to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let mut cat = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
        if with_meta {
            let xmp = doc.add_object(Stream::new(dictionary! {"Type" => "Metadata"}, b"<x/>".to_vec()));
            cat.set("Metadata", xmp);
            let info = doc.add_object(dictionary! {
                "Author" => Object::string_literal("secret"),
                "Producer" => Object::string_literal("tool"),
            });
            doc.trailer.set("Info", info);
        }
        let root = doc.add_object(cat);
        doc.trailer.set("Root", root);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn removes_metadata_and_reports_names() {
        let out = scrub(&sample(true)).unwrap();
        let names: Vec<_> = out.report.removed.iter().map(|c| c.category.as_str()).collect();
        assert_eq!(names, ["pdf_info:Author", "pdf_info:Producer", "xmp"]);
        let doc = Document::load_mem(&out.bytes).unwrap();
        assert!(!doc.trailer.has(b"Info"));
        assert!(!doc.catalog().unwrap().has(b"Metadata"));
        assert_eq!(doc.get_pages().len(), 1);
        assert!(!out.bytes.windows(6).any(|w| w == b"secret"));
        assert!(scan(&out.bytes).unwrap().removed.is_empty());
    }

    #[test]
    fn idempotent() {
        for meta in [true, false] {
            let first = scrub(&sample(meta)).unwrap();
            let second = scrub(&first.bytes).unwrap();
            assert_eq!(first.bytes, second.bytes);
            assert!(second.report.removed.is_empty());
        }
    }

    #[test]
    fn rejects_invalid() {
        assert!(matches!(scrub(b"%PDF-1.4 garbage"), Err(ScrubError::Unreadable(_))));
        assert!(matches!(scrub(b"nope"), Err(ScrubError::Unreadable(_))));
        let good = sample(true);
        assert!(matches!(scrub(&good[..good.len() / 2]), Err(ScrubError::Unreadable(_))));
    }
}

//! Core unit tests for the metadata scrubber.
//!
//! **Validates: Requirements 1.6, 1.7, 2.3, 2.5, 2.6**
use super::gen_scrub::{PngSpec};
use crate::privacy::scrubber::{scan, scrub, ScrubError, MAX_UPLOAD_BYTES};
use lopdf::{dictionary, Document, Object, Stream};

const LIMIT: usize = 26_214_400;

/// Valid PNG whose total size is exactly `total` bytes (padding lives in IDAT,
/// which the scrubber copies untouched).
fn png_of_size(total: usize) -> Vec<u8> {
    let base = PngSpec { width: 1, height: 1, items: vec![], idat: vec![] }.build().len();
    let png = PngSpec { width: 1, height: 1, items: vec![], idat: vec![7u8; total - base] }.build();
    assert_eq!(png.len(), total);
    png
}

/// PDF whose trailer references a (standard-handler style) /Encrypt dictionary.
fn encrypted_pdf() -> Vec<u8> {
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
    let root = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", root);
    let enc = doc.add_object(dictionary! {
        "Filter" => "Standard",
        "V" => 1, "R" => 2, "P" => -4,
        "O" => Object::string_literal(vec![1u8; 32]),
        "U" => Object::string_literal(vec![2u8; 32]),
    });
    doc.trailer.set("Encrypt", enc);
    doc.trailer.set(
        "ID",
        vec![
            Object::string_literal(vec![3u8; 16]),
            Object::string_literal(vec![3u8; 16]),
        ],
    );
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn encrypted_pdf_is_rejected_req_2_3() {
    let pdf = encrypted_pdf();
    assert_eq!(scrub(&pdf).unwrap_err(), ScrubError::EncryptedPdf);
    assert_eq!(scan(&pdf).unwrap_err(), ScrubError::EncryptedPdf);
}

#[test]
fn size_limit_constant_is_25_mib() {
    assert_eq!(MAX_UPLOAD_BYTES, LIMIT);
}

#[test]
fn exactly_limit_is_accepted_req_1_7_2_6() {
    let png = png_of_size(LIMIT);
    let out = scrub(&png).expect("file of exactly 26,214,400 bytes must be accepted");
    assert_eq!(out.bytes, png);
    assert!(out.report.removed.is_empty());
}

#[test]
fn one_byte_over_limit_is_too_large_req_1_7_2_6() {
    let png = png_of_size(LIMIT + 1);
    assert_eq!(scrub(&png).unwrap_err(), ScrubError::TooLarge);
    assert_eq!(scan(&png).unwrap_err(), ScrubError::TooLarge);
    // Size is checked before type: even unsupported content gives TooLarge.
    assert_eq!(scrub(&vec![0u8; LIMIT + 1]).unwrap_err(), ScrubError::TooLarge);
}

/// Documented behaviour: a 0-byte upload has no magic bytes, so its type cannot
/// be determined and it is rejected as UnsupportedType (415). Req 2.5 lists
/// 0 bytes under "not a valid PDF" (422), but the scrubber, which detects type
/// from magic bytes only, cannot know an empty upload was meant to be a PDF;
/// Req 1.5 (unknown type -> 415) applies.
#[test]
fn zero_byte_input_is_unsupported_type() {
    assert_eq!(scrub(&[]).unwrap_err(), ScrubError::UnsupportedType);
    assert_eq!(scan(&[]).unwrap_err(), ScrubError::UnsupportedType);
}

#[test]
fn corrupt_files_with_known_type_are_unreadable_req_1_6_2_5() {
    for bytes in [
        &b"%PDF-"[..],
        &b"%PDF-1.4 garbage"[..],
        &b"\x89PNG\r\n\x1a\n"[..],
        &[0xFF, 0xD8, 0xFF][..],
    ] {
        assert!(matches!(scrub(bytes), Err(ScrubError::Unreadable(_))), "{bytes:?}");
        assert!(matches!(scan(bytes), Err(ScrubError::Unreadable(_))), "{bytes:?}");
    }
}

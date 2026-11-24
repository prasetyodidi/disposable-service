// Feature: privacy-micro-apps-and-burn-workspace, Property 1: Scrub gambar menghapus metadata dan mempertahankan piksel
//
// **Validates: Requirements 1.1, 1.2, 1.9**
use proptest::prelude::*;

use super::gen_scrub::{arb_jpeg_spec, arb_png_spec, PNG_SIGNATURE};
use crate::privacy::scrubber::{scan, scrub, sniff};

/// JPEG: (SOF0 segment bytes incl. marker, bytes from SOS marker through EOI).
fn jpeg_pixels(b: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut pos = 2;
    let mut sof = Vec::new();
    loop {
        assert_eq!(b[pos], 0xFF, "marker expected at {pos}");
        let marker = b[pos + 1];
        let len = u16::from_be_bytes([b[pos + 2], b[pos + 3]]) as usize;
        if marker == 0xDA {
            return (sof, b[pos..].to_vec());
        }
        if marker == 0xC0 {
            sof = b[pos..pos + 2 + len].to_vec();
        }
        pos += 2 + len;
    }
}

/// PNG: (IHDR data, concatenated IDAT chunk data).
fn png_pixels(b: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let (mut ihdr, mut idat) = (Vec::new(), Vec::new());
    let mut pos = PNG_SIGNATURE.len();
    while pos + 12 <= b.len() {
        let len = u32::from_be_bytes([b[pos], b[pos + 1], b[pos + 2], b[pos + 3]]) as usize;
        let data = &b[pos + 8..pos + 8 + len];
        match &b[pos + 4..pos + 8] {
            b"IHDR" => ihdr = data.to_vec(),
            b"IDAT" => idat.extend_from_slice(data),
            _ => {}
        }
        pos += 12 + len;
    }
    (ihdr, idat)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn scrub_jpeg_removes_metadata_keeps_pixels(spec in arb_jpeg_spec()) {
        let input = spec.build();
        let out = scrub(&input).expect("scrub valid jpeg");
        prop_assert_eq!(sniff(&out.bytes), sniff(&input), "format must be preserved");
        let left = scan(&out.bytes).expect("scan scrubbed jpeg");
        prop_assert!(left.removed.is_empty(), "sensitive metadata left: {:?}", left.removed);
        prop_assert_eq!(jpeg_pixels(&out.bytes), jpeg_pixels(&input), "SOF/SOS..EOI must be identical");
    }

    #[test]
    fn scrub_png_removes_metadata_keeps_pixels(spec in arb_png_spec()) {
        let input = spec.build();
        let out = scrub(&input).expect("scrub valid png");
        prop_assert_eq!(sniff(&out.bytes), sniff(&input), "format must be preserved");
        let left = scan(&out.bytes).expect("scan scrubbed png");
        prop_assert!(left.removed.is_empty(), "sensitive metadata left: {:?}", left.removed);
        prop_assert_eq!(png_pixels(&out.bytes), png_pixels(&input), "IHDR/IDAT must be identical");
    }
}

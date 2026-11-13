//! Synthetic file generators (proptest strategies) for the metadata scrubber tests.
//!
//! Every generated file is structurally valid for the scrubbers in
//! `privacy/scrubber/{jpeg,png,pdf}.rs`. Each spec type can:
//! - `build()` the file bytes,
//! - `expected_report()` the `ScrubReport` the scrubber must produce for it,
//! - `clean()` itself into the same file without metadata, so that
//!   `scrub(spec.build()).bytes == spec.clean().build()` (JPEG and PNG, byte exact).
//!
//! Corruption helpers produce inputs the scrubbers must reject.
#![allow(dead_code)]

use disposable_common::{ScrubCategoryCount, ScrubReport};
use lopdf::{dictionary, Document, Object, Stream};
use proptest::prelude::*;
use std::collections::BTreeMap;

/// File format selector, used by the corruption helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Jpeg,
    Png,
    Pdf,
}

fn report(kind: &str, removed: Vec<(String, usize)>) -> ScrubReport {
    ScrubReport {
        file_kind: kind.to_string(),
        removed: removed
            .into_iter()
            .filter(|(_, c)| *c > 0)
            .map(|(category, count)| ScrubCategoryCount { category, count })
            .collect(),
    }
}

// ============================== JPEG ==============================

pub(crate) const EXIF_HEADER: &[u8] = b"Exif\0\0";
pub(crate) const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
pub(crate) const XMP_EXT_HEADER: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";

/// Which EXIF tags the synthetic APP1 EXIF segment contains.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExifSpec {
    pub little_endian: bool,
    /// IFD0 Artist (0x013B) -> author
    pub artist: bool,
    /// IFD0 Software (0x0131) -> software
    pub software: bool,
    /// IFD0 XPAuthor (0x9C9D) -> author
    pub xp_author: bool,
    /// IFD0 CameraSerialNumber (0xC62F) -> device_serial
    pub camera_serial: bool,
    /// IFD0 GPS pointer (0x8825) with a non-empty GPS IFD -> gps
    pub gps: bool,
    /// Exif sub-IFD BodySerialNumber (0xA431) -> device_serial
    pub body_serial: bool,
    /// Exif sub-IFD CameraOwnerName (0xA430) -> author
    pub owner: bool,
    /// Exif sub-IFD LensSerialNumber (0xA435) -> device_serial
    pub lens_serial: bool,
    /// IFD1 (embedded thumbnail) present -> thumbnail
    pub thumbnail: bool,
}

fn put16(out: &mut Vec<u8>, le: bool, v: u16) {
    out.extend_from_slice(&if le { v.to_le_bytes() } else { v.to_be_bytes() });
}

fn put32(out: &mut Vec<u8>, le: bool, v: u32) {
    out.extend_from_slice(&if le { v.to_le_bytes() } else { v.to_be_bytes() });
}

fn put_ifd(out: &mut Vec<u8>, le: bool, entries: &[(u16, u32)], next: u32) {
    put16(out, le, entries.len() as u16);
    for &(tag, val) in entries {
        put16(out, le, tag);
        put16(out, le, 2); // ASCII
        put32(out, le, 1);
        put32(out, le, val);
    }
    put32(out, le, next);
}

fn ifd_size(entries: usize) -> usize {
    2 + 12 * entries + 4
}

impl ExifSpec {
    /// APP1 payload: `Exif\0\0` + TIFF structure.
    pub(crate) fn build_payload(&self) -> Vec<u8> {
        let le = self.little_endian;
        let mut ifd0: Vec<(u16, u32)> = Vec::new();
        for (flag, tag) in [
            (self.artist, 0x013B),
            (self.software, 0x0131),
            (self.xp_author, 0x9C9D),
            (self.camera_serial, 0xC62F),
        ] {
            if flag {
                ifd0.push((tag, 0));
            }
        }
        let mut sub: Vec<(u16, u32)> = Vec::new();
        for (flag, tag) in [
            (self.owner, 0xA430),
            (self.body_serial, 0xA431),
            (self.lens_serial, 0xA435),
        ] {
            if flag {
                sub.push((tag, 0));
            }
        }
        let n0 = ifd0.len() + usize::from(self.gps) + usize::from(!sub.is_empty());
        let gps_off = 8 + ifd_size(n0);
        let gps_len = if self.gps { ifd_size(1) } else { 0 };
        let sub_off = gps_off + gps_len;
        let sub_len = if sub.is_empty() { 0 } else { ifd_size(sub.len()) };
        let ifd1_off = sub_off + sub_len;

        if self.gps {
            ifd0.push((0x8825, gps_off as u32));
        }
        if !sub.is_empty() {
            ifd0.push((0x8769, sub_off as u32));
        }
        ifd0.sort_by_key(|e| e.0);

        let mut t: Vec<u8> = if le { b"II*\0".to_vec() } else { b"MM\0*".to_vec() };
        put32(&mut t, le, 8);
        put_ifd(&mut t, le, &ifd0, if self.thumbnail { ifd1_off as u32 } else { 0 });
        if self.gps {
            put_ifd(&mut t, le, &[(0x0001, 0)], 0);
        }
        if !sub.is_empty() {
            sub.sort_by_key(|e| e.0);
            put_ifd(&mut t, le, &sub, 0);
        }
        if self.thumbnail {
            put_ifd(&mut t, le, &[(0x0201, 0)], 0);
        }
        let mut p = EXIF_HEADER.to_vec();
        p.extend(t);
        p
    }
}

/// A synthetic JPEG: kept structure (APP0, DQT, SOF0, DHT, SOS, entropy data, EOI)
/// plus removable metadata segments inserted after `meta_slot` kept header segments.
#[derive(Debug, Clone)]
pub(crate) struct JpegSpec {
    /// Keep an APP2 (ICC profile) segment, which must survive scrubbing.
    pub app2: bool,
    /// Keep a DRI segment.
    pub dri: bool,
    pub exif: Option<ExifSpec>,
    /// One XMP APP1 per entry; `true` uses the extension header.
    pub xmp: Vec<bool>,
    pub iptc: usize,
    pub comments: Vec<Vec<u8>>,
    pub app12: usize,
    pub app15: usize,
    /// APP1 segments that are neither EXIF nor XMP (category `other_metadata`).
    pub other_app1: usize,
    /// Entropy-coded data with 0xFF byte stuffing (never contains a marker).
    pub entropy: Vec<u8>,
    pub meta_slot: usize,
}

fn seg(marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, marker];
    v.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(payload);
    v
}

impl JpegSpec {
    fn kept_headers(&self) -> Vec<Vec<u8>> {
        let mut kept = vec![seg(0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0")];
        if self.app2 {
            kept.push(seg(0xE2, b"ICC_PROFILE\0\x01\x01profile-bytes"));
        }
        let mut dqt = vec![0u8];
        dqt.extend((1..=64u8).collect::<Vec<_>>());
        kept.push(seg(0xDB, &dqt));
        kept.push(seg(0xC0, &[8, 0, 1, 0, 1, 1, 1, 0x11, 0]));
        let mut dht = vec![0x00u8, 1];
        dht.extend([0u8; 15]);
        dht.push(0);
        kept.push(seg(0xC4, &dht));
        if self.dri {
            kept.push(seg(0xDD, &[0, 4]));
        }
        kept
    }

    fn metadata_block(&self) -> Vec<u8> {
        let mut m = Vec::new();
        if let Some(e) = &self.exif {
            m.extend(seg(0xE1, &e.build_payload()));
        }
        for &ext in &self.xmp {
            let mut p = if ext { XMP_EXT_HEADER } else { XMP_HEADER }.to_vec();
            p.extend_from_slice(b"<x:xmpmeta/>");
            m.extend(seg(0xE1, &p));
        }
        for _ in 0..self.iptc {
            m.extend(seg(0xED, b"Photoshop 3.0\0"));
        }
        for _ in 0..self.app12 {
            m.extend(seg(0xEC, b"Ducky"));
        }
        for _ in 0..self.app15 {
            m.extend(seg(0xEF, b"app15"));
        }
        for _ in 0..self.other_app1 {
            m.extend(seg(0xE1, b"Other\0data"));
        }
        for c in &self.comments {
            m.extend(seg(0xFE, c));
        }
        m
    }

    pub(crate) fn build(&self) -> Vec<u8> {
        let kept = self.kept_headers();
        let slot = self.meta_slot.min(kept.len());
        let mut out = vec![0xFF, 0xD8];
        for (i, s) in kept.iter().enumerate() {
            if i == slot {
                out.extend(self.metadata_block());
            }
            out.extend_from_slice(s);
        }
        if slot == kept.len() {
            out.extend(self.metadata_block());
        }
        out.extend(seg(0xDA, &[1, 1, 0, 0, 63, 0]));
        out.extend_from_slice(&self.entropy);
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }

    /// Same file without any removable metadata (expected scrub output, byte exact).
    pub(crate) fn clean(&self) -> JpegSpec {
        JpegSpec {
            exif: None,
            xmp: vec![],
            iptc: 0,
            comments: vec![],
            app12: 0,
            app15: 0,
            other_app1: 0,
            ..self.clone()
        }
    }

    pub(crate) fn expected_report(&self) -> ScrubReport {
        let mut c: BTreeMap<&str, usize> = BTreeMap::new();
        if let Some(e) = &self.exif {
            c.insert("gps", usize::from(e.gps));
            c.insert(
                "device_serial",
                [e.camera_serial, e.body_serial, e.lens_serial].iter().filter(|b| **b).count(),
            );
            c.insert(
                "author",
                [e.artist, e.xp_author, e.owner].iter().filter(|b| **b).count(),
            );
            c.insert("software", usize::from(e.software));
            c.insert("thumbnail", usize::from(e.thumbnail));
        }
        c.insert("xmp", self.xmp.len());
        c.insert("iptc", self.iptc);
        c.insert("comment", self.comments.len());
        c.insert("other_metadata", self.app12 + self.app15 + self.other_app1);
        // Same fixed order as the JPEG scrubber's report.
        const ORDER: [&str; 9] = [
            "gps",
            "device_serial",
            "author",
            "software",
            "thumbnail",
            "xmp",
            "iptc",
            "comment",
            "other_metadata",
        ];
        report(
            "jpeg",
            ORDER
                .iter()
                .map(|k| ((*k).to_string(), c.get(k).copied().unwrap_or(0)))
                .collect(),
        )
    }

    /// True when the spec carries any removable metadata segment.
    pub(crate) fn has_metadata(&self) -> bool {
        self.exif.is_some()
            || !self.xmp.is_empty()
            || self.iptc + self.app12 + self.app15 + self.other_app1 > 0
            || !self.comments.is_empty()
    }
}

pub(crate) fn arb_exif_spec() -> impl Strategy<Value = ExifSpec> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(little_endian, artist, software, xp_author, camera_serial, gps, body_serial, owner, lens_serial, thumbnail)| {
                ExifSpec {
                    little_endian,
                    artist,
                    software,
                    xp_author,
                    camera_serial,
                    gps,
                    body_serial,
                    owner,
                    lens_serial,
                    thumbnail,
                }
            },
        )
}

/// Entropy-coded data: random bytes with 0xFF stuffed as 0xFF 0x00.
fn arb_entropy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..256).prop_map(|raw| {
        let mut v = Vec::with_capacity(raw.len() + 4);
        for b in raw {
            v.push(b);
            if b == 0xFF {
                v.push(0x00);
            }
        }
        v
    })
}

pub(crate) fn arb_jpeg_spec() -> impl Strategy<Value = JpegSpec> {
    (
        any::<bool>(),
        any::<bool>(),
        prop::option::of(arb_exif_spec()),
        prop::collection::vec(any::<bool>(), 0..=2),
        0..=2usize,
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..32), 0..=3),
        0..=2usize,
        0..=2usize,
        0..=2usize,
        arb_entropy(),
        0..=6usize,
    )
        .prop_map(
            |(app2, dri, exif, xmp, iptc, comments, app12, app15, other_app1, entropy, meta_slot)| JpegSpec {
                app2,
                dri,
                exif,
                xmp,
                iptc,
                comments,
                app12,
                app15,
                other_app1,
                entropy,
                meta_slot,
            },
        )
}

// ============================== PNG ==============================

pub(crate) const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

/// CRC-32 (PNG/zlib polynomial) over the concatenation of `parts`.
pub(crate) fn png_crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &b in *part {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
            }
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// One chunk: length + type + data + valid CRC.
pub(crate) fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = (data.len() as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(data);
    v.extend_from_slice(&png_crc32(&[kind, data]).to_be_bytes());
    v
}

/// A chunk between IHDR and the final IDAT/IEND.
#[derive(Debug, Clone)]
pub(crate) enum PngItem {
    /// Chunk the scrubber must keep (gAMA, pHYs, sRGB, PLTE, bKGD, cHRM, IDAT).
    Keep { kind: [u8; 4], data: Vec<u8> },
    Exif(Vec<u8>),
    Text { keyword: String, text: Vec<u8> },
    ZText { keyword: String, data: Vec<u8> },
    /// iTXt with keyword `XML:com.adobe.xmp` when `xmp`, otherwise a plain keyword.
    IText { xmp: bool, body: Vec<u8> },
    Time([u8; 7]),
}

impl PngItem {
    fn chunk(&self) -> Vec<u8> {
        match self {
            PngItem::Keep { kind, data } => png_chunk(kind, data),
            PngItem::Exif(d) => png_chunk(b"eXIf", d),
            PngItem::Text { keyword, text } => {
                let mut d = keyword.as_bytes().to_vec();
                d.push(0);
                d.extend_from_slice(text);
                png_chunk(b"tEXt", &d)
            }
            PngItem::ZText { keyword, data } => {
                let mut d = keyword.as_bytes().to_vec();
                d.extend_from_slice(&[0, 0]);
                d.extend_from_slice(data);
                png_chunk(b"zTXt", &d)
            }
            PngItem::IText { xmp, body } => {
                let mut d = if *xmp { XMP_KEYWORD.to_vec() } else { b"Comment".to_vec() };
                d.extend_from_slice(&[0, 0, 0, 0, 0]);
                d.extend_from_slice(body);
                png_chunk(b"iTXt", &d)
            }
            PngItem::Time(t) => png_chunk(b"tIME", t),
        }
    }

    fn category(&self) -> Option<&'static str> {
        match self {
            PngItem::Keep { .. } => None,
            PngItem::Exif(_) => Some("exif"),
            PngItem::Text { .. } | PngItem::ZText { .. } => Some("text"),
            PngItem::IText { xmp: true, .. } => Some("xmp"),
            PngItem::IText { xmp: false, .. } => Some("text"),
            PngItem::Time(_) => Some("timestamp"),
        }
    }
}

/// A synthetic PNG: IHDR, `items`, a final IDAT, IEND. IDAT contents are random
/// bytes (the scrubber never decodes pixels).
#[derive(Debug, Clone)]
pub(crate) struct PngSpec {
    pub width: u32,
    pub height: u32,
    pub items: Vec<PngItem>,
    pub idat: Vec<u8>,
}

impl PngSpec {
    pub(crate) fn build(&self) -> Vec<u8> {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&self.width.to_be_bytes());
        ihdr.extend_from_slice(&self.height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut out = PNG_SIGNATURE.to_vec();
        out.extend(png_chunk(b"IHDR", &ihdr));
        for it in &self.items {
            out.extend(it.chunk());
        }
        out.extend(png_chunk(b"IDAT", &self.idat));
        out.extend(png_chunk(b"IEND", &[]));
        out
    }

    /// Same file without removable metadata chunks (expected scrub output, byte exact).
    pub(crate) fn clean(&self) -> PngSpec {
        PngSpec {
            items: self.items.iter().filter(|i| i.category().is_none()).cloned().collect(),
            ..self.clone()
        }
    }

    pub(crate) fn expected_report(&self) -> ScrubReport {
        let mut c: BTreeMap<&str, usize> = BTreeMap::new();
        for it in &self.items {
            if let Some(cat) = it.category() {
                *c.entry(cat).or_insert(0) += 1;
            }
        }
        // BTreeMap iteration = alphabetical, same as the PNG scrubber's report.
        report("png", c.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    pub(crate) fn has_metadata(&self) -> bool {
        self.items.iter().any(|i| i.category().is_some())
    }
}

fn arb_keyword() -> impl Strategy<Value = String> {
    "[A-Za-z0-9]{1,16}"
}

fn arb_png_item() -> impl Strategy<Value = PngItem> {
    let keep_kind = prop::sample::select(vec![*b"gAMA", *b"pHYs", *b"sRGB", *b"PLTE", *b"bKGD", *b"cHRM", *b"IDAT"]);
    let bytes = |max: usize| prop::collection::vec(any::<u8>(), 0..max);
    prop_oneof![
        (keep_kind, bytes(32)).prop_map(|(kind, data)| PngItem::Keep { kind, data }),
        bytes(64).prop_map(PngItem::Exif),
        (arb_keyword(), bytes(32)).prop_map(|(keyword, text)| PngItem::Text { keyword, text }),
        (arb_keyword(), bytes(32)).prop_map(|(keyword, data)| PngItem::ZText { keyword, data }),
        (any::<bool>(), bytes(32)).prop_map(|(xmp, body)| PngItem::IText { xmp, body }),
        any::<[u8; 7]>().prop_map(PngItem::Time),
    ]
}

pub(crate) fn arb_png_spec() -> impl Strategy<Value = PngSpec> {
    (
        1..=64u32,
        1..=64u32,
        prop::collection::vec(arb_png_item(), 0..10),
        prop::collection::vec(any::<u8>(), 1..128),
    )
        .prop_map(|(width, height, items, idat)| PngSpec { width, height, items, idat })
}

// ============================== PDF ==============================

/// Info dictionary fields the scrubber reports by name, in report order.
pub(crate) const PDF_INFO_FIELDS: [&str; 8] = [
    "Author",
    "Creator",
    "Producer",
    "Title",
    "Subject",
    "Keywords",
    "CreationDate",
    "ModDate",
];

#[derive(Debug, Clone)]
pub(crate) struct PdfPageSpec {
    /// Alphanumeric text (no PDF escaping needed).
    pub text: String,
    /// Page dictionary has a `/Metadata` stream.
    pub xmp: bool,
}

/// A synthetic PDF built with `lopdf` (classic xref, unencrypted).
#[derive(Debug, Clone)]
pub(crate) struct PdfSpec {
    pub pages: Vec<PdfPageSpec>,
    /// Info dictionary entries (subset of `PDF_INFO_FIELDS`); empty = no Info dictionary.
    /// Values start with `SENS`, so tests can search the bytes for leaked values.
    pub info: Vec<(&'static str, String)>,
    /// Catalog has a `/Metadata` stream.
    pub catalog_xmp: bool,
}

impl PdfSpec {
    pub(crate) fn build(&self) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let mut kids: Vec<Object> = Vec::new();
        for p in &self.pages {
            let content = format!("BT /F1 12 Tf 72 720 Td ({}) Tj ET", p.text);
            let content = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
            let mut page = dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => content,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            };
            if p.xmp {
                let xmp = doc.add_object(Stream::new(dictionary! {"Type" => "Metadata"}, b"<x:xmpmeta/>".to_vec()));
                page.set("Metadata", xmp);
            }
            kids.push(doc.add_object(page).into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
        );
        let mut cat = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
        if self.catalog_xmp {
            let xmp = doc.add_object(Stream::new(dictionary! {"Type" => "Metadata"}, b"<x:xmpmeta/>".to_vec()));
            cat.set("Metadata", xmp);
        }
        if !self.info.is_empty() {
            let mut info = lopdf::Dictionary::new();
            for (k, v) in &self.info {
                info.set(*k, Object::string_literal(v.as_str()));
            }
            let info = doc.add_object(info);
            doc.trailer.set("Info", info);
        }
        let root = doc.add_object(cat);
        doc.trailer.set("Root", root);
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("lopdf serialization of a synthetic PDF");
        out
    }

    pub(crate) fn expected_report(&self) -> ScrubReport {
        let mut removed: Vec<(String, usize)> = PDF_INFO_FIELDS
            .iter()
            .filter(|f| self.info.iter().any(|(k, _)| k == *f))
            .map(|f| (format!("pdf_info:{f}"), 1))
            .collect();
        let xmp = usize::from(self.catalog_xmp) + self.pages.iter().filter(|p| p.xmp).count();
        removed.push(("xmp".to_string(), xmp));
        report("pdf", removed)
    }

    pub(crate) fn has_metadata(&self) -> bool {
        !self.info.is_empty() || self.catalog_xmp || self.pages.iter().any(|p| p.xmp)
    }
}

pub(crate) fn arb_pdf_spec() -> impl Strategy<Value = PdfSpec> {
    let page = ("[A-Za-z0-9 ]{0,30}", any::<bool>()).prop_map(|(text, xmp)| PdfPageSpec { text, xmp });
    let info = prop::collection::vec(prop::option::of("SENS[A-Z0-9]{8,12}"), PDF_INFO_FIELDS.len()).prop_map(|vals| {
        PDF_INFO_FIELDS
            .iter()
            .zip(vals)
            .filter_map(|(k, v)| v.map(|v| (*k, v)))
            .collect::<Vec<_>>()
    });
    (prop::collection::vec(page, 1..=5), info, any::<bool>()).prop_map(|(pages, info, catalog_xmp)| PdfSpec {
        pages,
        info,
        catalog_xmp,
    })
}

// ============================== Corruption ==============================

/// A way to make a valid file unreadable.
#[derive(Debug, Clone)]
pub(crate) enum Corruption {
    /// Cut the file short. The magic bytes are always kept, so the result still
    /// sniffs as its format and the scrubber must answer `Unreadable`.
    /// (PDF is cut to at most half so lopdf cannot recover from a missing xref.)
    Truncate(usize),
    /// XOR one of the magic bytes (JPEG: first 3, PNG: first 8, PDF: first 5) with a
    /// non-zero mask. The result no longer sniffs, so the scrubber answers `UnsupportedType`.
    Header { idx: usize, xor: u8 },
    /// PNG only: flip a bit in the CRC of one chunk (answer: `Unreadable`).
    /// For JPEG/PDF this behaves like `Truncate`.
    BadCrc(usize),
}

pub(crate) fn arb_corruption() -> impl Strategy<Value = Corruption> {
    prop_oneof![
        any::<usize>().prop_map(Corruption::Truncate),
        (any::<usize>(), 1..=255u8).prop_map(|(idx, xor)| Corruption::Header { idx, xor }),
        any::<usize>().prop_map(Corruption::BadCrc),
    ]
}

fn magic_len(f: Format) -> usize {
    match f {
        Format::Jpeg => 3,
        Format::Png => 8,
        Format::Pdf => 5,
    }
}

fn truncate(f: Format, bytes: &[u8], seed: usize) -> Vec<u8> {
    let min = magic_len(f);
    let upper = match f {
        Format::Pdf => bytes.len() / 2,
        _ => bytes.len() - 1,
    };
    if upper <= min {
        return bytes[..min.min(bytes.len())].to_vec();
    }
    bytes[..min + seed % (upper - min + 1)].to_vec()
}

/// Apply `c` to a valid file of format `f` (see `Corruption` for the expected error).
pub(crate) fn corrupt(f: Format, bytes: &[u8], c: &Corruption) -> Vec<u8> {
    match c {
        Corruption::Truncate(seed) => truncate(f, bytes, *seed),
        Corruption::Header { idx, xor } => {
            let mut v = bytes.to_vec();
            v[idx % magic_len(f)] ^= xor;
            v
        }
        Corruption::BadCrc(seed) if f == Format::Png => {
            // Walk the chunks to find the CRC offsets.
            let mut crcs = Vec::new();
            let mut pos = PNG_SIGNATURE.len();
            while pos + 12 <= bytes.len() {
                let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
                crcs.push(pos + 8 + len);
                pos += 12 + len;
            }
            let mut v = bytes.to_vec();
            let n = crcs.len();
            let at = crcs[seed % n] + (seed / n) % 4;
            v[at] ^= 0x01;
            v
        }
        Corruption::BadCrc(seed) => truncate(f, bytes, *seed),
    }
}

// ============================== Non-format bytes ==============================

/// Random bytes that do not start with the JPEG, PNG or PDF magic (`sniff` is `None`).
pub(crate) fn arb_non_format_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..2048)
        .prop_filter("must not look like a supported format", |b| {
            crate::privacy::scrubber::sniff(b).is_none()
        })
}

#[cfg(test)]
mod self_tests {
    use super::*;
    use crate::privacy::scrubber::{scrub, ScrubError};

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn generated_jpeg_png_match_scrubber(j in arb_jpeg_spec(), p in arb_png_spec()) {
            let out = scrub(&j.build()).unwrap();
            prop_assert_eq!(&out.report, &j.expected_report());
            prop_assert_eq!(out.bytes, j.clean().build());
            let out = scrub(&p.build()).unwrap();
            prop_assert_eq!(&out.report, &p.expected_report());
            prop_assert_eq!(out.bytes, p.clean().build());
        }

        #[test]
        fn generated_pdf_matches_scrubber(s in arb_pdf_spec()) {
            let out = scrub(&s.build()).unwrap();
            prop_assert_eq!(&out.report, &s.expected_report());
        }

        #[test]
        fn corruptions_and_garbage_are_rejected(
            j in arb_jpeg_spec(), p in arb_png_spec(), d in arb_pdf_spec(),
            c in arb_corruption(), junk in arb_non_format_bytes(),
        ) {
            for (f, bytes) in [(Format::Jpeg, j.build()), (Format::Png, p.build()), (Format::Pdf, d.build())] {
                let bad = corrupt(f, &bytes, &c);
                let expected = match c {
                    Corruption::Header { .. } => ScrubError::UnsupportedType,
                    _ => ScrubError::Unreadable(""),
                };
                match (scrub(&bad), expected) {
                    (Err(ScrubError::UnsupportedType), ScrubError::UnsupportedType) => {}
                    (Err(ScrubError::Unreadable(_)), ScrubError::Unreadable(_)) => {}
                    (other, _) => prop_assert!(false, "unexpected result: {:?}", other.map(|o| o.kind)),
                }
            }
            prop_assert_eq!(scrub(&junk).unwrap_err(), ScrubError::UnsupportedType);
        }
    }
}

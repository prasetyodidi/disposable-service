//! JPEG scrubber working at marker-segment level (no pixel decode/re-encode).
//!
//! Removed: APP1 (EXIF/XMP), APP12, APP13 (IPTC), APP15, COM.
//! Kept: APP0, APP2 and every other segment, plus the entropy-coded data
//! after SOS, copied verbatim. Only category names and counts are recorded,
//! never metadata values.

use super::{FileKind, ScrubError, ScrubOutput};
use disposable_common::{ScrubCategoryCount, ScrubReport};

const MARKER_SOI: u8 = 0xD8;
const MARKER_EOI: u8 = 0xD9;
const MARKER_SOS: u8 = 0xDA;
const MARKER_APP1: u8 = 0xE1;
const MARKER_APP12: u8 = 0xEC;
const MARKER_APP13: u8 = 0xED;
const MARKER_APP15: u8 = 0xEF;
const MARKER_COM: u8 = 0xFE;

const EXIF_HEADER: &[u8] = b"Exif\0\0";
const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_EXT_HEADER: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";

/// Fixed output order of categories so reports are deterministic.
const CATEGORY_ORDER: [&str; 9] = [
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

#[derive(Default)]
struct Counts {
    gps: usize,
    device_serial: usize,
    author: usize,
    software: usize,
    thumbnail: usize,
    xmp: usize,
    iptc: usize,
    comment: usize,
    other: usize,
}

impl Counts {
    fn into_report(self) -> ScrubReport {
        let values = [
            self.gps,
            self.device_serial,
            self.author,
            self.software,
            self.thumbnail,
            self.xmp,
            self.iptc,
            self.comment,
            self.other,
        ];
        let removed = CATEGORY_ORDER
            .iter()
            .zip(values.iter())
            .filter(|(_, &c)| c > 0)
            .map(|(name, &count)| ScrubCategoryCount {
                category: (*name).to_string(),
                count,
            })
            .collect();
        ScrubReport {
            file_kind: FileKind::Jpeg.as_str().to_string(),
            removed,
        }
    }
}

const TRUNCATED: ScrubError = ScrubError::Unreadable("jpeg is truncated or invalid");

/// Walk the segments; returns the cleaned bytes and the category counts.
fn process(bytes: &[u8]) -> Result<(Vec<u8>, Counts), ScrubError> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != MARKER_SOI {
        return Err(TRUNCATED);
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..2]);
    let mut counts = Counts::default();
    let mut i = 2usize;

    loop {
        // Expect a marker: 0xFF (optionally padded with more 0xFF) then code.
        if i >= bytes.len() || bytes[i] != 0xFF {
            return Err(TRUNCATED);
        }
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            return Err(TRUNCATED);
        }
        let m = bytes[i];
        i += 1;
        match m {
            0x00 | MARKER_SOI | MARKER_EOI => return Err(TRUNCATED),
            0x01 | 0xD0..=0xD7 => {
                // Standalone marker without length.
                out.extend_from_slice(&[0xFF, m]);
                continue;
            }
            _ => {}
        }
        if i + 2 > bytes.len() {
            return Err(TRUNCATED);
        }
        let len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        if len < 2 || i + len > bytes.len() {
            return Err(TRUNCATED);
        }
        let payload = &bytes[i + 2..i + len];

        if m == MARKER_SOS {
            // Copy header and everything after it verbatim; require an EOI.
            let rest = &bytes[i + len..];
            if !rest.windows(2).any(|w| w == [0xFF, MARKER_EOI]) {
                return Err(TRUNCATED);
            }
            out.extend_from_slice(&[0xFF, m]);
            out.extend_from_slice(&bytes[i..]);
            return Ok((out, counts));
        }

        match m {
            MARKER_APP1 => classify_app1(payload, &mut counts),
            MARKER_APP13 => counts.iptc += 1,
            MARKER_COM => counts.comment += 1,
            MARKER_APP12 | MARKER_APP15 => counts.other += 1,
            _ => out.extend_from_slice(&bytes[i - 2..i + len]),
        }
        i += len;
    }
}

fn classify_app1(payload: &[u8], counts: &mut Counts) {
    if payload.starts_with(EXIF_HEADER) {
        count_exif(&payload[EXIF_HEADER.len()..], counts);
    } else if payload.starts_with(XMP_HEADER) || payload.starts_with(XMP_EXT_HEADER) {
        counts.xmp += 1;
    } else {
        counts.other += 1;
    }
}

// ---------- Minimal TIFF/IFD parser (counts only, no values stored) ----------

struct Tiff<'a> {
    data: &'a [u8],
    le: bool,
}

impl<'a> Tiff<'a> {
    fn u16(&self, off: usize) -> Option<u16> {
        let b: [u8; 2] = self.data.get(off..off.checked_add(2)?)?.try_into().ok()?;
        Some(if self.le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    }

    fn u32(&self, off: usize) -> Option<u32> {
        let b: [u8; 4] = self.data.get(off..off.checked_add(4)?)?.try_into().ok()?;
        Some(if self.le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    }

    /// Returns (entries as (tag, value-field u32), next IFD offset).
    fn ifd(&self, off: usize) -> Option<(Vec<(u16, u32)>, u32)> {
        let n = self.u16(off)? as usize;
        let mut entries = Vec::with_capacity(n.min(512));
        for k in 0..n {
            let e = off.checked_add(2 + k * 12)?;
            let tag = self.u16(e)?;
            let val = self.u32(e + 8)?;
            entries.push((tag, val));
        }
        let next = self.u32(off.checked_add(2 + n * 12)?).unwrap_or(0);
        Some((entries, next))
    }
}

fn count_exif(tiff: &[u8], counts: &mut Counts) {
    let le = match tiff.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return,
    };
    let t = Tiff { data: tiff, le };
    let Some(ifd0_off) = t.u32(4) else { return };
    let Some((ifd0, next)) = t.ifd(ifd0_off as usize) else { return };

    scan_ifd(&t, &ifd0, counts, 0);

    // IFD1 holds the embedded thumbnail.
    if next != 0 {
        if let Some((ifd1, _)) = t.ifd(next as usize) {
            if !ifd1.is_empty() {
                counts.thumbnail += 1;
            }
        }
    }
}

fn scan_ifd(t: &Tiff, entries: &[(u16, u32)], counts: &mut Counts, depth: u8) {
    for &(tag, val) in entries {
        match tag {
            0x013B | 0x9C9D | 0xA430 => counts.author += 1, // Artist, XPAuthor, CameraOwnerName
            0x0131 => counts.software += 1,                 // Software
            0xC62F | 0xA431 | 0xA435 => counts.device_serial += 1, // Camera/Body/Lens serial
            0x8825 if depth == 0 => {
                // GPS IFD pointer: count once if it has any entries.
                if let Some((gps, _)) = t.ifd(val as usize) {
                    if !gps.is_empty() {
                        counts.gps += 1;
                    }
                }
            }
            0x8769 if depth == 0 => {
                // Exif sub-IFD.
                if let Some((sub, _)) = t.ifd(val as usize) {
                    scan_ifd(t, &sub, counts, depth + 1);
                }
            }
            _ => {}
        }
    }
}

// ---------- Public (super) API ----------

pub(super) fn scrub(bytes: &[u8]) -> Result<ScrubOutput, ScrubError> {
    let (out, counts) = process(bytes)?;
    Ok(ScrubOutput {
        kind: FileKind::Jpeg,
        bytes: out,
        report: counts.into_report(),
    })
}

/// Counts sensitive metadata still present in `bytes`.
pub(super) fn scan(bytes: &[u8]) -> Result<ScrubReport, ScrubError> {
    let (_, counts) = process(bytes)?;
    Ok(counts.into_report())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(m: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0xFF, m];
        v.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        v.extend_from_slice(payload);
        v
    }

    fn exif() -> Vec<u8> {
        // Little-endian TIFF: IFD0 {Software, GPS ptr}, GPS IFD {1 entry}, IFD1 {1 entry}
        let mut t = b"II*\0".to_vec();
        t.extend_from_slice(&8u32.to_le_bytes());
        let ifd = |entries: &[(u16, u32)], next: u32| {
            let mut b = (entries.len() as u16).to_le_bytes().to_vec();
            for (tag, val) in entries {
                b.extend_from_slice(&tag.to_le_bytes());
                b.extend_from_slice(&2u16.to_le_bytes());
                b.extend_from_slice(&1u32.to_le_bytes());
                b.extend_from_slice(&val.to_le_bytes());
            }
            b.extend_from_slice(&next.to_le_bytes());
            b
        };
        // IFD0 at 8: 2 entries -> 2+24+4 = 30 bytes; GPS at 38 (2+12+4=18); IFD1 at 56
        t.extend(ifd(&[(0x0131, 0), (0x8825, 38)], 56));
        t.extend(ifd(&[(0x0001, 0)], 0));
        t.extend(ifd(&[(0x0201, 0)], 0));
        let mut p = EXIF_HEADER.to_vec();
        p.extend(t);
        p
    }

    fn sample() -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        v.extend(seg(0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0"));
        v.extend(seg(MARKER_APP1, &exif()));
        v.extend(seg(MARKER_APP1, XMP_HEADER));
        v.extend(seg(MARKER_APP13, b"Photoshop 3.0\0"));
        v.extend(seg(MARKER_COM, b"hello"));
        v.extend(seg(0xDB, &[0; 65]));
        v.extend(seg(MARKER_SOS, &[1, 1, 0, 0, 63, 0]));
        v.extend_from_slice(&[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD9]);
        v
    }

    #[test]
    fn scrubs_and_reports_categories() {
        let out = scrub(&sample()).unwrap();
        let names: Vec<_> = out.report.removed.iter().map(|c| c.category.as_str()).collect();
        assert_eq!(names, ["gps", "software", "thumbnail", "xmp", "iptc", "comment"]);
        assert!(scan(&out.bytes).unwrap().removed.is_empty());
        assert_eq!(scrub(&out.bytes).unwrap().bytes, out.bytes);
        assert!(out.bytes.windows(2).any(|w| w == [0xFF, 0xE0]));
        assert!(out.bytes.ends_with(&[0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD9]));
    }

    #[test]
    fn truncated_is_unreadable() {
        let s = sample();
        assert!(matches!(scrub(&s[..s.len() - 2]), Err(ScrubError::Unreadable(_))));
        assert!(matches!(scrub(&s[..30]), Err(ScrubError::Unreadable(_))));
    }
}

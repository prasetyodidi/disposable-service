//! PNG scrubber: operates on chunks only, never decodes or re-encodes pixels.
//!
//! Removes `eXIf`, `tEXt`, `zTXt`, `iTXt`, `tIME`. All other chunks (including
//! IDAT) are copied byte-for-byte. `iTXt` chunks whose keyword is
//! `XML:com.adobe.xmp` are reported as `xmp`. Bytes after IEND are dropped.

use super::{FileKind, ScrubError, ScrubOutput};
use disposable_common::{ScrubCategoryCount, ScrubReport};
use std::collections::BTreeMap;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";
const MAX_CHUNK_LEN: usize = 0x7FFF_FFFF;

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
    /// Whole chunk: length + type + data + crc.
    raw: &'a [u8],
}

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for &b in *part {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// Parse and validate the chunk stream (signature, CRC, IHDR first, IEND present).
/// Anything after IEND is ignored.
fn parse(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, ScrubError> {
    const BAD: ScrubError = ScrubError::Unreadable("file tidak dapat dibaca");
    if !bytes.starts_with(&SIGNATURE) {
        return Err(BAD);
    }
    let mut chunks = Vec::new();
    let mut pos = SIGNATURE.len();
    loop {
        let header = bytes.get(pos..pos + 8).ok_or(BAD)?;
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        if len > MAX_CHUNK_LEN {
            return Err(BAD);
        }
        let kind = [header[4], header[5], header[6], header[7]];
        if !kind.iter().all(|b| b.is_ascii_alphabetic()) {
            return Err(BAD);
        }
        let end = pos
            .checked_add(12)
            .and_then(|v| v.checked_add(len))
            .ok_or(BAD)?;
        let raw = bytes.get(pos..end).ok_or(BAD)?;
        let data = &raw[8..8 + len];
        let stored = u32::from_be_bytes([
            raw[8 + len],
            raw[9 + len],
            raw[10 + len],
            raw[11 + len],
        ]);
        if crc32(&[&kind, data]) != stored {
            return Err(BAD);
        }
        if chunks.is_empty() && &kind != b"IHDR" {
            return Err(BAD);
        }
        let is_end = &kind == b"IEND";
        chunks.push(Chunk { kind, data, raw });
        pos = end;
        if is_end {
            return Ok(chunks);
        }
    }
}

/// Metadata category for a chunk, or None when the chunk is kept.
fn category(c: &Chunk<'_>) -> Option<&'static str> {
    match &c.kind {
        b"eXIf" => Some("exif"),
        b"tIME" => Some("timestamp"),
        b"tEXt" | b"zTXt" => Some("text"),
        b"iTXt" => {
            let keyword = c.data.split(|&b| b == 0).next().unwrap_or(&[]);
            if keyword == XMP_KEYWORD {
                Some("xmp")
            } else {
                Some("text")
            }
        }
        _ => None,
    }
}

fn report(chunks: &[Chunk<'_>]) -> ScrubReport {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for c in chunks {
        if let Some(cat) = category(c) {
            *counts.entry(cat).or_insert(0) += 1;
        }
    }
    ScrubReport {
        file_kind: FileKind::Png.as_str().to_string(),
        removed: counts
            .into_iter()
            .map(|(category, count)| ScrubCategoryCount {
                category: category.to_string(),
                count,
            })
            .collect(),
    }
}

pub(super) fn scrub(bytes: &[u8]) -> Result<ScrubOutput, ScrubError> {
    let chunks = parse(bytes)?;
    let report = report(&chunks);
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&SIGNATURE);
    for c in chunks.iter().filter(|c| category(c).is_none()) {
        out.extend_from_slice(c.raw);
    }
    Ok(ScrubOutput {
        kind: FileKind::Png,
        bytes: out,
        report,
    })
}

pub(super) fn scan(bytes: &[u8]) -> Result<ScrubReport, ScrubError> {
    Ok(report(&parse(bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = (data.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(data);
        v.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
        v
    }

    fn png(chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut v = SIGNATURE.to_vec();
        for c in chunks {
            v.extend_from_slice(c);
        }
        v
    }

    fn ihdr() -> Vec<u8> {
        chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0])
    }

    #[test]
    fn crc_known_value() {
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }

    #[test]
    fn removes_metadata_keeps_rest_and_reports() {
        let idat = chunk(b"IDAT", &[1, 2, 3, 4]);
        let gama = chunk(b"gAMA", &[0, 0, 0, 1]);
        let input = png(&[
            ihdr(),
            chunk(b"eXIf", b"Exif\0\0"),
            gama.clone(),
            chunk(b"tEXt", b"Author\0Bob"),
            chunk(b"iTXt", b"XML:com.adobe.xmp\0\0\0\0\0<x/>"),
            chunk(b"iTXt", b"Comment\0\0\0\0\0hi"),
            chunk(b"tIME", &[7, 0xE8, 1, 1, 0, 0, 0]),
            idat.clone(),
            chunk(b"IEND", &[]),
        ]);
        let out = scrub(&input).unwrap();
        assert_eq!(out.bytes, png(&[ihdr(), gama, idat, chunk(b"IEND", &[])]));
        let got: Vec<_> = out
            .report
            .removed
            .iter()
            .map(|c| (c.category.as_str(), c.count))
            .collect();
        assert_eq!(got, vec![("exif", 1), ("text", 2), ("timestamp", 1), ("xmp", 1)]);
        assert!(scan(&out.bytes).unwrap().removed.is_empty());
        assert_eq!(scrub(&out.bytes).unwrap().bytes, out.bytes);
    }

    #[test]
    fn rejects_bad_crc_truncation_and_bad_structure() {
        let good = png(&[ihdr(), chunk(b"IEND", &[])]);
        assert!(scrub(&good).is_ok());
        let mut bad = good.clone();
        let n = bad.len();
        bad[n - 1] ^= 1;
        assert!(matches!(scrub(&bad), Err(ScrubError::Unreadable(_))));
        assert!(scrub(&good[..good.len() - 3]).is_err());
        assert!(scrub(&good[..8]).is_err());
        assert!(scrub(&png(&[chunk(b"IEND", &[])])).is_err());
        assert!(scrub(&png(&[ihdr()])).is_err());
    }
}

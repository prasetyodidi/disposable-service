//! Metadata scrubber: pure in-memory functions for JPEG, PNG and PDF.

use disposable_common::ScrubReport;

mod jpeg;
mod pdf;
mod png;

/// Maximum accepted upload size: 25 MiB (26_214_400 bytes).
pub const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Jpeg,
    Png,
    Pdf,
}

impl FileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileKind::Jpeg => "jpeg",
            FileKind::Png => "png",
            FileKind::Pdf => "pdf",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrubError {
    /// 413
    TooLarge,
    /// 415
    UnsupportedType,
    /// 422, static reason without user data
    Unreadable(&'static str),
    /// 422
    EncryptedPdf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrubOutput {
    pub kind: FileKind,
    pub bytes: Vec<u8>,
    pub report: ScrubReport,
}

/// Detect the file kind from magic bytes only (never from name or Content-Type).
pub fn sniff(bytes: &[u8]) -> Option<FileKind> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(FileKind::Jpeg)
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(FileKind::Png)
    } else if bytes.starts_with(b"%PDF-") {
        Some(FileKind::Pdf)
    } else {
        None
    }
}

/// Validate size and type: size (413) -> type (415).
fn detect(bytes: &[u8]) -> Result<FileKind, ScrubError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ScrubError::TooLarge);
    }
    sniff(bytes).ok_or(ScrubError::UnsupportedType)
}

/// Remove sensitive metadata. Order: size (413) -> type (415) -> parse (422).
pub fn scrub(bytes: &[u8]) -> Result<ScrubOutput, ScrubError> {
    match detect(bytes)? {
        FileKind::Jpeg => jpeg::scrub(bytes),
        FileKind::Png => png::scrub(bytes),
        FileKind::Pdf => pdf::scrub(bytes),
    }
}

/// Count sensitive metadata still present in `bytes` (for verification/tests).
pub fn scan(bytes: &[u8]) -> Result<ScrubReport, ScrubError> {
    match detect(bytes)? {
        FileKind::Jpeg => jpeg::scan(bytes),
        FileKind::Png => png::scan(bytes),
        FileKind::Pdf => pdf::scan(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_uses_magic_bytes() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(FileKind::Jpeg));
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n"), Some(FileKind::Png));
        assert_eq!(sniff(b"%PDF-1.4"), Some(FileKind::Pdf));
        assert_eq!(sniff(b"hello"), None);
    }

    #[test]
    fn rejects_oversize_before_type_check() {
        let big = vec![0u8; MAX_UPLOAD_BYTES + 1];
        assert_eq!(scrub(&big).unwrap_err(), ScrubError::TooLarge);
        assert_eq!(scan(&big).unwrap_err(), ScrubError::TooLarge);
    }

    #[test]
    fn rejects_unsupported_type() {
        assert_eq!(scrub(b"plain text").unwrap_err(), ScrubError::UnsupportedType);
        assert_eq!(scan(b"").unwrap_err(), ScrubError::UnsupportedType);
    }

    #[test]
    fn dispatches_by_sniffed_kind() {
        // Minimal PNG signature only: png parser runs (not UnsupportedType).
        let r = scrub(b"\x89PNG\r\n\x1a\n");
        assert!(!matches!(r, Err(ScrubError::UnsupportedType)));
        let r = scrub(&[0xFF, 0xD8, 0xFF]);
        assert!(!matches!(r, Err(ScrubError::UnsupportedType)));
        let r = scrub(b"%PDF-1.4 garbage");
        assert!(!matches!(r, Err(ScrubError::UnsupportedType)));
    }
}

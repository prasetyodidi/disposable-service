//! Poppler extraction engine: `pdfinfo` (page count, early classification)
//! followed by `pdftohtml -xml`. stderr is classified in memory and never logged.
use super::engine::{ExtractRequest, ExtractionEngine};
use super::error::EngineError;
use super::model::IntermediateDoc;
use super::options::{resolve_range, RangeError};
use super::proc::{run_bounded, ProcError, ProcSpec};
use super::xml::parse_pdftohtml_xml;
use std::ffi::OsString;
use std::path::Path;

const PDFINFO_MAX_STDOUT: usize = 64 * 1024;
const STDERR_CAP: usize = 8 * 1024;

pub struct PopplerEngine {
    pdftohtml: String,
    pdfinfo: String,
}

impl Default for PopplerEngine {
    fn default() -> Self {
        Self::with_binaries("pdftohtml", "pdfinfo")
    }
}

impl PopplerEngine {
    /// Engine with explicit executable names/paths (used by tests).
    pub fn with_binaries(pdftohtml: &str, pdfinfo: &str) -> Self {
        Self {
            pdftohtml: pdftohtml.to_string(),
            pdfinfo: pdfinfo.to_string(),
        }
    }
}

/// Arguments of `pdfinfo`: only the server-created input path.
pub(crate) fn pdfinfo_args(pdf_path: &Path) -> Vec<OsString> {
    vec![pdf_path.as_os_str().to_os_string()]
}

/// Arguments of `pdftohtml`: `-xml -i -q -stdout -f <from> -l <last> <pdf_path>`.
pub(crate) fn pdftohtml_args(from: u32, last: u32, pdf_path: &Path) -> Vec<OsString> {
    let mut v: Vec<OsString> = ["-xml", "-i", "-q", "-stdout", "-f"]
        .iter()
        .map(OsString::from)
        .collect();
    v.push(from.to_string().into());
    v.push("-l".into());
    v.push(last.to_string().into());
    v.push(pdf_path.as_os_str().to_os_string());
    v
}

/// Extracts N from the `Pages:   N` line of `pdfinfo` output.
pub(crate) fn parse_pages_line(stdout: &[u8]) -> Option<u32> {
    let text = String::from_utf8_lossy(stdout);
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            let n: u32 = rest.trim().parse().ok()?;
            return if n >= 1 { Some(n) } else { None };
        }
    }
    None
}

/// `Encrypted` for password errors, otherwise `Corrupt`. Pure, case-insensitive.
pub(crate) fn classify_stderr(snippet: &[u8]) -> EngineError {
    let s = String::from_utf8_lossy(snippet).to_lowercase();
    if s.contains("incorrect password") || (s.contains("password") && s.contains("encrypted")) {
        EngineError::Encrypted
    } else {
        EngineError::Corrupt
    }
}

pub(crate) fn map_proc(e: ProcError) -> EngineError {
    match e {
        ProcError::NotFound | ProcError::SpawnFailed => EngineError::Unavailable,
        ProcError::Timeout => EngineError::Timeout,
        ProcError::OutputTooLarge => EngineError::OutputTooLarge,
        ProcError::Io => EngineError::Corrupt,
    }
}

#[axum::async_trait]
impl ExtractionEngine for PopplerEngine {
    fn name(&self) -> &'static str {
        "poppler"
    }

    async fn extract(&self, req: &ExtractRequest<'_>) -> Result<IntermediateDoc, EngineError> {
        let info = run_bounded(
            &ProcSpec {
                program: &self.pdfinfo,
                args: pdfinfo_args(req.pdf_path),
                work_dir: req.work_dir,
                max_stdout: PDFINFO_MAX_STDOUT,
                max_stderr: STDERR_CAP,
            },
            &req.deadline,
        )
        .await
        .map_err(map_proc)?;
        if !info.status_ok {
            return Err(classify_stderr(&info.stderr_snippet));
        }
        let total = parse_pages_line(&info.stdout).ok_or(EngineError::Corrupt)?;

        let (first, last, _warning) = resolve_range(req.page_from, req.page_to, total, req.max_pages)
            .map_err(|e| match e {
                RangeError::FromBeyondTotal(n) => EngineError::PageFromOutOfRange { total_pages: n },
                RangeError::TooManyPages => EngineError::TooManyPages,
            })?;

        let out = run_bounded(
            &ProcSpec {
                program: &self.pdftohtml,
                args: pdftohtml_args(first, last, req.pdf_path),
                work_dir: req.work_dir,
                max_stdout: req.max_output_bytes,
                max_stderr: STDERR_CAP,
            },
            &req.deadline,
        )
        .await
        .map_err(map_proc)?;
        if !out.status_ok {
            return Err(classify_stderr(&out.stderr_snippet));
        }
        let mut doc = parse_pdftohtml_xml(&out.stdout).map_err(|_| EngineError::Corrupt)?;
        doc.total_pages = total;
        doc.first_page = first;
        doc.last_page = last;
        Ok(doc)
    }
}

#[cfg(test)]
mod poppler_tests {
    use super::*;
    use crate::pdf2md::proc::Deadline;
    use std::time::Duration;

    #[test]
    fn pdftohtml_args_are_exact() {
        let a = pdftohtml_args(2, 9, Path::new("/w/input.pdf"));
        let a: Vec<_> = a.iter().map(|s| s.to_str().unwrap()).collect();
        assert_eq!(
            a,
            ["-xml", "-i", "-q", "-stdout", "-f", "2", "-l", "9", "/w/input.pdf"]
        );
        assert_eq!(pdfinfo_args(Path::new("/w/input.pdf")).len(), 1);
    }

    #[test]
    fn pages_line_parsing() {
        let out = b"Title:  x\nPages:          12\nEncrypted: no\n";
        assert_eq!(parse_pages_line(out), Some(12));
        assert_eq!(parse_pages_line(b"Title: x\n"), None);
        assert_eq!(parse_pages_line(b"Pages: abc\n"), None);
        assert_eq!(parse_pages_line(b"Pages: 0\n"), None);
    }

    #[test]
    fn stderr_classification() {
        assert_eq!(
            classify_stderr(b"Command Line Error: Incorrect password"),
            EngineError::Encrypted
        );
        assert_eq!(
            classify_stderr(b"This file is ENCRYPTED; a Password is required"),
            EngineError::Encrypted
        );
        assert_eq!(classify_stderr(b"Syntax Error: Couldn't read xref"), EngineError::Corrupt);
        assert_eq!(classify_stderr(b"May not be a PDF file"), EngineError::Corrupt);
        assert_eq!(classify_stderr(b""), EngineError::Corrupt);
        assert_eq!(classify_stderr(&[0xff, 0xfe, 0x00]), EngineError::Corrupt);
    }

    #[test]
    fn proc_errors_map() {
        assert_eq!(map_proc(ProcError::NotFound), EngineError::Unavailable);
        assert_eq!(map_proc(ProcError::SpawnFailed), EngineError::Unavailable);
        assert_eq!(map_proc(ProcError::Timeout), EngineError::Timeout);
        assert_eq!(map_proc(ProcError::OutputTooLarge), EngineError::OutputTooLarge);
        assert_eq!(map_proc(ProcError::Io), EngineError::Corrupt);
    }

    #[tokio::test]
    async fn missing_binaries_are_unavailable() {
        let dir = std::env::temp_dir().join(format!("pdf2md-poppler-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("input.pdf");
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();
        let eng = PopplerEngine::with_binaries("tidak-ada-xyz", "tidak-ada-xyz");
        let req = ExtractRequest {
            pdf_path: &pdf,
            work_dir: &dir,
            page_from: 1,
            page_to: None,
            max_pages: 300,
            deadline: Deadline::new(Duration::from_secs(5)),
            max_output_bytes: 1024,
        };
        let r = eng.extract(&req).await;
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(r.unwrap_err(), EngineError::Unavailable);
    }
}

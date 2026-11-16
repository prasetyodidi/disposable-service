//! Error types and the error-code table (HTTP status, `code` string, priority).
//!
//! Messages are static: they never contain file names, document content,
//! filesystem paths or subprocess stderr.
use axum::http::StatusCode;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    Unauthorized,
    BurnInProgress,
    FileTooLarge,
    NotAPdf,
    BadRequest,
    FileRequired,
    BadOptions,
    PageFromOutOfRange,
    CorruptPdf,
    EncryptedPdf,
    NoTextLayer,
    TooManyPages,
    OutputTooLarge,
    Timeout,
    EngineUnavailable,
    Internal,
}

impl ErrorCode {
    pub fn http_status(self) -> StatusCode {
        use ErrorCode::*;
        match self {
            Unauthorized => StatusCode::UNAUTHORIZED,
            BurnInProgress | EngineUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            FileTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            NotAPdf => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            BadRequest | FileRequired | BadOptions | PageFromOutOfRange => StatusCode::BAD_REQUEST,
            CorruptPdf | EncryptedPdf | NoTextLayer | TooManyPages | OutputTooLarge => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Timeout => StatusCode::GATEWAY_TIMEOUT,
            Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code_str(self) -> &'static str {
        use ErrorCode::*;
        match self {
            Unauthorized => "unauthorized",
            BurnInProgress => "burn_in_progress",
            FileTooLarge => "file_too_large",
            NotAPdf => "not_a_pdf",
            BadRequest => "invalid_request",
            FileRequired => "file_required",
            BadOptions => "invalid_options",
            PageFromOutOfRange => "page_from_out_of_range",
            CorruptPdf => "corrupt_pdf",
            EncryptedPdf => "encrypted_pdf",
            NoTextLayer => "no_text_layer",
            TooManyPages => "too_many_pages",
            OutputTooLarge => "output_too_large",
            Timeout => "timeout",
            EngineUnavailable => "engine_unavailable",
            Internal => "internal_error",
        }
    }

    /// Precedence order of Requirement 2.12 (lower value wins). `EngineUnavailable`
    /// and `Internal` sit outside the validation order and get 8 and 9.
    pub fn priority(self) -> u8 {
        use ErrorCode::*;
        match self {
            Unauthorized => 1,
            BurnInProgress => 2,
            FileTooLarge => 3,
            NotAPdf => 4,
            BadRequest | FileRequired | BadOptions | PageFromOutOfRange => 5,
            CorruptPdf | EncryptedPdf | NoTextLayer | TooManyPages | OutputTooLarge => 6,
            Timeout => 7,
            EngineUnavailable => 8,
            Internal => 9,
        }
    }
}

/// Errors reported by an extraction engine (Requirement 7.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    Corrupt,
    Encrypted,
    NoTextLayer,
    Timeout,
    TooManyPages,
    /// The executable is missing or cannot be started.
    Unavailable,
    /// Engine output exceeded the output limit.
    OutputTooLarge,
    /// `page_from` is greater than the document's page count.
    PageFromOutOfRange { total_pages: u32 },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EngineError::Corrupt => "corrupt",
            EngineError::Encrypted => "encrypted",
            EngineError::NoTextLayer => "no_text_layer",
            EngineError::Timeout => "timeout",
            EngineError::TooManyPages => "too_many_pages",
            EngineError::Unavailable => "engine_unavailable",
            EngineError::OutputTooLarge => "output_too_large",
            EngineError::PageFromOutOfRange { .. } => "page_from_out_of_range",
        })
    }
}

impl std::error::Error for EngineError {}

/// Errors returned by `convert()` and the HTTP/MCP adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    FileTooLarge { max_bytes: u64 },
    NotAPdf,
    BadRequest,
    FileRequired,
    /// `field` is the option name (or `options`); `reason` is a static description.
    BadOptions { field: String, reason: String },
    PageFromOutOfRange { total_pages: u32 },
    CorruptPdf,
    EncryptedPdf,
    NoTextLayer,
    TooManyPages,
    OutputTooLarge,
    Timeout,
    EngineUnavailable,
    Internal,
}

impl From<EngineError> for ConvertError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::Corrupt => ConvertError::CorruptPdf,
            EngineError::Encrypted => ConvertError::EncryptedPdf,
            EngineError::NoTextLayer => ConvertError::NoTextLayer,
            EngineError::Timeout => ConvertError::Timeout,
            EngineError::TooManyPages => ConvertError::TooManyPages,
            EngineError::Unavailable => ConvertError::EngineUnavailable,
            EngineError::OutputTooLarge => ConvertError::OutputTooLarge,
            EngineError::PageFromOutOfRange { total_pages } => {
                ConvertError::PageFromOutOfRange { total_pages }
            }
        }
    }
}

impl ConvertError {
    pub fn code(&self) -> ErrorCode {
        match self {
            ConvertError::FileTooLarge { .. } => ErrorCode::FileTooLarge,
            ConvertError::NotAPdf => ErrorCode::NotAPdf,
            ConvertError::BadRequest => ErrorCode::BadRequest,
            ConvertError::FileRequired => ErrorCode::FileRequired,
            ConvertError::BadOptions { .. } => ErrorCode::BadOptions,
            ConvertError::PageFromOutOfRange { .. } => ErrorCode::PageFromOutOfRange,
            ConvertError::CorruptPdf => ErrorCode::CorruptPdf,
            ConvertError::EncryptedPdf => ErrorCode::EncryptedPdf,
            ConvertError::NoTextLayer => ErrorCode::NoTextLayer,
            ConvertError::TooManyPages => ErrorCode::TooManyPages,
            ConvertError::OutputTooLarge => ErrorCode::OutputTooLarge,
            ConvertError::Timeout => ErrorCode::Timeout,
            ConvertError::EngineUnavailable => ErrorCode::EngineUnavailable,
            ConvertError::Internal => ErrorCode::Internal,
        }
    }

    pub fn http_status(&self) -> StatusCode {
        self.code().http_status()
    }

    pub fn code_str(&self) -> &'static str {
        self.code().code_str()
    }

    /// Static Indonesian message. Contains only limits, option names and page counts.
    pub fn message(&self) -> String {
        match self {
            ConvertError::FileTooLarge { max_bytes } => {
                format!("Ukuran file melebihi batas. Batas ukuran file adalah {max_bytes} byte.")
            }
            ConvertError::NotAPdf => "File bukan PDF yang valid.".into(),
            ConvertError::BadRequest => "Permintaan tidak valid.".into(),
            ConvertError::FileRequired => "Bagian multipart 'file' wajib diisi.".into(),
            ConvertError::BadOptions { field, reason } => {
                format!("Opsi tidak valid pada '{field}': {reason}.")
            }
            ConvertError::PageFromOutOfRange { total_pages } => format!(
                "page_from melebihi jumlah halaman dokumen. Dokumen memiliki {total_pages} halaman."
            ),
            ConvertError::CorruptPdf => "PDF rusak atau tidak dapat dibaca.".into(),
            ConvertError::EncryptedPdf => {
                "PDF dilindungi kata sandi dan tidak dapat dikonversi.".into()
            }
            ConvertError::NoTextLayer => {
                "PDF tidak memiliki lapisan teks (mungkin hasil pindai); OCR tidak didukung.".into()
            }
            ConvertError::TooManyPages => {
                "Jumlah halaman yang diminta melebihi batas 300 halaman. Gunakan page_from dan page_to."
                    .into()
            }
            ConvertError::OutputTooLarge => {
                "Hasil konversi terlalu besar. Gunakan page_from dan page_to untuk memperkecil rentang halaman."
                    .into()
            }
            ConvertError::Timeout => "Konversi melebihi batas waktu.".into(),
            ConvertError::EngineUnavailable => "Mesin ekstraksi PDF tidak tersedia.".into(),
            ConvertError::Internal => "Terjadi kesalahan internal.".into(),
        }
    }
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for ConvertError {}

#[cfg(test)]
mod unit {
    use super::*;

    const ALL: [ErrorCode; 16] = [
        ErrorCode::Unauthorized,
        ErrorCode::BurnInProgress,
        ErrorCode::FileTooLarge,
        ErrorCode::NotAPdf,
        ErrorCode::BadRequest,
        ErrorCode::FileRequired,
        ErrorCode::BadOptions,
        ErrorCode::PageFromOutOfRange,
        ErrorCode::CorruptPdf,
        ErrorCode::EncryptedPdf,
        ErrorCode::NoTextLayer,
        ErrorCode::TooManyPages,
        ErrorCode::OutputTooLarge,
        ErrorCode::Timeout,
        ErrorCode::EngineUnavailable,
        ErrorCode::Internal,
    ];

    #[test]
    fn table_matches_design() {
        assert_eq!(ErrorCode::OutputTooLarge.http_status().as_u16(), 422);
        assert_eq!(ErrorCode::PageFromOutOfRange.http_status().as_u16(), 400);
        assert_eq!(ErrorCode::EngineUnavailable.http_status().as_u16(), 503);
        assert_eq!(ErrorCode::Timeout.http_status().as_u16(), 504);
        assert_eq!(ErrorCode::Internal.code_str(), "internal_error");
        let mut codes: Vec<_> = ALL.iter().map(|c| c.code_str()).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), ALL.len());
        assert!(ErrorCode::FileTooLarge.priority() < ErrorCode::NotAPdf.priority());
        assert!(ErrorCode::BadOptions.priority() < ErrorCode::CorruptPdf.priority());
        assert!(ErrorCode::CorruptPdf.priority() < ErrorCode::Timeout.priority());
    }

    #[test]
    fn engine_errors_map_and_messages_are_static() {
        let e: ConvertError = EngineError::PageFromOutOfRange { total_pages: 7 }.into();
        assert_eq!(e.http_status().as_u16(), 400);
        assert!(e.message().contains('7'));
        assert_eq!(ConvertError::from(EngineError::Unavailable).code_str(), "engine_unavailable");
        assert!(ConvertError::FileTooLarge { max_bytes: 64 }.message().contains("64"));
    }
}

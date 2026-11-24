//! Runtime configuration for the PDF to Markdown mini app.
//!
//! `PdfConfig::from_env_with` is pure: the environment reader is injected so
//! tests never touch the process environment.
use std::fmt;
use std::time::Duration;

pub const ABSOLUTE_MAX_BYTES: u64 = 26_214_400; // 25 MiB, hard upper bound
pub const LAMBDA_DEFAULT_MAX_BYTES: u64 = 5_242_880; // 5 MiB
pub const MAX_PAGES: u32 = 300;
pub const MULTIPART_OVERHEAD: u64 = 65_536;
pub const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_OUTPUT_BYTES: usize = 104_857_600; // 100 MiB
pub const DOWNLOAD_TTL_SECS: u64 = 300;
pub const LAMBDA_DEFAULT_MAX_RESPONSE_BYTES: u64 = 5_767_168; // 5.5 MiB

pub const ENV_MAX_BYTES: &str = "PDF_MD_MAX_BYTES";
pub const ENV_MAX_RESPONSE_BYTES: &str = "PDF_MD_MAX_RESPONSE_BYTES";
const ENV_LAMBDA: &str = "AWS_LAMBDA_FUNCTION_NAME";

#[derive(Debug, Clone, PartialEq)]
pub struct PdfConfig {
    pub max_bytes: u64,
    pub max_pages: u32,
    pub subprocess_timeout: Duration,
    pub max_output_bytes: usize,
    /// `None` means unlimited (non-Lambda servers).
    pub max_response_bytes: Option<u64>,
    /// Logged once at startup.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// Message names `PDF_MD_MAX_BYTES`.
    MaxBytesInvalid(String),
    /// Message names `PDF_MD_MAX_RESPONSE_BYTES`.
    MaxResponseBytesInvalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MaxBytesInvalid(m) | ConfigError::MaxResponseBytesInvalid(m) => {
                f.write_str(m)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Accepts only `^[0-9]+$`, value >= 1 and fitting in `u64`.
fn parse_positive_u64(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u64>().ok().filter(|v| *v >= 1)
}

fn is_lambda(get: &dyn Fn(&str) -> Option<String>) -> bool {
    get(ENV_LAMBDA).is_some_and(|v| !v.is_empty())
}

impl PdfConfig {
    pub fn from_env_with(get: &dyn Fn(&str) -> Option<String>) -> Result<PdfConfig, ConfigError> {
        let lambda = is_lambda(get);
        let mut warnings = Vec::new();

        let default_max = if lambda {
            LAMBDA_DEFAULT_MAX_BYTES
        } else {
            ABSOLUTE_MAX_BYTES
        };
        let max_bytes = match get(ENV_MAX_BYTES).as_deref() {
            None | Some("") => default_max,
            Some(raw) => match parse_positive_u64(raw) {
                Some(v) if v > ABSOLUTE_MAX_BYTES => {
                    warnings.push(format!(
                        "{ENV_MAX_BYTES} melebihi {ABSOLUTE_MAX_BYTES}; memakai {ABSOLUTE_MAX_BYTES}"
                    ));
                    ABSOLUTE_MAX_BYTES
                }
                Some(v) => v,
                None => {
                    return Err(ConfigError::MaxBytesInvalid(format!(
                        "{ENV_MAX_BYTES} tidak valid: harus bilangan bulat positif (hanya digit 0-9, minimal 1)."
                    )))
                }
            },
        };

        let max_response_bytes = match get(ENV_MAX_RESPONSE_BYTES).as_deref() {
            None | Some("") => lambda.then_some(LAMBDA_DEFAULT_MAX_RESPONSE_BYTES),
            Some(raw) => match parse_positive_u64(raw) {
                Some(v) => Some(v),
                None => {
                    return Err(ConfigError::MaxResponseBytesInvalid(format!(
                        "{ENV_MAX_RESPONSE_BYTES} tidak valid: harus bilangan bulat positif (hanya digit 0-9, minimal 1)."
                    )))
                }
            },
        };

        Ok(PdfConfig {
            max_bytes,
            max_pages: MAX_PAGES,
            subprocess_timeout: SUBPROCESS_TIMEOUT,
            max_output_bytes: MAX_OUTPUT_BYTES,
            max_response_bytes,
            warnings,
        })
    }

    /// Configuration with no environment overrides.
    pub fn defaults() -> PdfConfig {
        PdfConfig::from_env_with(&|_| None).expect("defaults are valid")
    }

    pub fn body_limit(&self) -> usize {
        self.max_bytes.saturating_add(MULTIPART_OVERHEAD) as usize
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> Result<PdfConfig, ConfigError> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        PdfConfig::from_env_with(&move |k| {
            owned.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
        })
    }

    #[test]
    fn defaults_and_lambda() {
        let c = cfg(&[]).unwrap();
        assert_eq!(c.max_bytes, 26_214_400);
        assert_eq!(c.max_response_bytes, None);
        assert_eq!(c.body_limit(), 26_214_400 + 65_536);
        let l = cfg(&[("AWS_LAMBDA_FUNCTION_NAME", "f"), ("PDF_MD_MAX_BYTES", "")]).unwrap();
        assert_eq!(l.max_bytes, 5_242_880);
        assert_eq!(l.max_response_bytes, Some(5_767_168));
        assert_eq!(cfg(&[("AWS_LAMBDA_FUNCTION_NAME", "")]).unwrap().max_bytes, 26_214_400);
    }

    #[test]
    fn valid_clamped_and_invalid() {
        assert_eq!(cfg(&[("PDF_MD_MAX_BYTES", "64")]).unwrap().max_bytes, 64);
        let c = cfg(&[("PDF_MD_MAX_BYTES", "99999999999")]).unwrap();
        assert_eq!(c.max_bytes, 26_214_400);
        assert_eq!(c.warnings.len(), 1);
        for bad in ["0", "-1", "1.5", "10MB", " 5", "abc", " ", "99999999999999999999999"] {
            let e = cfg(&[("PDF_MD_MAX_BYTES", bad)]).unwrap_err();
            assert!(e.to_string().contains("PDF_MD_MAX_BYTES"), "{bad}");
        }
    }

    #[test]
    fn response_limit() {
        assert_eq!(
            cfg(&[("PDF_MD_MAX_RESPONSE_BYTES", "99999999999")]).unwrap().max_response_bytes,
            Some(99_999_999_999)
        );
        let e = cfg(&[("PDF_MD_MAX_RESPONSE_BYTES", "0")]).unwrap_err();
        assert!(e.to_string().contains("PDF_MD_MAX_RESPONSE_BYTES"));
    }
}

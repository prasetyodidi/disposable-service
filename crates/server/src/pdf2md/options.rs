//! Conversion options parsing and page range resolution (pure functions).

use serde_json::Value;

pub const MAX_OPTIONS_BYTES: usize = 4096;
const KNOWN_FIELDS: [&str; 4] = ["page_breaks", "page_from", "page_to", "strip_headers_footers"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionOptions {
    pub page_from: u32,
    pub page_to: Option<u32>,
    pub page_breaks: bool,
    pub strip_headers_footers: bool,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        ConversionOptions { page_from: 1, page_to: None, page_breaks: false, strip_headers_footers: true }
    }
}

/// `field` is the offending field name, or `"options"` when the JSON as a whole is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsError {
    pub field: String,
    pub reason: &'static str,
}

impl std::fmt::Display for OptionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

fn err(field: &str, reason: &'static str) -> OptionsError {
    OptionsError { field: field.to_string(), reason }
}

fn parse_page(field: &str, v: &Value) -> Result<u32, OptionsError> {
    match v {
        Value::Number(n) => match n.as_u64() {
            Some(x) if x >= 1 => Ok(x.min(u32::MAX as u64) as u32),
            _ => Err(err(field, "harus bilangan bulat >= 1")),
        },
        _ => Err(err(field, "harus bilangan bulat >= 1")),
    }
}

fn parse_bool(field: &str, v: &Value) -> Result<bool, OptionsError> {
    v.as_bool().ok_or_else(|| err(field, "harus boolean"))
}

pub fn parse_options(raw: Option<&[u8]>) -> Result<ConversionOptions, OptionsError> {
    let mut opts = ConversionOptions::default();
    let raw = match raw {
        None => return Ok(opts),
        Some(r) if r.is_empty() => return Ok(opts),
        Some(r) => r,
    };
    if raw.len() > MAX_OPTIONS_BYTES {
        return Err(err("options", "terlalu besar"));
    }
    let value: Value = serde_json::from_slice(raw).map_err(|_| err("options", "JSON tidak valid"))?;
    let obj = value.as_object().ok_or_else(|| err("options", "harus objek JSON"))?;

    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    for k in &keys {
        if !KNOWN_FIELDS.contains(&k.as_str()) {
            return Err(err(k, "field tidak dikenal"));
        }
    }
    for k in keys {
        let v = &obj[k];
        match k.as_str() {
            "page_from" => opts.page_from = parse_page(k, v)?,
            "page_to" => opts.page_to = Some(parse_page(k, v)?),
            "page_breaks" => opts.page_breaks = parse_bool(k, v)?,
            "strip_headers_footers" => opts.strip_headers_footers = parse_bool(k, v)?,
            _ => unreachable!(),
        }
    }
    if let Some(to) = opts.page_to {
        if opts.page_from > to {
            return Err(err("page_from", "tidak boleh lebih besar dari page_to"));
        }
    }
    Ok(opts)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    /// `page_from` is beyond the document; carries the total page count.
    FromBeyondTotal(u32),
    TooManyPages,
}

/// Warning emitted when `page_to` was clamped to the document length.
pub type Warning = String;

/// Resolves the effective `(first, last)` range. `to > total` yields exactly one warning.
pub fn resolve_range(
    from: u32,
    to: Option<u32>,
    total: u32,
    max_pages: u32,
) -> Result<(u32, u32, Option<Warning>), RangeError> {
    if from > total {
        return Err(RangeError::FromBeyondTotal(total));
    }
    let last = to.unwrap_or(total).min(total);
    let warn = match to {
        Some(t) if t > total => {
            Some(format!("page_to ({t}) melebihi jumlah halaman ({total}); disesuaikan menjadi {total}"))
        }
        _ => None,
    };
    if (last - from) as u64 + 1 > max_pages as u64 {
        return Err(RangeError::TooManyPages);
    }
    Ok((from, last, warn))
}

#[cfg(test)]
mod options_tests {
    use super::*;

    fn p(s: &str) -> Result<ConversionOptions, OptionsError> {
        parse_options(Some(s.as_bytes()))
    }

    #[test]
    fn defaults() {
        assert_eq!(parse_options(None).unwrap(), ConversionOptions::default());
        assert_eq!(parse_options(Some(b"")).unwrap(), ConversionOptions::default());
        assert_eq!(p("{}").unwrap(), ConversionOptions::default());
    }

    #[test]
    fn valid_values() {
        let o = p(r#"{"page_from":2,"page_to":5,"page_breaks":true,"strip_headers_footers":false}"#).unwrap();
        assert_eq!(
            o,
            ConversionOptions { page_from: 2, page_to: Some(5), page_breaks: true, strip_headers_footers: false }
        );
        assert_eq!(p(r#"{"page_to":99999999999}"#).unwrap().page_to, Some(u32::MAX));
    }

    #[test]
    fn invalid_values_name_field() {
        for (s, f) in [
            ("not json", "options"),
            ("[1]", "options"),
            (r#"{"bogus":1}"#, "bogus"),
            (r#"{"page_from":0}"#, "page_from"),
            (r#"{"page_from":-1}"#, "page_from"),
            (r#"{"page_from":1.0}"#, "page_from"),
            (r#"{"page_to":null}"#, "page_to"),
            (r#"{"page_to":"3"}"#, "page_to"),
            (r#"{"page_breaks":1}"#, "page_breaks"),
            (r#"{"strip_headers_footers":"yes"}"#, "strip_headers_footers"),
            (r#"{"page_from":5,"page_to":2}"#, "page_from"),
        ] {
            assert_eq!(p(s).unwrap_err().field, f, "{s}");
        }
    }

    #[test]
    fn oversized_options_rejected() {
        let big = vec![b' '; MAX_OPTIONS_BYTES + 1];
        assert_eq!(parse_options(Some(&big)).unwrap_err().field, "options");
        let ok = format!("{{}}{}", " ".repeat(MAX_OPTIONS_BYTES - 2));
        assert!(parse_options(Some(ok.as_bytes())).is_ok());
    }

    #[test]
    fn range_resolution() {
        assert_eq!(resolve_range(11, None, 10, 300), Err(RangeError::FromBeyondTotal(10)));
        assert_eq!(resolve_range(1, None, 10, 300).unwrap(), (1, 10, None));
        let (f, l, w) = resolve_range(2, Some(50), 10, 300).unwrap();
        assert_eq!((f, l), (2, 10));
        assert!(w.is_some());
        assert!(resolve_range(1, Some(300), 300, 300).is_ok());
        assert_eq!(resolve_range(1, None, 301, 300), Err(RangeError::TooManyPages));
        assert_eq!(resolve_range(2, None, 301, 300).unwrap(), (2, 301, None));
    }
}

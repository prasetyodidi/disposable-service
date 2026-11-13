//! JWT inspector: decodes header/payload locally, optionally verifies an HMAC
//! signature and redacts identity claims. Never logs token, payload or secret.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, SecondsFormat, Utc};
use disposable_common::{ClaimTime, JwtInspectResult, SignatureCheck, TokenStatus};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Sha256, Sha384, Sha512};

pub const MAX_TOKEN_CHARS: usize = 8192;
pub const MAX_SECRET_BYTES: usize = 1024;

const TIME_CLAIMS: [&str; 3] = ["exp", "iat", "nbf"];

pub struct InspectOptions<'a> {
    pub hmac_secret: Option<&'a [u8]>,
    pub redact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    Header,
    Payload,
    Signature,
}

impl Segment {
    pub fn name(self) -> &'static str {
        match self {
            Segment::Header => "header",
            Segment::Payload => "payload",
            Segment::Signature => "signature",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JwtError {
    TokenLength,
    Structure,
    Segment(Segment),
    SecretLength,
}

impl std::fmt::Display for JwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JwtError::TokenLength => write!(
                f,
                "Panjang token harus 1 sampai {} karakter",
                MAX_TOKEN_CHARS
            ),
            JwtError::Structure => {
                write!(f, "Token harus terdiri dari tepat tiga segmen dipisah titik")
            }
            JwtError::Segment(s) => write!(f, "Segmen {} tidak valid", s.name()),
            JwtError::SecretLength => write!(
                f,
                "Panjang secret harus 1 sampai {} byte",
                MAX_SECRET_BYTES
            ),
        }
    }
}

impl std::error::Error for JwtError {}

fn decode_json_object(seg: &str, which: Segment) -> Result<Value, JwtError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(seg)
        .map_err(|_| JwtError::Segment(which))?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| JwtError::Segment(which))?;
    if v.is_object() {
        Ok(v)
    } else {
        Err(JwtError::Segment(which))
    }
}

fn claim_time(payload: &Value, claim: &str) -> Option<(ClaimTime, Option<i64>)> {
    let raw = payload.get(claim)?.clone();
    let secs = raw
        .as_number()
        .and_then(|n| n.as_i64())
        .filter(|s| DateTime::<Utc>::from_timestamp(*s, 0).is_some());
    let iso = secs.and_then(|s| {
        DateTime::<Utc>::from_timestamp(s, 0)
            .map(|d| d.to_rfc3339_opts(SecondsFormat::Secs, true))
    });
    let valid = iso.is_some();
    Some((
        ClaimTime {
            claim: claim.to_string(),
            raw,
            iso,
            valid,
        },
        secs,
    ))
}

fn verify_hmac(alg: &str, secret: &[u8], signing_input: &[u8], sig: &[u8]) -> Option<bool> {
    macro_rules! check {
        ($h:ty) => {{
            let mut mac = <Hmac<$h> as Mac>::new_from_slice(secret).ok()?;
            mac.update(signing_input);
            Some(mac.verify_slice(sig).is_ok())
        }};
    }
    match alg {
        "HS256" => check!(Sha256),
        "HS384" => check!(Sha384),
        "HS512" => check!(Sha512),
        _ => None,
    }
}

fn redaction_category(key: &str) -> Option<&'static str> {
    match key {
        "email" => Some("email"),
        "phone_number" | "phone" => Some("phone"),
        "sub" | "name" | "preferred_username" => Some("identity"),
        _ => None,
    }
}

fn redact_value(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if let Some(cat) = redaction_category(k) {
                    *val = Value::String(format!("[REDACTED:{}]", cat));
                } else {
                    redact_value(val);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_value),
        _ => {}
    }
}

pub fn inspect(
    token: &str,
    opts: &InspectOptions,
    now_unix: i64,
) -> Result<JwtInspectResult, JwtError> {
    let len = token.chars().count();
    if len == 0 || len > MAX_TOKEN_CHARS {
        return Err(JwtError::TokenLength);
    }
    if let Some(s) = opts.hmac_secret {
        if s.is_empty() || s.len() > MAX_SECRET_BYTES {
            return Err(JwtError::SecretLength);
        }
    }

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::Structure);
    }
    let header = decode_json_object(parts[0], Segment::Header)?;
    let mut payload = decode_json_object(parts[1], Segment::Payload)?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|_| JwtError::Segment(Segment::Signature))?;

    // Time claims (computed from the original payload).
    let mut times = Vec::new();
    let (mut exp, mut nbf) = (None, None);
    for claim in TIME_CLAIMS {
        if let Some((ct, secs)) = claim_time(&payload, claim) {
            match claim {
                "exp" => exp = secs,
                "nbf" => nbf = secs,
                _ => {}
            }
            times.push(ct);
        }
    }
    let status = if exp.is_some_and(|e| now_unix >= e) {
        TokenStatus::Expired
    } else if nbf.is_some_and(|n| now_unix < n) {
        TokenStatus::NotYetValid
    } else {
        TokenStatus::Active
    };

    let alg = header.get("alg").and_then(|a| a.as_str());
    let mut warnings = Vec::new();
    if alg.is_some_and(|a| a.eq_ignore_ascii_case("none")) {
        warnings.push("Token tidak ditandatangani (alg: none)".to_string());
    }

    let signature = match opts.hmac_secret {
        None => SignatureCheck::NotRequested,
        Some(secret) => {
            let signing_input = format!("{}.{}", parts[0], parts[1]);
            match alg.and_then(|a| verify_hmac(a, secret, signing_input.as_bytes(), &sig_bytes)) {
                Some(true) => SignatureCheck::Valid,
                Some(false) => SignatureCheck::Invalid,
                None => SignatureCheck::Unsupported,
            }
        }
    };

    if opts.redact {
        redact_value(&mut payload);
    }

    let header_pretty = serde_json::to_string_pretty(&header).unwrap_or_default();
    let payload_pretty = serde_json::to_string_pretty(&payload).unwrap_or_default();

    Ok(JwtInspectResult {
        header,
        payload,
        header_pretty,
        payload_pretty,
        times,
        status,
        signature,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(v: &Value) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(v).unwrap())
    }

    fn make(header: Value, payload: Value, secret: &[u8]) -> String {
        let input = format!("{}.{}", b64(&header), b64(&payload));
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).unwrap();
        mac.update(input.as_bytes());
        format!("{}.{}", input, URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
    }

    fn opts<'a>(secret: Option<&'a [u8]>, redact: bool) -> InspectOptions<'a> {
        InspectOptions { hmac_secret: secret, redact }
    }

    #[test]
    fn decodes_verifies_and_status() {
        let t = make(
            serde_json::json!({"alg":"HS256","typ":"JWT"}),
            serde_json::json!({"exp": 100, "nbf": 200, "iat": "x"}),
            b"s3cret",
        );
        let r = inspect(&t, &opts(Some(b"s3cret"), false), 150).unwrap();
        assert_eq!(r.signature, SignatureCheck::Valid);
        assert_eq!(r.status, TokenStatus::Expired);
        assert_eq!(r.times.len(), 3);
        assert!(r.times.iter().any(|c| c.claim == "iat" && !c.valid));
        assert!(r.header_pretty.contains("\n  \"alg\""));
        let bad = inspect(&t, &opts(Some(b"other"), false), 50).unwrap();
        assert_eq!(bad.signature, SignatureCheck::Invalid);
        assert_eq!(bad.status, TokenStatus::NotYetValid);
    }

    #[test]
    fn errors_name_segments() {
        assert_eq!(inspect("", &opts(None, false), 0).unwrap_err(), JwtError::TokenLength);
        assert_eq!(inspect("a.b", &opts(None, false), 0).unwrap_err(), JwtError::Structure);
        assert_eq!(
            inspect("!!.e30.", &opts(None, false), 0).unwrap_err(),
            JwtError::Segment(Segment::Header)
        );
        assert_eq!(
            inspect("e30.!!.", &opts(None, false), 0).unwrap_err(),
            JwtError::Segment(Segment::Payload)
        );
        assert_eq!(
            inspect("e30.e30.!", &opts(None, false), 0).unwrap_err(),
            JwtError::Segment(Segment::Signature)
        );
    }

    #[test]
    fn none_alg_unsupported_and_redaction() {
        let t = format!(
            "{}.{}.",
            b64(&serde_json::json!({"alg":"NoNe"})),
            b64(&serde_json::json!({"a":{"b":[{"email":"x@y.z"}]},"sub":"u"}))
        );
        let r = inspect(&t, &opts(Some(b"k"), true), 0).unwrap();
        assert_eq!(r.warnings.len(), 1);
        assert_eq!(r.signature, SignatureCheck::Unsupported);
        assert_eq!(r.payload["sub"], "[REDACTED:identity]");
        assert_eq!(r.payload["a"]["b"][0]["email"], "[REDACTED:email]");
    }
}

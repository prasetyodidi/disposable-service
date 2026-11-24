//! proptest generators for the JWT inspector (`privacy/jwt_inspector.rs`).
//!
//! Provides random header/payload JSON (nesting depth <= 5), random time
//! claims, identity keys placed at random depth, real HMAC signing
//! (HS256/384/512) and segment corrupters. Helpers are `pub(crate)` so the
//! property tests can reuse them.
#![allow(dead_code)]

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use proptest::prelude::*;
use serde_json::{Map, Value};
use sha2::{Sha256, Sha384, Sha512};

use crate::privacy::jwt_inspector::Segment;

/// Maximum nesting depth of generated JSON values.
pub(crate) const MAX_JSON_DEPTH: u32 = 5;

/// Keys redacted by the inspector, mapped to their category.
pub(crate) const IDENTITY_KEYS: [(&str, &str); 6] = [
    ("email", "email"),
    ("phone_number", "phone"),
    ("phone", "phone"),
    ("sub", "identity"),
    ("name", "identity"),
    ("preferred_username", "identity"),
];

// ---------------------------------------------------------------------------
// Algorithms and signing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HmacAlg {
    Hs256,
    Hs384,
    Hs512,
}

impl HmacAlg {
    pub(crate) fn name(self) -> &'static str {
        match self {
            HmacAlg::Hs256 => "HS256",
            HmacAlg::Hs384 => "HS384",
            HmacAlg::Hs512 => "HS512",
        }
    }
}

pub(crate) fn hmac_alg() -> impl Strategy<Value = HmacAlg> {
    prop_oneof![
        Just(HmacAlg::Hs256),
        Just(HmacAlg::Hs384),
        Just(HmacAlg::Hs512)
    ]
}

/// Real HMAC over `input` with the given algorithm.
pub(crate) fn hmac_sign(alg: HmacAlg, secret: &[u8], input: &[u8]) -> Vec<u8> {
    macro_rules! sign {
        ($h:ty) => {{
            let mut mac = <Hmac<$h> as Mac>::new_from_slice(secret).expect("any key length");
            mac.update(input);
            mac.finalize().into_bytes().to_vec()
        }};
    }
    match alg {
        HmacAlg::Hs256 => sign!(Sha256),
        HmacAlg::Hs384 => sign!(Sha384),
        HmacAlg::Hs512 => sign!(Sha512),
    }
}

/// Secret of 1..=64 bytes (within the inspector's 1..=1024 limit).
pub(crate) fn secret_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..=64)
}

// ---------------------------------------------------------------------------
// Token assembly
// ---------------------------------------------------------------------------

pub(crate) fn b64_json(v: &Value) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(v).expect("serializable"))
}

/// Builds `header.payload.signature` signed with real HMAC.
pub(crate) fn build_signed_token(
    header: &Value,
    payload: &Value,
    alg: HmacAlg,
    secret: &[u8],
) -> String {
    let input = format!("{}.{}", b64_json(header), b64_json(payload));
    let sig = hmac_sign(alg, secret, input.as_bytes());
    format!("{}.{}", input, URL_SAFE_NO_PAD.encode(sig))
}

// ---------------------------------------------------------------------------
// Random JSON
// ---------------------------------------------------------------------------

/// Generic object key; never collides with identity or time claim names.
fn generic_key() -> impl Strategy<Value = String> {
    "k_[a-z0-9]{1,6}"
}

/// Random JSON value with nesting depth <= `MAX_JSON_DEPTH`. Floats are
/// excluded so values round-trip exactly; object keys are generic only.
pub(crate) fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (-1_000_000i64..1_000_000).prop_map(|n| Value::from(n)),
        "[ -~]{0,16}".prop_map(Value::String),
    ];
    leaf.prop_recursive(MAX_JSON_DEPTH, 48, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map(generic_key(), inner, 0..4)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

/// Random JSON object (depth <= 5) with generic keys only.
pub(crate) fn json_object() -> impl Strategy<Value = Map<String, Value>> {
    prop::collection::btree_map(generic_key(), json_value(), 0..5)
        .prop_map(|m| m.into_iter().collect())
}

/// Random header object whose `alg` is `alg`.
pub(crate) fn header_for(alg: HmacAlg) -> impl Strategy<Value = Value> {
    json_object().prop_map(move |mut m| {
        m.insert("alg".into(), Value::String(alg.name().into()));
        m.insert("typ".into(), Value::String("JWT".into()));
        Value::Object(m)
    })
}

// ---------------------------------------------------------------------------
// Time claims
// ---------------------------------------------------------------------------

/// Upper bound (year 2100) for valid generated timestamps.
pub(crate) const MAX_TS: i64 = 4_102_444_800;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TimeVal {
    /// Integer seconds the inspector can convert to ISO-8601.
    Valid(i64),
    /// Non-convertible raw value (string, bool, null, out-of-range number).
    Invalid(Value),
}

impl TimeVal {
    pub(crate) fn to_json(&self) -> Value {
        match self {
            TimeVal::Valid(s) => Value::from(*s),
            TimeVal::Invalid(v) => v.clone(),
        }
    }
    pub(crate) fn seconds(&self) -> Option<i64> {
        match self {
            TimeVal::Valid(s) => Some(*s),
            TimeVal::Invalid(_) => None,
        }
    }
}

pub(crate) fn time_val() -> impl Strategy<Value = TimeVal> {
    prop_oneof![
        4 => (0i64..=MAX_TS).prop_map(TimeVal::Valid),
        1 => prop_oneof![
            "[a-z]{1,6}".prop_map(Value::String),
            any::<bool>().prop_map(Value::Bool),
            Just(Value::Null),
            Just(Value::from(i64::MAX)),
            Just(Value::from(i64::MIN)),
        ]
        .prop_map(TimeVal::Invalid),
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimeClaims {
    pub exp: Option<TimeVal>,
    pub iat: Option<TimeVal>,
    pub nbf: Option<TimeVal>,
}

impl TimeClaims {
    /// Present claims as `(name, value)` pairs.
    pub(crate) fn present(&self) -> Vec<(&'static str, &TimeVal)> {
        [("exp", &self.exp), ("iat", &self.iat), ("nbf", &self.nbf)]
            .into_iter()
            .filter_map(|(n, v)| v.as_ref().map(|v| (n, v)))
            .collect()
    }

    pub(crate) fn insert_into(&self, obj: &mut Map<String, Value>) {
        for (name, v) in self.present() {
            obj.insert(name.into(), v.to_json());
        }
    }
}

pub(crate) fn time_claims() -> impl Strategy<Value = TimeClaims> {
    (
        prop::option::of(time_val()),
        prop::option::of(time_val()),
        prop::option::of(time_val()),
    )
        .prop_map(|(exp, iat, nbf)| TimeClaims { exp, iat, nbf })
}

/// "Current time" fed to `inspect`.
pub(crate) fn now_unix() -> impl Strategy<Value = i64> {
    0i64..=MAX_TS
}

// ---------------------------------------------------------------------------
// Identity keys at random depth
// ---------------------------------------------------------------------------

/// An identity key planted in the payload with its distinctive plain value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlantedIdentity {
    pub key: &'static str,
    pub category: &'static str,
    pub value: String,
    /// Number of container levels above the key (0 = top-level key).
    pub depth: usize,
}

pub(crate) fn identity_key() -> impl Strategy<Value = (&'static str, &'static str)> {
    prop::sample::select(IDENTITY_KEYS.to_vec())
}

/// Wraps `{key: value}` in `depth` random object/array layers and returns the
/// resulting value plus the planted record. `depth` <= 4 so the total JSON
/// depth (with the surrounding payload) stays <= 5.
fn plant(
    key: &'static str,
    category: &'static str,
    value: String,
    layers: Vec<bool>,
) -> (Value, PlantedIdentity) {
    let mut inner = {
        let mut m = Map::new();
        m.insert(key.into(), Value::String(value.clone()));
        Value::Object(m)
    };
    let depth = layers.len();
    for is_array in layers {
        inner = if is_array {
            Value::Array(vec![inner])
        } else {
            let mut m = Map::new();
            m.insert("k_wrap".into(), inner);
            Value::Object(m)
        };
    }
    (
        inner,
        PlantedIdentity {
            key,
            category,
            value,
            depth,
        },
    )
}

/// Payload plus the identity keys planted in it. Each planted key lives under
/// its own top-level slot (`k_id0`, `k_id1`, ...), at random depth 0..=4.
#[derive(Debug, Clone)]
pub(crate) struct PayloadCase {
    pub payload: Value,
    pub time: TimeClaims,
    pub planted: Vec<PlantedIdentity>,
}

pub(crate) fn payload_case() -> impl Strategy<Value = PayloadCase> {
    let planted = prop::collection::vec(
        (
            identity_key(),
            "id-[a-z0-9]{6,12}",
            prop::collection::vec(any::<bool>(), 0..=4),
        ),
        0..4,
    );
    (json_object(), time_claims(), planted).prop_map(|(mut obj, time, planted)| {
        let mut records = Vec::new();
        for (i, ((key, cat), value, layers)) in planted.into_iter().enumerate() {
            let (v, rec) = plant(key, cat, value, layers);
            obj.insert(format!("k_id{i}"), v);
            records.push(rec);
        }
        time.insert_into(&mut obj);
        PayloadCase {
            payload: Value::Object(obj),
            time,
            planted: records,
        }
    })
}

// ---------------------------------------------------------------------------
// Full signed-token case
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct JwtCase {
    pub header: Value,
    pub payload: PayloadCase,
    pub alg: HmacAlg,
    pub secret: Vec<u8>,
    pub token: String,
    pub now: i64,
}

/// Well-formed token signed with a real HMAC (HS256/384/512).
pub(crate) fn jwt_case() -> impl Strategy<Value = JwtCase> {
    (hmac_alg(), secret_bytes(), payload_case(), now_unix()).prop_flat_map(
        |(alg, secret, payload, now)| {
            header_for(alg).prop_map(move |header| {
                let token = build_signed_token(&header, &payload.payload, alg, &secret);
                JwtCase {
                    header,
                    payload: payload.clone(),
                    alg,
                    secret: secret.clone(),
                    token,
                    now,
                }
            })
        },
    )
}

// ---------------------------------------------------------------------------
// Segment corrupters
// ---------------------------------------------------------------------------

/// Ways to make a single segment undecodable for the inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Corruption {
    /// Characters outside the base64url alphabet.
    BadBase64,
    /// Valid base64url of bytes that are not JSON.
    NotJson,
    /// Valid base64url of JSON that is not an object (header/payload only).
    NotObject,
}

pub(crate) fn corruption_for(seg: Segment) -> BoxedStrategy<Corruption> {
    match seg {
        Segment::Signature => Just(Corruption::BadBase64).boxed(),
        _ => prop_oneof![
            Just(Corruption::BadBase64),
            Just(Corruption::NotJson),
            Just(Corruption::NotObject)
        ]
        .boxed(),
    }
}

fn corrupt_segment_text(c: Corruption, non_object: &Value) -> String {
    match c {
        Corruption::BadBase64 => "!!@@##".to_string(),
        Corruption::NotJson => URL_SAFE_NO_PAD.encode(b"\xff\xfe not json"),
        Corruption::NotObject => b64_json(non_object),
    }
}

fn non_object_json() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(Value::from),
        "[a-z]{0,8}".prop_map(Value::String),
        prop::collection::vec(any::<i32>().prop_map(Value::from), 0..3).prop_map(Value::Array),
    ]
}

/// Replaces one segment of `token` per `how`, leaving the others intact.
pub(crate) fn corrupt_token(token: &str, seg: Segment, how: Corruption, non_object: &Value) -> String {
    let mut parts: Vec<String> = token.split('.').map(str::to_string).collect();
    assert_eq!(parts.len(), 3, "corrupt_token expects a well-formed token");
    let idx = match seg {
        Segment::Header => 0,
        Segment::Payload => 1,
        Segment::Signature => 2,
    };
    parts[idx] = corrupt_segment_text(how, non_object);
    parts.join(".")
}

/// A well-formed token with exactly one segment corrupted.
#[derive(Debug, Clone)]
pub(crate) struct CorruptedToken {
    pub token: String,
    pub segment: Segment,
    pub how: Corruption,
}

fn any_segment() -> impl Strategy<Value = Segment> {
    prop_oneof![
        Just(Segment::Header),
        Just(Segment::Payload),
        Just(Segment::Signature)
    ]
}

/// Corrupts a single segment. When several segments are bad the inspector
/// reports the first failing one in header, payload, signature order; since
/// only one is corrupted here the expected segment is `segment`.
pub(crate) fn corrupted_token() -> impl Strategy<Value = CorruptedToken> {
    (jwt_case(), any_segment(), non_object_json()).prop_flat_map(|(case, seg, non_obj)| {
        corruption_for(seg).prop_map(move |how| CorruptedToken {
            token: corrupt_token(&case.token, seg, how, &non_obj),
            segment: seg,
            how,
        })
    })
}

/// Tokens that do not consist of exactly three dot-separated segments.
pub(crate) fn malformed_structure_token() -> impl Strategy<Value = String> {
    prop_oneof![
        // 1 segment, 2 segments, or 4+ segments
        prop::collection::vec("[A-Za-z0-9_-]{0,12}", 1..=2).prop_map(|p| p.join(".")),
        prop::collection::vec("[A-Za-z0-9_-]{0,12}", 4..=6).prop_map(|p| p.join(".")),
    ]
}

#[cfg(test)]
mod self_tests {
    use super::*;
    use crate::privacy::jwt_inspector::{inspect, InspectOptions, JwtError};
    use disposable_common::SignatureCheck;

    fn depth(v: &Value) -> usize {
        match v {
            Value::Array(a) => 1 + a.iter().map(depth).max().unwrap_or(0),
            Value::Object(m) => 1 + m.values().map(depth).max().unwrap_or(0),
            _ => 0,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn generated_tokens_decode_and_verify(case in jwt_case()) {
            prop_assert!(depth(&case.header) <= 1 + MAX_JSON_DEPTH as usize);
            let opts = InspectOptions { hmac_secret: Some(&case.secret), redact: false };
            let r = inspect(&case.token, &opts, case.now).unwrap();
            prop_assert_eq!(r.signature, SignatureCheck::Valid);
            prop_assert_eq!(r.payload, case.payload.payload);
        }

        #[test]
        fn corrupted_tokens_name_the_segment(c in corrupted_token()) {
            let opts = InspectOptions { hmac_secret: None, redact: false };
            prop_assert_eq!(
                inspect(&c.token, &opts, 0).unwrap_err(),
                JwtError::Segment(c.segment)
            );
        }

        #[test]
        fn malformed_structure_rejected(t in malformed_structure_token()) {
            prop_assume!(!t.is_empty());
            let opts = InspectOptions { hmac_secret: None, redact: false };
            prop_assert_eq!(inspect(&t, &opts, 0).unwrap_err(), JwtError::Structure);
        }
    }
}

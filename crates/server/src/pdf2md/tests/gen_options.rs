//! proptest strategies for conversion options JSON (`pdf2md/options.rs`).
#![allow(dead_code)]
use proptest::prelude::*;
use serde_json::{json, Map, Value};

use crate::pdf2md::options::{ConversionOptions, MAX_OPTIONS_BYTES};

/// Valid options JSON and the `ConversionOptions` it must parse to.
#[derive(Debug, Clone)]
pub(crate) struct ValidOptions {
    pub raw: Vec<u8>,
    pub expected: ConversionOptions,
}

/// Invalid options JSON and the field the error must name.
#[derive(Debug, Clone)]
pub(crate) struct InvalidOptions {
    pub raw: Vec<u8>,
    pub field: String,
}

pub(crate) fn arb_valid_options() -> impl Strategy<Value = ValidOptions> {
    (
        prop::option::of(1..=1000u32),
        prop::option::of(0..=1000u32),
        prop::option::of(any::<bool>()),
        prop::option::of(any::<bool>()),
    )
        .prop_map(|(from, to_delta, breaks, strip)| {
            let mut obj = Map::new();
            let mut expected = ConversionOptions::default();
            if let Some(f) = from {
                obj.insert("page_from".into(), json!(f));
                expected.page_from = f;
            }
            if let Some(d) = to_delta {
                let to = expected.page_from + d;
                obj.insert("page_to".into(), json!(to));
                expected.page_to = Some(to);
            }
            if let Some(b) = breaks {
                obj.insert("page_breaks".into(), json!(b));
                expected.page_breaks = b;
            }
            if let Some(s) = strip {
                obj.insert("strip_headers_footers".into(), json!(s));
                expected.strip_headers_footers = s;
            }
            ValidOptions { raw: Value::Object(obj).to_string().into_bytes(), expected }
        })
}

fn bad_number() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(json!(0)),
        (1..=1000i64).prop_map(|n| json!(-n)),
        (1..=1000u32).prop_map(|n| json!(f64::from(n) + 0.5)),
        Just(json!(1.0)),
        Just(json!("3")),
        Just(json!(true)),
        Just(json!(null)),
        Just(json!([1])),
    ]
}

fn bad_bool() -> impl Strategy<Value = Value> {
    prop_oneof![Just(json!(1)), Just(json!(0)), Just(json!("yes")), Just(json!(null)), Just(json!([true]))]
}

fn single(key: &str, v: Value, breaks: bool) -> Vec<u8> {
    let mut obj = Map::new();
    if breaks {
        obj.insert("page_breaks".into(), json!(true));
    }
    obj.insert(key.into(), v);
    Value::Object(obj).to_string().into_bytes()
}

/// Exactly one mutation of an otherwise valid options object.
pub(crate) fn arb_invalid_options() -> impl Strategy<Value = InvalidOptions> {
    let bad_page = (prop::sample::select(vec!["page_from", "page_to"]), bad_number(), any::<bool>())
        .prop_map(|(k, v, b)| InvalidOptions { raw: single(k, v, b), field: k.into() });
    let bad_flag = (prop::sample::select(vec!["page_breaks", "strip_headers_footers"]), bad_bool())
        .prop_map(|(k, v)| InvalidOptions { raw: single(k, v, false), field: k.into() });
    let unknown = "[a-z]{3,10}".prop_map(|s| {
        let k = format!("x_{s}");
        InvalidOptions { raw: single(&k, json!(1), true), field: k }
    });
    let from_gt_to = (2..=1000u32).prop_flat_map(|from| (Just(from), 1..from)).prop_map(|(from, to)| {
        InvalidOptions {
            raw: json!({"page_from": from, "page_to": to}).to_string().into_bytes(),
            field: "page_from".into(),
        }
    });
    let whole = prop_oneof![
        Just(b"not json".to_vec()),
        Just(b"{\"page_from\":".to_vec()),
        Just(b"[1]".to_vec()),
        Just(b"\"text\"".to_vec()),
        Just(b"42".to_vec()),
        Just(b"null".to_vec()),
        Just(vec![b' '; MAX_OPTIONS_BYTES + 1]),
    ]
    .prop_map(|raw| InvalidOptions { raw, field: "options".into() });
    prop_oneof![bad_page, bad_flag, unknown, from_gt_to, whole]
}

#[cfg(test)]
mod self_checks {
    use super::*;
    use crate::pdf2md::options::parse_options;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn valid_options_parse_to_expected(v in arb_valid_options()) {
            prop_assert_eq!(parse_options(Some(&v.raw)).unwrap(), v.expected);
        }

        #[test]
        fn invalid_options_name_the_field(i in arb_invalid_options()) {
            let e = parse_options(Some(&i.raw)).unwrap_err();
            prop_assert_eq!(e.field, i.field);
        }
    }
}

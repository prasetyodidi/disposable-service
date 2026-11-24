// Feature: privacy-micro-apps-and-burn-workspace, Property 19: Peringatan algoritma none
//! **Validates: Requirements 6.4**

use proptest::prelude::*;
use serde_json::Value;

use super::gen_jwt::{b64_json, json_object};
use crate::privacy::jwt_inspector::{inspect, InspectOptions};

const WARNING: &str = "Token tidak ditandatangani (alg: none)";

/// Any casing of "none".
fn none_variant() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<bool>(), 4).prop_map(|flags| {
        "none"
            .chars()
            .zip(flags)
            .map(|(c, up)| if up { c.to_ascii_uppercase() } else { c })
            .collect()
    })
}

/// Any alg string that is not "none" in any casing.
fn other_alg() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("HS256".to_string()),
        Just("HS384".to_string()),
        Just("HS512".to_string()),
        Just("RS256".to_string()),
        Just("ES256".to_string()),
        "[ -~]{0,10}".prop_filter("not none", |s| !s.eq_ignore_ascii_case("none")),
    ]
}

fn token_with_alg(alg: &str, extra: serde_json::Map<String, Value>) -> String {
    let mut h = extra;
    h.insert("alg".into(), Value::String(alg.to_string()));
    format!(
        "{}.{}.",
        b64_json(&Value::Object(h)),
        b64_json(&serde_json::json!({}))
    )
}

fn warned(token: &str) -> bool {
    let opts = InspectOptions { hmac_secret: None, redact: false };
    inspect(token, &opts, 0)
        .unwrap()
        .warnings
        .iter()
        .any(|w| w == WARNING)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn none_alg_any_case_warns(alg in none_variant(), extra in json_object()) {
        prop_assert!(warned(&token_with_alg(&alg, extra)));
    }

    #[test]
    fn other_alg_does_not_warn(alg in other_alg(), extra in json_object()) {
        prop_assert!(!warned(&token_with_alg(&alg, extra)));
    }
}

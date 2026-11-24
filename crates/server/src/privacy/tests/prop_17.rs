// Feature: privacy-micro-apps-and-burn-workspace, Property 17: Round trip inspeksi JWT
//! **Validates: Requirements 6.1, 6.12**

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use proptest::prelude::*;
use serde_json::Value;

use super::gen_jwt::jwt_case;
use crate::privacy::jwt_inspector::{inspect, InspectOptions};

fn reencode_roundtrip(v: &Value) -> Value {
    let enc = URL_SAFE_NO_PAD.encode(serde_json::to_vec(v).unwrap());
    let bytes = URL_SAFE_NO_PAD.decode(enc).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn jwt_inspection_round_trip(case in jwt_case()) {
        let opts = InspectOptions { hmac_secret: None, redact: false };
        let r = inspect(&case.token, &opts, case.now).unwrap();

        // Semantically equivalent to the input.
        prop_assert_eq!(&r.header, &case.header);
        prop_assert_eq!(&r.payload, &case.payload.payload);

        // Pretty output: 2-space indent and parses back to the same value.
        for (pretty, orig) in [(&r.header_pretty, &case.header), (&r.payload_pretty, &case.payload.payload)] {
            let parsed: Value = serde_json::from_str(pretty).unwrap();
            prop_assert_eq!(&parsed, orig);
            for line in pretty.lines() {
                let indent = line.len() - line.trim_start_matches(' ').len();
                prop_assert_eq!(indent % 2, 0);
            }
            if orig.as_object().is_some_and(|m| !m.is_empty()) {
                prop_assert!(pretty.contains("\n  \""));
            }
        }

        // Re-encode to base64url and decode again.
        prop_assert_eq!(reencode_roundtrip(&r.header), case.header.clone());
        prop_assert_eq!(reencode_roundtrip(&r.payload), case.payload.payload.clone());
    }
}

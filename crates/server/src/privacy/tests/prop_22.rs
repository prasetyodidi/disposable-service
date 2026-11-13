// Feature: privacy-micro-apps-and-burn-workspace, Property 22: Redaksi klaim identitas JWT
//! **Validates: Requirements 6.9**

use proptest::prelude::*;
use serde_json::Value;

use super::gen_jwt::{jwt_case, IDENTITY_KEYS};
use crate::privacy::jwt_inspector::{inspect, InspectOptions};

/// Independent oracle: replace the value of every identity key at any depth.
fn expected_redacted(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, val)| {
                    match IDENTITY_KEYS.iter().find(|(key, _)| key == k) {
                        Some((_, cat)) => (k.clone(), Value::String(format!("[REDACTED:{cat}]"))),
                        None => (k.clone(), expected_redacted(val)),
                    }
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(expected_redacted).collect()),
        other => other.clone(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn identity_claims_are_redacted_and_others_untouched(case in jwt_case()) {
        let opts = InspectOptions { hmac_secret: None, redact: true };
        let r = inspect(&case.token, &opts, case.now).unwrap();

        // Identity keys replaced, every other key unchanged.
        prop_assert_eq!(&r.payload, &expected_redacted(&case.payload.payload));

        // Original identity values no longer appear in the output.
        for p in &case.payload.planted {
            prop_assert!(!r.payload_pretty.contains(&p.value));
        }

        // Without redaction the payload is returned verbatim.
        let plain = inspect(
            &case.token,
            &InspectOptions { hmac_secret: None, redact: false },
            case.now,
        )
        .unwrap();
        prop_assert_eq!(plain.payload, case.payload.payload);
    }
}
